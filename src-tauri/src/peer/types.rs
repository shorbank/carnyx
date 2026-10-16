use super::{PeerError, Result};
use crate::identity::PeerId;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddrV4};

pub const INTERNET_RELAY: &str = "https://euc1-1.relay.n0.iroh.link./";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionMode {
    Lan,
    Internet,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionPath {
    Direct,
    Relay,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "lowercase", deny_unknown_fields)]
pub enum ConnectionTarget {
    Lan {
        address: SocketAddrV4,
    },
    Internet {
        #[serde(rename = "endpointId")]
        endpoint_id: String,
        #[serde(rename = "relayUrl")]
        relay_url: String,
    },
}
impl ConnectionTarget {
    pub fn mode(&self) -> ConnectionMode {
        match self {
            Self::Lan { .. } => ConnectionMode::Lan,
            Self::Internet { .. } => ConnectionMode::Internet,
        }
    }
    fn validate(&self, configured_relay: bool) -> Result<()> {
        match self {
            Self::Lan { address } => validate_address(*address),
            Self::Internet {
                endpoint_id,
                relay_url,
            } => {
                endpoint_id
                    .parse::<iroh::EndpointId>()
                    .map_err(|_| PeerError::new("invalid_code", "invalid internet endpoint"))?;
                let relay = relay_url
                    .parse::<iroh::RelayUrl>()
                    .map_err(|_| PeerError::new("invalid_code", "invalid relay"))?;
                if relay.scheme() != "https" {
                    return Err(PeerError::new("invalid_code", "HTTPS relay required"));
                }
                if configured_relay && relay != internet_relay() {
                    return Err(PeerError::new(
                        "unsupported_relay",
                        "relay differs from configured pilot relay",
                    ));
                }
                Ok(())
            }
        }
    }
}
pub fn internet_relay() -> iroh::RelayUrl {
    INTERNET_RELAY.parse().expect("valid configured relay")
}

#[derive(Clone, Debug)]
pub struct PeerIdentity {
    pub peer: PeerId,
    pub public_key: [u8; 32],
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Invitation {
    pub peer_id: String,
    pub public_key: String,
    pub target: ConnectionTarget,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyInvitation {
    version: u8,
    peer_id: String,
    public_key: String,
    address: SocketAddrV4,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InternetInvitation {
    version: u8,
    peer_id: String,
    public_key: String,
    target: ConnectionTarget,
}
impl Invitation {
    pub fn new(peer: PeerId, public: &[u8; 32], address: SocketAddrV4) -> Self {
        Self {
            peer_id: peer.to_string(),
            public_key: URL_SAFE_NO_PAD.encode(public),
            target: ConnectionTarget::Lan { address },
        }
    }
    pub fn internet(peer: PeerId, public: &[u8; 32], endpoint_id: iroh::EndpointId) -> Self {
        Self {
            peer_id: peer.to_string(),
            public_key: URL_SAFE_NO_PAD.encode(public),
            target: ConnectionTarget::Internet {
                endpoint_id: endpoint_id.to_string(),
                relay_url: INTERNET_RELAY.into(),
            },
        }
    }
    pub fn peer(&self) -> Result<PeerId> {
        self.peer_id
            .parse()
            .map_err(|e| PeerError::new("invalid_code", e))
    }
    pub fn key(&self) -> Result<[u8; 32]> {
        URL_SAFE_NO_PAD
            .decode(&self.public_key)
            .map_err(|e| PeerError::new("invalid_code", e))?
            .try_into()
            .map_err(|_| PeerError::new("invalid_code", "expected 32-byte public key"))
    }
    pub fn identity(&self) -> Result<PeerIdentity> {
        self.validate(false)?;
        Ok(PeerIdentity {
            peer: self.peer()?,
            public_key: self.key()?,
        })
    }
    pub fn validate(&self, configured_relay: bool) -> Result<()> {
        if self.peer()?.to_string() != self.peer_id || self.key()? == [0; 32] {
            return Err(PeerError::new("invalid_code", "invalid identity"));
        }
        self.target.validate(configured_relay)
    }
    #[cfg(test)]
    pub fn lan_address(&self) -> Result<SocketAddrV4> {
        match self.target {
            ConnectionTarget::Lan { address } => Ok(address),
            _ => Err(PeerError::new("mode_mismatch", "LAN target required")),
        }
    }
    pub fn encode(&self) -> String {
        let (version, bytes) = match &self.target {
            ConnectionTarget::Lan { address } => (
                1,
                serde_json::to_vec(&LegacyInvitation {
                    version: 1,
                    peer_id: self.peer_id.clone(),
                    public_key: self.public_key.clone(),
                    address: *address,
                }),
            ),
            ConnectionTarget::Internet { .. } => (
                2,
                serde_json::to_vec(&InternetInvitation {
                    version: 2,
                    peer_id: self.peer_id.clone(),
                    public_key: self.public_key.clone(),
                    target: self.target.clone(),
                }),
            ),
        };
        format!(
            "carnyx:v{version}:{}",
            URL_SAFE_NO_PAD.encode(bytes.expect("invitation serializes"))
        )
    }
    pub fn decode(code: &str) -> Result<Self> {
        if code.len() > 2048 {
            return Err(PeerError::new("invalid_code", "code too long"));
        }
        let code = code.trim();
        let decode = |body: &str| {
            URL_SAFE_NO_PAD
                .decode(body)
                .map_err(|e| PeerError::new("invalid_code", e))
        };
        let invitation = if let Some(body) = code.strip_prefix("carnyx:v1:") {
            let value: LegacyInvitation = serde_json::from_slice(&decode(body)?)
                .map_err(|e| PeerError::new("invalid_code", e))?;
            if value.version != 1 {
                return Err(PeerError::new("invalid_code", "unsupported invitation"));
            }
            Self {
                peer_id: value.peer_id,
                public_key: value.public_key,
                target: ConnectionTarget::Lan {
                    address: value.address,
                },
            }
        } else if let Some(body) = code.strip_prefix("carnyx:v2:") {
            let value: InternetInvitation = serde_json::from_slice(&decode(body)?)
                .map_err(|e| PeerError::new("invalid_code", e))?;
            if value.version != 2 || value.target.mode() != ConnectionMode::Internet {
                return Err(PeerError::new("invalid_code", "unsupported invitation"));
            }
            Self {
                peer_id: value.peer_id,
                public_key: value.public_key,
                target: value.target,
            }
        } else {
            return Err(PeerError::new("invalid_code", "unsupported invitation"));
        };
        invitation.validate(true)?;
        Ok(invitation)
    }
}

pub fn validate_address(address: SocketAddrV4) -> Result<()> {
    let ip = address.ip();
    if address.port() == 0 || ip.is_unspecified() || ip.is_multicast() || ip.is_broadcast() {
        return Err(PeerError::new(
            "invalid_address",
            "expected unicast IPv4 and nonzero port",
        ));
    }
    Ok(())
}
pub fn local_addresses() -> Result<Vec<Ipv4Addr>> {
    let mut addresses: Vec<_> = if_addrs::get_if_addrs()?
        .into_iter()
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(ip) => Some(ip),
            _ => None,
        })
        .collect();
    addresses.sort_by_key(|ip| (ip.is_loopback(), !ip.is_private(), *ip));
    addresses.dedup();
    Ok(addresses)
}
pub fn local_endpoint(address: &str, port: u16) -> Result<SocketAddrV4> {
    let ip = address
        .parse::<Ipv4Addr>()
        .map_err(|e| PeerError::new("invalid_address", e))?;
    let endpoint = SocketAddrV4::new(ip, port);
    validate_address(endpoint)?;
    if !local_addresses()?.contains(&ip) {
        return Err(PeerError::new(
            "invalid_address",
            "address is not on this host",
        ));
    }
    Ok(endpoint)
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Initiator,
    Responder,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "lowercase", deny_unknown_fields)]
pub enum SessionSettings {
    Lan {
        #[serde(rename = "localAddress")]
        local_address: Ipv4Addr,
        #[serde(rename = "localPort")]
        local_port: u16,
        role: Role,
    },
    Internet,
}
impl SessionSettings {
    pub fn mode(&self) -> ConnectionMode {
        match self {
            Self::Lan { .. } => ConnectionMode::Lan,
            Self::Internet => ConnectionMode::Internet,
        }
    }
    pub fn endpoint(&self) -> Result<SocketAddrV4> {
        match self {
            Self::Lan {
                local_address,
                local_port,
                ..
            } => {
                let e = SocketAddrV4::new(*local_address, *local_port);
                validate_address(e)?;
                Ok(e)
            }
            _ => Err(PeerError::new("mode_mismatch", "LAN settings required")),
        }
    }
    pub fn validate_for(&self, peer: &Invitation) -> Result<()> {
        if self.mode() != peer.target.mode() {
            return Err(PeerError::new(
                "mode_mismatch",
                "code and selected mode differ",
            ));
        }
        if self.mode() == ConnectionMode::Lan {
            self.endpoint()?;
        }
        Ok(())
    }
    pub fn role(&self, own: PeerId, peer: &Invitation) -> Role {
        match self {
            Self::Lan { role, .. } => *role,
            Self::Internet => {
                if own.to_string() < peer.peer_id {
                    Role::Initiator
                } else {
                    Role::Responder
                }
            }
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionProfile {
    pub version: u8,
    pub invitation: Invitation,
    pub session: Option<SessionSettings>,
}
impl ConnectionProfile {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err(PeerError::new(
                "storage_error",
                "unsupported connection profile",
            ));
        }
        self.invitation.validate(false)?;
        if let Some(s) = &self.session {
            s.validate_for(&self.invitation)?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerState {
    pub phase: &'static str,
    pub peer_id: Option<String>,
    pub revision: u64,
    pub error: Option<&'static str>,
    pub mode: Option<ConnectionMode>,
    pub path: Option<ConnectionPath>,
}
impl Default for PeerState {
    fn default() -> Self {
        Self {
            phase: "disconnected",
            peer_id: None,
            revision: 0,
            error: None,
            mode: None,
            path: None,
        }
    }
}
