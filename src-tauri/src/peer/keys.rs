use super::{PeerError, Result, types::Invitation};
use crate::identity::PeerId;
use zeroize::Zeroizing;

pub trait SecretStore: Send + Sync {
    fn load(&self, peer: PeerId) -> Result<Option<Vec<u8>>>;
    fn save(&self, peer: PeerId, secret: &[u8]) -> Result<()>;
}
pub trait KeyRepository {
    fn public_key(&self) -> Result<Option<[u8; 32]>>;
    fn save_public_key(&mut self, key: &[u8; 32]) -> Result<()>;
    fn selected_peer(&self) -> Result<Option<Invitation>>;
    #[cfg(test)]
    fn select_peer(&mut self, peer: &Invitation) -> Result<()>;
}
#[derive(Clone)]
pub struct Keys {
    pub private: Zeroizing<Vec<u8>>,
    pub public: [u8; 32],
}
pub fn validate_pin(expected: Option<&[u8]>, actual: &[u8]) -> Result<()> {
    if expected.is_some_and(|key| key != actual) {
        return Err(PeerError::new("peer_key_changed", "pinned key differs"));
    }
    Ok(())
}
#[cfg(test)]
pub fn load_keys(
    repository: &mut impl KeyRepository,
    secrets: &dyn SecretStore,
    peer: PeerId,
) -> Result<Keys> {
    let expected = repository.public_key()?;
    let keys = restore_keys(secrets, peer, expected)?;
    if expected.is_none() {
        repository.save_public_key(&keys.public)?;
    }
    Ok(keys)
}
pub fn restore_keys(
    secrets: &dyn SecretStore,
    peer: PeerId,
    expected: Option<[u8; 32]>,
) -> Result<Keys> {
    let secret = match secrets.load(peer)? {
        Some(secret) => Zeroizing::new(secret),
        None if expected.is_some() => {
            return Err(PeerError::new("key_unavailable", "private key missing"));
        }
        None => {
            let generated =
                snow::Builder::new(super::protocol::NOISE.parse().expect("valid Noise pattern"))
                    .generate_keypair()
                    .map_err(|e| PeerError::new("key_unavailable", e))?;
            let secret = Zeroizing::new(generated.private);
            secrets.save(peer, &secret)?;
            secret
        }
    };
    let bytes: [u8; 32] = secret
        .as_slice()
        .try_into()
        .map_err(|_| PeerError::new("key_unavailable", "invalid private key length"))?;
    let public = x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(bytes)).to_bytes();
    if expected.is_some_and(|key| key != public) {
        return Err(PeerError::new(
            "key_unavailable",
            "private/public key mismatch",
        ));
    }
    Ok(Keys {
        private: secret,
        public,
    })
}

pub struct OsSecretStore {
    service: String,
}
impl OsSecretStore {
    pub fn new(identifier: &str) -> Self {
        Self {
            service: format!("{identifier}.noise-key.v1"),
        }
    }
    fn entry(&self, peer: PeerId) -> Result<keyring_core::Entry> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            use keyring_core::api::CredentialStoreApi;
            #[cfg(target_os = "macos")]
            let store = apple_native_keyring_store::keychain::Store::new();
            #[cfg(target_os = "windows")]
            let store = windows_native_keyring_store::store::Store::new();
            store
                .and_then(|s| s.build(&self.service, &peer.to_string(), None))
                .map_err(|e| PeerError::new("key_unavailable", e))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = peer;
            Err(PeerError::new(
                "unsupported_platform",
                "peer transport supports macOS and Windows",
            ))
        }
    }
}
impl SecretStore for OsSecretStore {
    fn load(&self, peer: PeerId) -> Result<Option<Vec<u8>>> {
        match self.entry(peer)?.get_secret() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(PeerError::new("key_unavailable", e)),
        }
    }
    fn save(&self, peer: PeerId, secret: &[u8]) -> Result<()> {
        self.entry(peer)?
            .set_secret(secret)
            .map_err(|e| PeerError::new("key_unavailable", e))
    }
}
