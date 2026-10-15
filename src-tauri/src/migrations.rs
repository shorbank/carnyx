use crate::{identity::LocalIdentity, sqlite_identity, storage_error::StorageError};
use rusqlite::{Connection, TransactionBehavior, params};

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
    let identity = migrate_transaction(&transaction)?;
    transaction.commit()?;
    Ok(identity)
}

/// Check schema upgrades on the encrypted candidate without changing its logical copy.
pub(crate) fn validate_upgrade(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    migrate_transaction(&transaction)?;
    transaction.rollback()?;
    Ok(())
}

fn migrate_transaction(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<LocalIdentity, StorageError> {
    let version: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if !(0..=5).contains(&version) {
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
        ("trusted_peer", CREATE_TRUSTED_PEER),
        ("peer_keys", CREATE_PEER_KEYS),
        ("deliveries", CREATE_DELIVERIES),
    ] {
        validate_schema_sql(&transaction, "table", name, ddl)?;
    }
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
    Ok(identity)
}
