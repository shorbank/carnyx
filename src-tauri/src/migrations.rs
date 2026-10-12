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
    let version: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if !(0..=3).contains(&version) {
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
    transaction.commit()?;
    Ok(identity)
}
