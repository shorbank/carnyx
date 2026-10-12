use crate::{
    identity::{LocalIdentity, PeerId},
    storage_error::StorageError,
};
use rusqlite::{Connection, params};

pub(crate) const CREATE_IDENTITY: &str = "CREATE TABLE local_identity (
    slot INTEGER PRIMARY KEY NOT NULL CHECK(slot = 1),
    peer_id TEXT NOT NULL UNIQUE
)";

pub(crate) fn create(connection: &Connection) -> Result<LocalIdentity, StorageError> {
    let identity = LocalIdentity::generate();
    connection.execute(CREATE_IDENTITY, [])?;
    connection.execute(
        "INSERT INTO local_identity(slot, peer_id) VALUES(1, ?1)",
        params![identity.peer_id().to_string()],
    )?;
    Ok(identity)
}

pub(crate) fn load(connection: &Connection) -> Result<LocalIdentity, StorageError> {
    let mut statement = connection
        .prepare("SELECT slot, peer_id FROM local_identity")
        .map_err(|_| StorageError::InvalidIdentity)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() != 1 || rows[0].0 != 1 {
        return Err(StorageError::InvalidIdentity);
    }
    let peer = rows[0]
        .1
        .parse::<PeerId>()
        .map_err(|_| StorageError::InvalidIdentity)?;
    if rows[0].1 != peer.to_string() {
        return Err(StorageError::InvalidIdentity);
    }
    Ok(LocalIdentity::from_stored(peer))
}
