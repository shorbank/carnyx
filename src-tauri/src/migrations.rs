use crate::{identity::LocalIdentity, sqlite_identity, storage_error::StorageError};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

const CREATE_MESSAGES: &str = "CREATE TABLE messages_v3 (
    id TEXT PRIMARY KEY NOT NULL CHECK(length(id) > 0),
    text TEXT NOT NULL,
    created_at INTEGER NOT NULL CHECK(created_at BETWEEN 0 AND 8640000000000000),
    sender_id TEXT NOT NULL,
    recipient_id TEXT,
    direction TEXT NOT NULL CHECK(direction IN ('local', 'outgoing', 'incoming')),
    received_at INTEGER CHECK(received_at IS NULL OR received_at BETWEEN 0 AND 8640000000000000),
    CHECK(recipient_id IS NULL OR sender_id <> recipient_id),
    CHECK((direction = 'local' AND recipient_id IS NULL AND received_at IS NULL)
       OR (direction = 'outgoing' AND recipient_id IS NOT NULL AND received_at IS NULL)
       OR (direction = 'incoming' AND recipient_id IS NOT NULL AND received_at IS NOT NULL))
)";
const CREATE_INDEX: &str =
    "CREATE INDEX idx_messages_local_time ON messages(COALESCE(received_at, created_at))";
const CREATE_TRANSPORT_IDENTITY: &str = "CREATE TABLE transport_identity(slot INTEGER PRIMARY KEY NOT NULL CHECK(slot=1), public_key BLOB NOT NULL CHECK(length(public_key)=32))";
const CREATE_TRUSTED_PEER: &str = "CREATE TABLE trusted_peer(slot INTEGER PRIMARY KEY NOT NULL CHECK(slot=1), peer_id TEXT NOT NULL, public_key BLOB NOT NULL CHECK(length(public_key)=32), address TEXT NOT NULL)";
const CREATE_PEER_KEYS: &str = "CREATE TABLE peer_keys(peer_id TEXT PRIMARY KEY NOT NULL, public_key BLOB NOT NULL CHECK(length(public_key)=32))";
const CREATE_SESSION_SETTINGS: &str = "CREATE TABLE peer_session_settings(slot INTEGER PRIMARY KEY NOT NULL CHECK(slot=1), peer_id TEXT NOT NULL, local_address TEXT NOT NULL, local_port INTEGER NOT NULL CHECK(local_port BETWEEN 1 AND 65535), role TEXT NOT NULL CHECK(role IN ('initiator','responder')))";
const CREATE_DELIVERIES: &str = "CREATE TABLE deliveries(message_id TEXT PRIMARY KEY NOT NULL REFERENCES messages(id) ON DELETE CASCADE, state TEXT NOT NULL CHECK(state IN ('pending','acknowledged')), attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts>=0), acknowledged_at INTEGER, CHECK((state='pending' AND acknowledged_at IS NULL) OR (state='acknowledged' AND acknowledged_at BETWEEN 0 AND 8640000000000000)))";

const CREATE_CONNECTION_PROFILE: &str = "CREATE TABLE connection_profile(slot INTEGER PRIMARY KEY NOT NULL CHECK(slot=1), profile TEXT NOT NULL CHECK(json_valid(profile)))";
const CREATE_INTERNET_IDENTITY: &str = "CREATE TABLE internet_identity(slot INTEGER PRIMARY KEY NOT NULL CHECK(slot=1), endpoint_id BLOB NOT NULL CHECK(length(endpoint_id)=32))";

fn compact(sql: &str) -> String {
    sql.replace('"', "")
        .split_whitespace()
        .collect::<String>()
        .to_lowercase()
}

fn validate_schema_sql(
    connection: &Connection,
    kind: &str,
    name: &str,
    expected: &str,
) -> Result<(), StorageError> {
    let sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type=?1 AND name=?2",
            params![kind, name],
            |row| row.get(0),
        )
        .map_err(|_| StorageError::InvalidSchema)?;
    if compact(&sql) != compact(expected) {
        return Err(StorageError::InvalidSchema);
    }
    Ok(())
}

fn validate_legacy(connection: &Connection, version: i64) -> Result<(), StorageError> {
    let mut statement = connection.prepare("PRAGMA table_info(messages)")?;
    let columns = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if columns
        != vec![
            ("id".into(), "TEXT".into(), true, 1),
            ("text".into(), "TEXT".into(), true, 0),
            ("created_at".into(), "INTEGER".into(), true, 0),
        ]
    {
        return Err(StorageError::InvalidSchema);
    }
    connection.prepare("SELECT rowid, id, text, created_at FROM messages")?;
    if version == 2 {
        let mut statement = connection.prepare("PRAGMA index_info(idx_messages_created_at)")?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(2))?
            .collect::<Result<Vec<_>, _>>()?;
        if names != vec!["created_at"] {
            return Err(StorageError::InvalidSchema);
        }
    }
    Ok(())
}

pub(crate) fn migrate(connection: &mut Connection) -> Result<LocalIdentity, StorageError> {
    // Serialize opening/migration across independent repository connections.
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let identity = migrate_transaction(&transaction, 6)?;
    transaction.commit()?;
    Ok(identity)
}

/// Check schema upgrades on the encrypted candidate without changing its logical copy.
pub(crate) fn validate_upgrade(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    migrate_transaction(&transaction, 6)?;
    transaction.rollback()?;
    Ok(())
}

fn migrate_transaction(
    transaction: &rusqlite::Transaction<'_>,
    target: i64,
) -> Result<LocalIdentity, StorageError> {
    let version: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if !(0..=6).contains(&version) {
        return Err(StorageError::UnsupportedSchemaVersion(version));
    }
    let identity = if version < 3 {
        if version == 0 {
            transaction.execute_batch("CREATE TABLE messages(id TEXT PRIMARY KEY NOT NULL, text TEXT NOT NULL, created_at INTEGER NOT NULL)")?;
        }
        validate_legacy(&transaction, version)?;
        let identity = sqlite_identity::create(&transaction)?;
        transaction.execute_batch(CREATE_MESSAGES)?;
        transaction.execute("INSERT INTO messages_v3(rowid, id, text, created_at, sender_id, recipient_id, direction, received_at)
            SELECT rowid, id, text, created_at, ?1, NULL, 'local', NULL FROM messages ORDER BY rowid", params![identity.peer_id().to_string()])?;
        transaction
            .execute_batch("DROP TABLE messages; ALTER TABLE messages_v3 RENAME TO messages;")?;
        transaction.execute_batch(CREATE_INDEX)?;
        transaction.execute_batch("PRAGMA user_version = 3")?;
        identity
    } else {
        sqlite_identity::load(&transaction)?
    };
    validate_schema_sql(
        &transaction,
        "table",
        "messages",
        &CREATE_MESSAGES.replace("messages_v3", "messages"),
    )?;
    validate_schema_sql(
        &transaction,
        "table",
        "local_identity",
        sqlite_identity::CREATE_IDENTITY,
    )?;
    validate_schema_sql(
        &transaction,
        "index",
        "idx_messages_local_time",
        CREATE_INDEX,
    )?;
    if version < 4 {
        transaction.execute_batch(CREATE_TRANSPORT_IDENTITY)?;
        transaction.execute_batch(CREATE_TRUSTED_PEER)?;
        transaction.execute_batch(CREATE_PEER_KEYS)?;
        transaction.execute_batch(CREATE_DELIVERIES)?;
        transaction.execute("INSERT INTO deliveries(message_id,state) SELECT id,'pending' FROM messages WHERE direction='outgoing'", [])?;
        transaction.execute_batch("PRAGMA user_version = 4")?;
    }
    for (name, ddl) in [
        ("transport_identity", CREATE_TRANSPORT_IDENTITY),
        ("peer_keys", CREATE_PEER_KEYS),
        ("deliveries", CREATE_DELIVERIES),
    ] {
        validate_schema_sql(&transaction, "table", name, ddl)?;
    }
    if version < 6 {
        validate_schema_sql(&transaction, "table", "trusted_peer", CREATE_TRUSTED_PEER)?;
        if version < 5 {
            transaction.execute_batch(CREATE_SESSION_SETTINGS)?;
            transaction.execute_batch("PRAGMA user_version = 5")?;
        }
        validate_schema_sql(
            &transaction,
            "table",
            "peer_session_settings",
            CREATE_SESSION_SETTINGS,
        )?;
        if target == 6 {
            transaction.execute_batch(CREATE_CONNECTION_PROFILE)?;
            transaction.execute_batch(CREATE_INTERNET_IDENTITY)?;
            migrate_connection_profile(transaction)?;
            transaction.execute_batch(
                "DROP TABLE peer_session_settings; DROP TABLE trusted_peer; PRAGMA user_version=6;",
            )?;
        }
    }
    if target == 6 {
        validate_schema_sql(
            &transaction,
            "table",
            "connection_profile",
            CREATE_CONNECTION_PROFILE,
        )?;
        validate_schema_sql(
            &transaction,
            "table",
            "internet_identity",
            CREATE_INTERNET_IDENTITY,
        )?;
        let raw: Option<String> = transaction
            .query_row(
                "SELECT profile FROM connection_profile WHERE slot=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(raw) = raw {
            let profile: crate::peer::types::ConnectionProfile =
                serde_json::from_str(&raw).map_err(|_| StorageError::InvalidSchema)?;
            profile
                .validate()
                .map_err(|_| StorageError::InvalidSchema)?;
        }
    }
    Ok(identity)
}

fn migrate_connection_profile(tx: &rusqlite::Transaction<'_>) -> Result<(), StorageError> {
    use crate::peer::types::{ConnectionProfile, Invitation, Role, SessionSettings};
    let peer: Option<(String, Vec<u8>, String)> = tx
        .query_row(
            "SELECT peer_id,public_key,address FROM trusted_peer WHERE slot=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let settings: Option<(String, String, u16, String)> = tx
        .query_row(
            "SELECT peer_id,local_address,local_port,role FROM peer_session_settings WHERE slot=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    if settings
        .as_ref()
        .is_some_and(|s| peer.as_ref().is_none_or(|p| s.0 != p.0))
    {
        return Err(StorageError::InvalidSchema);
    }
    if let Some((id, key, address)) = peer {
        let invitation = Invitation::new(
            id.parse().map_err(|_| StorageError::InvalidSchema)?,
            &key.try_into().map_err(|_| StorageError::InvalidSchema)?,
            address.parse().map_err(|_| StorageError::InvalidSchema)?,
        );
        if invitation.peer_id != id {
            return Err(StorageError::InvalidSchema);
        }
        let session = settings
            .map(|(_, address, port, role)| {
                Ok::<_, StorageError>(SessionSettings::Lan {
                    local_address: address.parse().map_err(|_| StorageError::InvalidSchema)?,
                    local_port: port,
                    role: match role.as_str() {
                        "initiator" => Role::Initiator,
                        "responder" => Role::Responder,
                        _ => return Err(StorageError::InvalidSchema),
                    },
                })
            })
            .transpose()?;
        let profile = ConnectionProfile {
            version: 1,
            invitation,
            session,
        };
        profile
            .validate()
            .map_err(|_| StorageError::InvalidSchema)?;
        let json = serde_json::to_string(&profile).map_err(|_| StorageError::InvalidSchema)?;
        tx.execute(
            "INSERT INTO connection_profile(slot,profile) VALUES(1,?1)",
            [json],
        )?;
    }
    Ok(())
}
#[cfg(test)]
pub(crate) fn legacy_v5(connection: &mut Connection) -> Result<LocalIdentity, StorageError> {
    let tx = connection.transaction()?;
    let identity = migrate_transaction(&tx, 5)?;
    tx.commit()?;
    Ok(identity)
}

/// Constructs historical on-disk fixtures from test data; never used by the app.
#[cfg(test)]
pub(crate) fn restore_legacy_fixture(c: &mut Connection) -> Result<(), StorageError> {
    use crate::peer::types::{ConnectionProfile, Role, SessionSettings};
    let raw: Option<String> = c
        .query_row(
            "SELECT profile FROM connection_profile WHERE slot=1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    c.execute_batch(CREATE_TRUSTED_PEER)?;
    c.execute_batch(CREATE_SESSION_SETTINGS)?;
    if let Some(raw) = raw {
        let profile: ConnectionProfile = serde_json::from_str(&raw).unwrap();
        let peer = profile.invitation;
        c.execute(
            "INSERT INTO trusted_peer(slot,peer_id,public_key,address) VALUES(1,?1,?2,?3)",
            params![
                peer.peer_id,
                peer.key().unwrap().as_slice(),
                peer.lan_address().unwrap().to_string()
            ],
        )?;
        if let Some(SessionSettings::Lan {
            local_address,
            local_port,
            role,
        }) = profile.session
        {
            c.execute(
                "INSERT INTO peer_session_settings VALUES(1,?1,?2,?3,?4)",
                params![
                    peer.peer_id,
                    local_address.to_string(),
                    local_port,
                    if role == Role::Initiator {
                        "initiator"
                    } else {
                        "responder"
                    }
                ],
            )?;
        }
    }
    c.execute_batch(
        "DROP TABLE connection_profile; DROP TABLE internet_identity; PRAGMA user_version=5;",
    )?;
    Ok(())
}
