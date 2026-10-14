use super::{PeerError, Result};
use crate::identity::PeerId;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddrV4};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Invitation {
    pub version: u8,
    pub peer_id: String,
    pub public_key: String,
    pub address: SocketAddrV4,
}
impl Invitation {
    pub fn new(peer: PeerId, public: &[u8; 32], address: SocketAddrV4) -> Self {
        Self {
            version: 1,
            peer_id: peer.to_string(),
            public_key: URL_SAFE_NO_PAD.encode(public),
            address,
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
    pub fn encode(&self) -> String {
        format!(
            "carnyx:v1:{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).expect("invitation serializes"))
        )
    }
    pub fn decode(code: &str) -> Result<Self> {
        if code.len() > 2048 {
            return Err(PeerError::new("invalid_code", "code too long"));
        }
        let body = code
            .trim()
            .strip_prefix("carnyx:v1:")
            .ok_or_else(|| PeerError::new("invalid_code", "unsupported invitation"))?;
        let bytes = URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|e| PeerError::new("invalid_code", e))?;
        let invitation: Self =
            serde_json::from_slice(&bytes).map_err(|e| PeerError::new("invalid_code", e))?;
        if invitation.version != 1
            || invitation.peer()?.to_string() != invitation.peer_id
            || invitation.key()? == [0; 32]
        {
            return Err(PeerError::new("invalid_code", "invalid identity"));
        }
        validate_address(invitation.address)?;
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerState {
    pub phase: &'static str,
    pub peer_id: Option<String>,
    pub revision: u64,
    pub error: Option<&'static str>,
}
impl Default for PeerState {
    fn default() -> Self {
        Self {
            phase: "disconnected",
            peer_id: None,
            revision: 0,
            error: None,
        }
    }
}

/// Local settings are separate from the public invitation exchanged with the peer.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSettings {
    pub local_address: Ipv4Addr,
    pub local_port: u16,
    pub role: Role,
}
impl SessionSettings {
    pub fn endpoint(&self) -> Result<SocketAddrV4> {
        let endpoint = SocketAddrV4::new(self.local_address, self.local_port);
        validate_address(endpoint)?;
        Ok(endpoint)
    }
}
