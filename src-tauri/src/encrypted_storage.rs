//! SQLCipher connections, native key lifecycle, and restartable plaintext conversion.
//! Nothing in this module returns key bytes, SQL errors or message text to IPC.
use crate::credentials::Credentials;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::{fs::File, io::Read, path::Path, time::Duration};
use zeroize::Zeroizing;

pub(crate) type DatabaseKey = Zeroizing<[u8; 32]>;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StorageFailure {
    KeyUnavailable,
    KeyMissing,
    UnlockFailed,
    CipherUnavailable,
    MigrationFailed,
    CleanupFailed,
    StorageUnavailable,
}
type Result<T> = std::result::Result<T, StorageFailure>;
mod migration;
use migration::Paths;

fn plaintext(path: &Path) -> Result<bool> {
    let mut header = [0; 16];
    let mut file = File::open(path).map_err(|_| StorageFailure::StorageUnavailable)?;
    let count = file
        .read(&mut header)
        .map_err(|_| StorageFailure::StorageUnavailable)?;
    Ok(count == 16 && &header == b"SQLite format 3\0")
}

fn key(paths: &Paths, credentials: &dyn Credentials, identifier: &str) -> Result<DatabaseKey> {
    // Stable before the database is unlocked, separate from the Noise identity.
    let service = format!("{identifier}.history-key.v1");
    let account = "messages.sqlite";
    let loaded = credentials
        .load(&service, account)
        .map_err(|_| StorageFailure::KeyUnavailable)?;
    if let Some(secret) = loaded {
        let secret = Zeroizing::new(secret);
        return secret
            .as_slice()
            .try_into()
            .map(Zeroizing::new)
            .map_err(|_| StorageFailure::KeyUnavailable);
    }
    if paths
        .stage
        .try_exists()
        .map_err(|_| StorageFailure::StorageUnavailable)?
        || paths
            .retired
            .try_exists()
            .map_err(|_| StorageFailure::StorageUnavailable)?
        || paths
            .marker
            .try_exists()
            .map_err(|_| StorageFailure::StorageUnavailable)?
        || (paths
            .main
            .try_exists()
            .map_err(|_| StorageFailure::StorageUnavailable)?
            && !plaintext(&paths.main)?)
    {
        return Err(StorageFailure::KeyMissing);
    }
    let mut generated = Zeroizing::new([0; 32]);
    getrandom::fill(generated.as_mut()).map_err(|_| StorageFailure::KeyUnavailable)?;
    credentials
        .save(&service, account, generated.as_ref())
        .map_err(|_| StorageFailure::KeyUnavailable)?;
    // Confirm persistence before creating any ciphertext.
    let persisted = Zeroizing::new(
        credentials
            .load(&service, account)
            .map_err(|_| StorageFailure::KeyUnavailable)?
            .ok_or(StorageFailure::KeyUnavailable)?,
    );
    if persisted.as_slice() != generated.as_ref() {
        return Err(StorageFailure::KeyUnavailable);
    }
    Ok(generated)
}

pub(crate) fn connection(path: &Path, key: &DatabaseKey, create: bool) -> Result<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | if create {
            OpenFlags::SQLITE_OPEN_CREATE
        } else {
            OpenFlags::empty()
        };
    let connection =
        Connection::open_with_flags(path, flags).map_err(|_| StorageFailure::StorageUnavailable)?;
    let version: String = connection
        .query_row("PRAGMA cipher_version", [], |row| row.get(0))
        .map_err(|_| StorageFailure::CipherUnavailable)?;
    if !version.starts_with("4.19.0 ") && version != "4.19.0" {
        return Err(StorageFailure::CipherUnavailable);
    }
    connection
        .execute_batch("PRAGMA cipher_log_level = NONE;")
        .map_err(|_| StorageFailure::CipherUnavailable)?;
    // Key through the native API, never a SQL string or traced statement.
    // SAFETY: connection handle is live, and key points at 32 initialized bytes for this call.
    let result =
        unsafe { rusqlite::ffi::sqlite3_key(connection.handle(), key.as_ptr().cast(), 32) };
    if result != rusqlite::ffi::SQLITE_OK {
        return Err(StorageFailure::UnlockFailed);
    }
    connection.execute_batch("PRAGMA cipher_memory_security = ON; PRAGMA temp_store = MEMORY; PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")
        .map_err(|_| StorageFailure::UnlockFailed)?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|_| StorageFailure::StorageUnavailable)?;
    connection
        .query_row("SELECT count(*) FROM sqlite_master", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|_| StorageFailure::UnlockFailed)?;
    let status: String = connection
        .query_row("PRAGMA cipher_status", [], |row| row.get(0))
        .map_err(|_| StorageFailure::CipherUnavailable)?;
    if status != "1" {
        return Err(StorageFailure::CipherUnavailable);
    }
    Ok(connection)
}

fn integrity(connection: &Connection) -> Result<()> {
    let result: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|_| StorageFailure::UnlockFailed)?;
    if result != "ok" {
        return Err(StorageFailure::UnlockFailed);
    }
    let mut statement = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(|_| StorageFailure::UnlockFailed)?;
    if statement
        .query([])
        .map_err(|_| StorageFailure::UnlockFailed)?
        .next()
        .map_err(|_| StorageFailure::UnlockFailed)?
        .is_some()
    {
        return Err(StorageFailure::UnlockFailed);
    }
    Ok(())
}

/// Opens storage only after conversion AND removal of all owned plaintext artifacts.
pub(crate) fn open(
    path: &Path,
    credentials: &dyn Credentials,
    identifier: &str,
) -> Result<Connection> {
    migration::open_observed(path, credentials, identifier, |_| Ok(()))
}
#[cfg(test)]
pub(crate) fn test_connection(path: impl AsRef<Path>) -> rusqlite::Result<Connection> {
    connection(path.as_ref(), &Zeroizing::new([42; 32]), true)
        .map_err(|_| rusqlite::Error::InvalidQuery)
}

#[cfg(test)]
mod tests;
