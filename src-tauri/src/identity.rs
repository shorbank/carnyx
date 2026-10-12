use std::{error::Error, fmt, str::FromStr};
use uuid::{Uuid, Variant, Version};

/// Stable application identifier; authentication is a later transport concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeerId(Uuid);

#[derive(Debug)]
pub struct InvalidPeerId;
impl fmt::Display for InvalidPeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "peer ID must be a UUID-v4")
    }
}
impl Error for InvalidPeerId {}

impl PeerId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}
impl FromStr for PeerId {
    type Err = InvalidPeerId;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let uuid = Uuid::parse_str(value).map_err(|_| InvalidPeerId)?;
        if uuid.get_version() != Some(Version::Random) || uuid.get_variant() != Variant::RFC4122 {
            return Err(InvalidPeerId);
        }
        Ok(Self(uuid))
    }
}
impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalIdentity {
    peer_id: PeerId,
}
impl LocalIdentity {
    pub fn generate() -> Self {
        Self {
            peer_id: PeerId::generate(),
        }
    }
    pub(crate) fn from_stored(peer_id: PeerId) -> Self {
        Self { peer_id }
    }
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_and_canonicalizes_peer_ids() {
        let id = PeerId::generate();
        assert_eq!(id.to_string().to_uppercase().parse::<PeerId>().unwrap(), id);
        for invalid in [
            "",
            "not-a-peer",
            "00000000-0000-0000-0000-000000000000",
            "00000000-0000-1000-8000-000000000000",
            "00000000-0000-4000-0000-000000000000",
        ] {
            assert!(invalid.parse::<PeerId>().is_err());
        }
    }
}
