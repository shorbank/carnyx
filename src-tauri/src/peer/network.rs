//! Connection establishment and resource ownership. No SQL, Tauri, or message policy.
use super::{
    PeerError, Result,
    transport::Stream,
    types::{
        ConnectionMode, ConnectionPath, ConnectionTarget, Invitation, Role, SessionSettings,
        internet_relay,
    },
};
use crate::identity::PeerId;
use futures_util::StreamExt;
use iroh::{
    Endpoint, EndpointAddr, RelayMode,
    endpoint::{Connection, QuicTransportConfig, presets},
};
use std::{net::SocketAddrV4, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
    time::{Duration, timeout},
};

pub const ALPN: &[u8] = b"carnyx/chat/1";
fn network(error: impl std::fmt::Display) -> PeerError {
    PeerError::new("network_error", error)
}
pub struct Connector {
    pub role: Role,
    config: Config,
}
#[derive(Clone)]
pub struct NetworkConfig {
    relay: iroh::RelayUrl,
    direct: bool,
}
impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            relay: internet_relay(),
            direct: true,
        }
    }
}
#[cfg(test)]
impl NetworkConfig {
    pub fn local(relay: iroh::RelayUrl, direct: bool) -> Self {
        Self { relay, direct }
    }
}
enum Config {
    Lan {
        remote: SocketAddrV4,
        local: SocketAddrV4,
        listener: Option<TcpListener>,
    },
    Internet {
        secret: iroh::SecretKey,
        remote: EndpointAddr,
        endpoint: Option<Endpoint>,
        config: NetworkConfig,
    },
}
pub struct Established {
    pub stream: Stream,
    pub lease: ConnectionLease,
}
pub struct ConnectionLease(Option<Connection>);
pub struct PathObserver {
    task: Option<JoinHandle<()>>,
    connection: Option<Connection>,
}
impl Drop for PathObserver {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        if let Some(connection) = self.connection.take() {
            connection.close(0u8.into(), b"Carnyx session ended");
        }
    }
}
impl Drop for ConnectionLease {
    fn drop(&mut self) {
        if let Some(connection) = self.0.take() {
            connection.close(0u8.into(), b"Carnyx handshake ended");
        }
    }
}
impl ConnectionLease {
    pub fn observe(mut self, changed: Arc<dyn Fn(ConnectionPath) + Send + Sync>) -> PathObserver {
        let connection = self.0.take();
        let task = connection.as_ref().map(|connection| {
            let changed = changed.clone();
            let connection = connection.clone();
            tokio::spawn(async move {
                let mut paths = connection.paths_stream();
                while let Some(snapshot) = paths.next().await {
                    let selected = snapshot.iter().find(|p| p.is_selected());
                    let path = match selected {
                        Some(p) if p.remote_addr().is_ip() => ConnectionPath::Direct,
                        Some(p) if p.remote_addr().is_relay() => ConnectionPath::Relay,
                        _ => ConnectionPath::Unknown,
                    };
                    changed(path);
                }
            })
        });
        if connection.is_none() {
            changed(ConnectionPath::Direct);
        }
        PathObserver { task, connection }
    }
}
impl Connector {
    pub async fn new(
        own: PeerId,
        peer: &Invitation,
        settings: &SessionSettings,
        secret: Option<iroh::SecretKey>,
        options: &NetworkConfig,
    ) -> Result<Self> {
        settings.validate_for(peer)?;
        peer.validate(true)?;
        let role = settings.role(own, peer);
        let config = match (&peer.target, settings) {
            (ConnectionTarget::Lan { address }, SessionSettings::Lan { .. }) => {
                let local = settings.endpoint()?;
                let listener = if role == Role::Responder {
                    Some(TcpListener::bind(local).await?)
                } else {
                    None
                };
                Config::Lan {
                    remote: *address,
                    local,
                    listener,
                }
            }
            (ConnectionTarget::Internet { endpoint_id, .. }, SessionSettings::Internet) => {
                Config::Internet {
                    secret: secret
                        .ok_or_else(|| PeerError::new("key_unavailable", "internet key missing"))?,
                    remote: EndpointAddr::new(endpoint_id.parse().map_err(network)?)
                        .with_relay_url(options.relay.clone()),
                    endpoint: None,
                    config: options.clone(),
                }
            }
            _ => return Err(PeerError::new("mode_mismatch", "code and settings differ")),
        };
        Ok(Self { role, config })
    }
    pub fn mode(&self) -> ConnectionMode {
        match self.config {
            Config::Lan { .. } => ConnectionMode::Lan,
            Config::Internet { .. } => ConnectionMode::Internet,
        }
    }
    pub async fn establish(&mut self) -> Result<Established> {
        match &mut self.config {
            Config::Lan {
                remote,
                local,
                listener,
            } => {
                let stream = if self.role == Role::Initiator {
                    timeout(Duration::from_secs(5), TcpStream::connect(*remote))
                        .await
                        .map_err(|_| PeerError::new("timeout", "LAN connect timeout"))??
                } else {
                    if listener.is_none() {
                        *listener = Some(TcpListener::bind(*local).await?);
                    }
                    listener.as_ref().expect("bound listener").accept().await?.0
                };
                stream.set_nodelay(true)?;
                Ok(Established {
                    stream: Box::new(stream),
                    lease: ConnectionLease(None),
                })
            }
            Config::Internet {
                secret,
                remote,
                endpoint,
                config,
            } => {
                if endpoint.is_none() {
                    let mut builder = Endpoint::builder(presets::Minimal)
                        .secret_key(secret.clone())
                        .relay_mode(RelayMode::Custom(
                            [iroh::RelayConfig::from(config.relay.clone())]
                                .into_iter()
                                .collect(),
                        ))
                        .clear_address_lookup()
                        .transport_config(
                            QuicTransportConfig::builder()
                                .max_concurrent_uni_streams(0u8.into())
                                .max_concurrent_bidi_streams(
                                    if self.role == Role::Responder {
                                        1u8
                                    } else {
                                        0u8
                                    }
                                    .into(),
                                )
                                .build(),
                        )
                        .alpns(vec![ALPN.to_vec()]);
                    if !config.direct {
                        builder = builder.clear_ip_transports();
                    }
                    *endpoint = Some(builder.bind().await.map_err(network)?);
                }
                let endpoint = endpoint.as_ref().expect("bound endpoint");
                if self.role == Role::Initiator {
                    timeout(Duration::from_secs(30), async {
                        let connection = endpoint
                            .connect(remote.clone(), ALPN)
                            .await
                            .map_err(network)?;
                        let lease = ConnectionLease(Some(connection));
                        let (send, recv) = lease
                            .0
                            .as_ref()
                            .expect("connected transport")
                            .open_bi()
                            .await
                            .map_err(network)?;
                        Ok(Established {
                            stream: Box::new(tokio::io::join(recv, send)),
                            lease,
                        })
                    })
                    .await
                    .map_err(|_| PeerError::new("timeout", "internet connect timeout"))?
                } else {
                    loop {
                        let incoming = endpoint
                            .accept()
                            .await
                            .ok_or_else(|| network("endpoint closed"))?;
                        // Bound each untrusted handshake. Unknown peers never consume a chat session.
                        let Ok(Ok(connection)) = timeout(Duration::from_secs(10), incoming).await
                        else {
                            continue;
                        };
                        if connection.remote_id() != remote.id {
                            connection.close(1u8.into(), b"Unexpected peer");
                            continue;
                        }
                        match timeout(Duration::from_secs(10), connection.accept_bi()).await {
                            Ok(Ok((send, recv))) => {
                                return Ok(Established {
                                    stream: Box::new(tokio::io::join(recv, send)),
                                    lease: ConnectionLease(Some(connection)),
                                });
                            }
                            _ => {
                                connection.close(1u8.into(), b"Stream timeout");
                            }
                        }
                    }
                }
            }
        }
    }
    pub async fn close(&mut self) {
        match &mut self.config {
            Config::Lan { listener, .. } => {
                listener.take();
            }
            Config::Internet { endpoint, .. } => {
                if let Some(endpoint) = endpoint.take() {
                    endpoint.close().await;
                }
            }
        }
    }
}
