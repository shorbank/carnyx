//! Internet transport identity uses the shared native adapter, independently of Noise/SQLCipher.
use super::{PeerError, Result};
use crate::{credentials::Credentials, identity::PeerId};
use zeroize::Zeroizing;
fn unavailable() -> PeerError {
    PeerError::new("key_unavailable", "internet identity unavailable")
}
pub fn restore(
    credentials: &dyn Credentials,
    identifier: &str,
    own: PeerId,
    expected: Option<[u8; 32]>,
) -> Result<iroh::SecretKey> {
    let service = format!("{identifier}.internet-key.v1");
    let account = own.to_string();
    let raw = match credentials
        .load(&service, &account)
        .map_err(|_| unavailable())?
    {
        Some(bytes) => Zeroizing::new(bytes),
        None if expected.is_some() => return Err(unavailable()),
        None => {
            let mut bytes = Zeroizing::new(vec![0; 32]);
            getrandom::fill(&mut bytes).map_err(|_| unavailable())?;
            credentials
                .save(&service, &account, &bytes)
                .map_err(|_| unavailable())?;
            let restored = Zeroizing::new(
                credentials
                    .load(&service, &account)
                    .map_err(|_| unavailable())?
                    .ok_or_else(unavailable)?,
            );
            if restored.as_slice() != bytes.as_slice() {
                return Err(unavailable());
            }
            bytes
        }
    };
    let bytes = Zeroizing::new(<[u8; 32]>::try_from(raw.as_slice()).map_err(|_| unavailable())?);
    let secret = iroh::SecretKey::from_bytes(&bytes);
    if expected.is_some_and(|key| key != *secret.public().as_bytes()) {
        return Err(unavailable());
    }
    Ok(secret)
}
