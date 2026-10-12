pub use crate::storage_error::StorageError;
use crate::{
    identity::{LocalIdentity, PeerId},
    message::{Direction, Message, MessageError, MessageParts},
    message_service::{InsertResult, MessageRepository},
    migrations,
};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use std::{fs, path::Path};

const COLUMNS: &str = "id, text, created_at, sender_id, recipient_id, direction, received_at";
const INSERT: &str = "INSERT INTO messages(id, text, created_at, sender_id, recipient_id, direction, received_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)";

pub struct SqliteMessageRepository {
    connection: Connection,
    local_identity: LocalIdentity,
}
impl SqliteMessageRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let mut connection = Connection::open(path)?;
        let local_identity = migrations::migrate(&mut connection)?;
        Ok(Self {
            connection,
            local_identity,
        })
    }
    pub fn local_identity(&self) -> LocalIdentity {
        self.local_identity
    }
}

fn decode_peer(value: String, index: usize) -> rusqlite::Result<PeerId> {
    value.parse().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}
fn read_message(row: &Row<'_>) -> rusqlite::Result<Message> {
    let sender_id = decode_peer(row.get(3)?, 3)?;
    let recipient_id = row
        .get::<_, Option<String>>(4)?
        .map(|value| decode_peer(value, 4))
        .transpose()?;
    let invalid_value = |error: MessageError| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    };
    let parts = MessageParts {
        id: row.get(0)?,
        text: row.get(1)?,
        created_at: row.get(2)?,
        sender_id,
        recipient_id,
        direction: Direction::from_stored(&row.get::<_, String>(5)?).map_err(invalid_value)?,
        received_at: row.get(6)?,
    };
    Message::from_stored(parts).map_err(invalid_value)
}
fn insert(connection: &Connection, message: &Message, sql: &str) -> rusqlite::Result<usize> {
    connection.execute(
        sql,
        params![
            message.id(),
            message.text(),
            message.created_at(),
            message.sender_id().to_string(),
            message.recipient_id().map(|peer| peer.to_string()),
            message.direction().as_str(),
            message.received_at()
        ],
    )
}
fn storage_error(error: rusqlite::Error) -> MessageError {
    MessageError::Storage(Box::new(StorageError::Sqlite(error)))
}

impl MessageRepository for SqliteMessageRepository {
    fn insert(&mut self, message: &Message) -> Result<(), MessageError> {
        insert(&self.connection, message, INSERT).map_err(storage_error)?;
        Ok(())
    }
    fn insert_if_absent(&mut self, message: &Message) -> Result<InsertResult, MessageError> {
        let mut attempt = || -> rusqlite::Result<InsertResult> {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let count = insert(
                &transaction,
                message,
                &format!("{INSERT} ON CONFLICT(id) DO NOTHING"),
            )?;
            let result = if count == 1 {
                InsertResult::Inserted
            } else {
                InsertResult::Existing(transaction.query_row(
                    &format!("SELECT {COLUMNS} FROM messages WHERE id=?1"),
                    params![message.id()],
                    read_message,
                )?)
            };
            transaction.commit()?;
            Ok(result)
        };
        attempt().map_err(storage_error)
    }
    fn find_by_id(&self, id: &str) -> Result<Option<Message>, MessageError> {
        self.connection
            .query_row(
                &format!("SELECT {COLUMNS} FROM messages WHERE id=?1"),
                params![id],
                read_message,
            )
            .optional()
            .map_err(storage_error)
    }
    fn list_since(&self, cutoff: i64) -> Result<Vec<Message>, MessageError> {
        let query = || -> rusqlite::Result<Vec<Message>> {
            let mut statement = self.connection.prepare_cached(&format!("SELECT {COLUMNS} FROM messages WHERE COALESCE(received_at, created_at) > ?1 ORDER BY COALESCE(received_at, created_at) ASC, rowid ASC"))?;
            statement
                .query_map(params![cutoff], read_message)?
                .collect()
        };
        query().map_err(storage_error)
    }
    fn delete_through(&mut self, cutoff: i64) -> Result<Vec<String>, MessageError> {
        let mut delete = || -> rusqlite::Result<Vec<String>> {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let ids = {
                let mut statement = transaction.prepare("SELECT id FROM messages WHERE COALESCE(received_at, created_at) <= ?1 ORDER BY COALESCE(received_at, created_at) ASC, rowid ASC")?;
                statement
                    .query_map(params![cutoff], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?
            };
            if !ids.is_empty() {
                transaction.execute(
                    "DELETE FROM messages WHERE COALESCE(received_at, created_at) <= ?1",
                    params![cutoff],
                )?;
            }
            transaction.commit()?;
            Ok(ids)
        };
        delete().map_err(storage_error)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_service::MessageService;

    #[test]
    fn persists_all_fields_across_reopening_and_creates_directories() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/messages.sqlite");
        let repository = SqliteMessageRepository::open(&path).unwrap();
        let own = repository.local_identity().peer_id();
        let mut service = MessageService::new(repository, own);
        let message = service
            .create_message(" Hallo 🦀\n  '); DROP TABLE messages; -- ")
            .unwrap();
        drop(service);
        let repository = SqliteMessageRepository::open(&path).unwrap();
        assert_eq!(repository.find_by_id(message.id()).unwrap(), Some(message));
        assert_eq!(repository.find_by_id("missing").unwrap(), None);
        assert_eq!(
            repository
                .connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            3
        );
    }

    #[test]
    fn duplicate_ids_do_not_replace_existing_messages() {
        let directory = tempfile::tempdir().unwrap();
        let mut repository =
            SqliteMessageRepository::open(directory.path().join("messages.sqlite")).unwrap();
        let first =
            Message::local("Original", 1234, repository.local_identity().peer_id()).unwrap();
        repository.insert(&first).unwrap();
        let duplicate = Message::from_stored(MessageParts {
            id: first.id().to_owned(),
            text: "Replacement".into(),
            created_at: 0,
            sender_id: first.sender_id(),
            recipient_id: None,
            direction: Direction::Local,
            received_at: None,
        })
        .unwrap();
        assert!(matches!(
            repository.insert(&duplicate),
            Err(MessageError::Storage(_))
        ));
        assert_eq!(repository.find_by_id(first.id()).unwrap(), Some(first));
    }

    #[test]
    fn rejects_unknown_versions_without_modifying_existing_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE preserved (value TEXT); INSERT INTO preserved VALUES ('keep'); PRAGMA user_version = 4;").unwrap();
        drop(connection);
        assert!(matches!(
            SqliteMessageRepository::open(&path),
            Err(StorageError::UnsupportedSchemaVersion(4))
        ));
        let connection = Connection::open(&path).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT value FROM preserved", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "keep"
        );
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            4
        );
    }

    #[test]
    fn failed_schema_creation_does_not_mark_database_as_initialized() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE messages (existing TEXT);")
            .unwrap();
        drop(connection);
        assert!(matches!(
            SqliteMessageRepository::open(&path),
            Err(StorageError::Sqlite(_))
        ));
        let connection = Connection::open(&path).unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
        connection
            .execute("INSERT INTO messages (existing) VALUES ('keep')", [])
            .unwrap();
    }

    #[test]
    fn forwards_filesystem_errors_instead_of_using_memory_storage() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("not-a-directory");
        fs::write(&file, "keep").unwrap();
        assert!(matches!(
            SqliteMessageRepository::open(file.join("messages.sqlite")),
            Err(StorageError::Io(_))
        ));
        assert_eq!(fs::read_to_string(file).unwrap(), "keep");
    }

    #[test]
    fn migrates_version_one_without_losing_messages_and_preserves_tie_order() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE messages(id TEXT PRIMARY KEY NOT NULL, text TEXT NOT NULL, created_at INTEGER NOT NULL);
            INSERT INTO messages VALUES ('z', 'first', 10), ('a', 'second', 10), ('later', 'third', 11);
            PRAGMA user_version = 1;").unwrap();
        drop(connection);
        let mut repository = SqliteMessageRepository::open(&path).unwrap();
        assert_eq!(
            repository
                .list_since(9)
                .unwrap()
                .iter()
                .map(Message::id)
                .collect::<Vec<_>>(),
            vec!["z", "a", "later"]
        );
        assert_eq!(repository.delete_through(10).unwrap(), vec!["z", "a"]);
        drop(repository);
        let repository = SqliteMessageRepository::open(path).unwrap();
        assert_eq!(
            repository
                .list_since(9)
                .unwrap()
                .iter()
                .map(Message::id)
                .collect::<Vec<_>>(),
            vec!["later"]
        );
    }

    #[test]
    fn rejects_damaged_version_two_schema() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE messages(text TEXT); PRAGMA user_version = 2;")
            .unwrap();
        drop(connection);
        assert!(matches!(
            SqliteMessageRepository::open(path),
            Err(StorageError::InvalidSchema)
        ));
    }

    #[test]
    fn failed_delete_rolls_back_and_never_returns_expired_ids() {
        let directory = tempfile::tempdir().unwrap();
        let mut repository =
            SqliteMessageRepository::open(directory.path().join("messages.sqlite")).unwrap();
        let message =
            Message::local("keep on failure", 10, repository.local_identity().peer_id()).unwrap();
        repository.insert(&message).unwrap();
        repository.connection.execute_batch("CREATE TRIGGER fail_delete BEFORE DELETE ON messages BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        assert!(matches!(
            repository.delete_through(10),
            Err(MessageError::Storage(_))
        ));
        assert_eq!(repository.find_by_id(message.id()).unwrap(), Some(message));
    }

    #[test]
    fn reopening_service_retains_valid_messages_and_startup_cleanup_removes_old_ones() {
        use crate::{clock::Clock, message_service::RETENTION_MILLIS};
        struct FixedClock;
        impl Clock for FixedClock {
            fn now_millis(&self) -> Result<i64, MessageError> {
                Ok(RETENTION_MILLIS + 10)
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let mut repository = SqliteMessageRepository::open(&path).unwrap();
        let old = Message::local("old", 10, repository.local_identity().peer_id()).unwrap();
        let valid = Message::local("valid", 11, repository.local_identity().peer_id()).unwrap();
        repository.insert(&old).unwrap();
        repository.insert(&valid).unwrap();
        drop(repository);
        let repository = SqliteMessageRepository::open(&path).unwrap();
        let own = repository.local_identity().peer_id();
        let mut service = MessageService::with_clock(repository, FixedClock, own);
        assert_eq!(service.list_messages().unwrap(), vec![valid.clone()]);
        assert_eq!(service.cleanup_expired().unwrap(), vec![old.id()]);
        assert_eq!(service.list_messages().unwrap(), vec![valid]);
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn identity_survives_reopening_and_complete_message_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let mut repository = SqliteMessageRepository::open(&path).unwrap();
        let identity = repository.local_identity();
        repository
            .insert(&Message::local("delete", 100, identity.peer_id()).unwrap())
            .unwrap();
        repository.delete_through(100).unwrap();
        drop(repository);
        let repository = SqliteMessageRepository::open(&path).unwrap();
        assert_eq!(repository.local_identity(), identity);
        assert!(repository.list_since(-1).unwrap().is_empty());
        let other = SqliteMessageRepository::open(directory.path().join("other.sqlite")).unwrap();
        assert_ne!(repository.local_identity(), other.local_identity());
    }

    fn legacy(path: &Path, time: i64) {
        let connection = Connection::open(path).unwrap();
        connection.execute_batch("CREATE TABLE messages(id TEXT PRIMARY KEY NOT NULL, text TEXT NOT NULL, created_at INTEGER NOT NULL);
            CREATE INDEX idx_messages_created_at ON messages(created_at); PRAGMA user_version=2;").unwrap();
        connection.execute("INSERT INTO messages(rowid,id,text,created_at) VALUES(42, 'legacy-z', ' original text ', ?1), (84, 'legacy-a', 'second', ?1)", params![time]).unwrap();
    }

    #[test]
    fn migrates_version_two_preserving_ids_text_timestamps_rowids_and_direction() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        legacy(&path, 100);
        let repository = SqliteMessageRepository::open(&path).unwrap();
        let own = repository.local_identity().peer_id();
        let messages = repository.list_since(99).unwrap();
        assert_eq!(
            messages.iter().map(Message::id).collect::<Vec<_>>(),
            vec!["legacy-z", "legacy-a"]
        );
        assert_eq!(messages[0].text(), " original text ");
        for message in messages {
            assert_eq!(message.sender_id(), own);
            assert_eq!(message.recipient_id(), None);
            assert_eq!(message.direction(), Direction::Local);
            assert_eq!(message.created_at(), 100);
            assert_eq!(message.received_at(), None);
        }
        let rows: Vec<_> = repository
            .connection
            .prepare("SELECT rowid FROM messages ORDER BY rowid")
            .unwrap()
            .query_map([], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, vec![42, 84]);
    }

    #[test]
    fn migration_failure_rolls_back_identity_and_schema_changes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        legacy(&path, -1);
        assert!(SqliteMessageRepository::open(&path).is_err());
        let connection = Connection::open(&path).unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM messages", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name='local_identity'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn missing_or_corrupt_identity_is_never_regenerated_in_version_three() {
        let directory = tempfile::tempdir().unwrap();
        for damage in [
            "DELETE FROM local_identity",
            "UPDATE local_identity SET peer_id='bad-peer'",
        ] {
            let path = directory.path().join(if damage.starts_with("DELETE") {
                "missing.sqlite"
            } else {
                "invalid.sqlite"
            });
            drop(SqliteMessageRepository::open(&path).unwrap());
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch(damage).unwrap();
            drop(connection);
            assert!(matches!(
                SqliteMessageRepository::open(&path),
                Err(StorageError::InvalidIdentity)
            ));
        }
    }

    #[test]
    fn database_constraints_reject_inconsistent_direction_fields() {
        let directory = tempfile::tempdir().unwrap();
        let repository =
            SqliteMessageRepository::open(directory.path().join("messages.sqlite")).unwrap();
        assert!(repository.connection.execute("INSERT INTO messages(id,text,created_at,sender_id,recipient_id,direction,received_at) VALUES('invalid','text',100,'sender','recipient','local',NULL)", []).is_err());
        assert!(repository.list_since(-1).unwrap().is_empty());
    }
}
