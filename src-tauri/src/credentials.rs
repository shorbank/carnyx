//! The only adapter to native credential stores. Never forward native error details.
pub trait Credentials: Send + Sync {
    fn load(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, ()>;
    fn save(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), ()>;
}

pub struct OsCredentials;
impl OsCredentials {
    fn entry(service: &str, account: &str) -> Result<keyring_core::Entry, ()> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            use keyring_core::api::CredentialStoreApi;
            #[cfg(target_os = "macos")]
            let store = apple_native_keyring_store::keychain::Store::new();
            #[cfg(target_os = "windows")]
            let store = windows_native_keyring_store::store::Store::new();
            store
                .and_then(|s| s.build(service, account, None))
                .map_err(|_| ())
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = (service, account);
            Err(())
        }
    }
}
impl Credentials for OsCredentials {
    fn load(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, ()> {
        match Self::entry(service, account)?.get_secret() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(_) => Err(()),
        }
    }
    fn save(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), ()> {
        Self::entry(service, account)?
            .set_secret(secret)
            .map_err(|_| ())
    }
}
