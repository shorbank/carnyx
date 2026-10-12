use crate::{
    message::{Message, MessageError},
    message_service::MessageRepository,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::{error::Error, fmt, fs, io, path::Path};

#[derive(Debug)]
pub enum StorageError {
    Io(io::Error),
    Sqlite(rusqlite::Error),
    UnsupportedSchemaVersion(i64),
    InvalidSchema,
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "database filesystem error: {error}"),
            Self::Sqlite(error) => write!(f, "SQLite error: {error}"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(f, "unsupported database schema version: {version}")
            }
            Self::InvalidSchema => write!(f, "invalid messages database schema"),
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Sqlite(error) => Some(error),
            Self::UnsupportedSchemaVersion(_) | Self::InvalidSchema => None,
        }
    }
}

impl From<io::Error> for StorageError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

pub struct SqliteMessageRepository {
    connection: Connection,
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
        let transaction = connection.transaction()?;
        let version: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        match version {
            0 => {
                transaction.execute_batch(
                    "CREATE TABLE messages (
                        id TEXT PRIMARY KEY NOT NULL,
                        text TEXT NOT NULL,
                        created_at INTEGER NOT NULL
                    );
                    ",
                )?;
            }
            1 | 2 => {}
            other => return Err(StorageError::UnsupportedSchemaVersion(other)),
        }
        let columns = {
            let mut statement = transaction.prepare("PRAGMA table_info(messages)")?;
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, bool>(3)?,
                        row.get::<_, i64>(5)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        if columns
            != vec![
                ("id".into(), "TEXT".into(), true, 1),
                ("text".into(), "TEXT".into(), true, 0),
                ("created_at".into(), "INTEGER".into(), true, 0),
            ]
        {
            return Err(StorageError::InvalidSchema);
        }
        transaction.prepare("SELECT rowid, id, text, created_at FROM messages")?;
        if version < 2 {
            transaction.execute_batch("CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at); PRAGMA user_version = 2;")?;
        }
        let indexed_columns = {
            let mut statement =
                transaction.prepare("PRAGMA index_info(idx_messages_created_at)")?;
            statement
                .query_map([], |row| row.get::<_, String>(2))?
                .collect::<Result<Vec<_>, _>>()?
        };
        if indexed_columns != vec!["created_at"] {
            return Err(StorageError::InvalidSchema);
        }
        transaction.commit()?;
        Ok(Self { connection })
    }
}

impl MessageRepository for SqliteMessageRepository {
    fn insert(&mut self, message: &Message) -> Result<(), MessageError> {
        self.connection
            .execute(
                "INSERT INTO messages (id, text, created_at) VALUES (?1, ?2, ?3)",
                params![message.id(), message.text(), message.created_at()],
            )
            .map_err(|error| MessageError::Storage(Box::new(StorageError::Sqlite(error))))?;
        Ok(())
    }

    fn find_by_id(&self, id: &str) -> Result<Option<Message>, MessageError> {
        self.connection
            .query_row(
                "SELECT id, text, created_at FROM messages WHERE id = ?1",
                params![id],
                |row| Ok(Message::from_stored(row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| MessageError::Storage(Box::new(StorageError::Sqlite(error))))
    }

    fn list_since(&self, cutoff: i64) -> Result<Vec<Message>, MessageError> {
        let query = || -> rusqlite::Result<Vec<Message>> {
            let mut statement = self.connection.prepare_cached(
                "SELECT id, text, created_at FROM messages WHERE created_at > ?1 ORDER BY created_at ASC, rowid ASC"
            )?;
            statement
                .query_map(params![cutoff], |row| {
                    Ok(Message::from_stored(row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .collect()
        };
        query().map_err(storage_error)
    }

    fn delete_through(&mut self, cutoff: i64) -> Result<Vec<String>, MessageError> {
        let mut delete = || -> rusqlite::Result<Vec<String>> {
            let transaction = self
                .connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let ids = {
                let mut statement = transaction.prepare("SELECT id FROM messages WHERE created_at <= ?1 ORDER BY created_at ASC, rowid ASC")?;
                statement
                    .query_map(params![cutoff], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?
            };
            if !ids.is_empty() {
                transaction.execute(
                    "DELETE FROM messages WHERE created_at <= ?1",
                    params![cutoff],
                )?;
            }
            transaction.commit()?;
            Ok(ids)
        };
        delete().map_err(storage_error)
    }
}

fn storage_error(error: rusqlite::Error) -> MessageError {
    MessageError::Storage(Box::new(StorageError::Sqlite(error)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_service::MessageService;

    #[test]
    fn persists_all_fields_across_reopening_and_creates_directories() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/messages.sqlite");
        let mut service = MessageService::new(SqliteMessageRepository::open(&path).unwrap());
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
            2
        );
    }

    #[test]
    fn duplicate_ids_do_not_replace_existing_messages() {
        let directory = tempfile::tempdir().unwrap();
        let mut repository =
            SqliteMessageRepository::open(directory.path().join("messages.sqlite")).unwrap();
        let first = Message::new("Original", 1234).unwrap();
        repository.insert(&first).unwrap();
        let duplicate = Message::from_stored(first.id().to_owned(), "Replacement".into(), 0);
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
        connection.execute_batch("CREATE TABLE preserved (value TEXT); INSERT INTO preserved VALUES ('keep'); PRAGMA user_version = 3;").unwrap();
        drop(connection);
        assert!(matches!(
            SqliteMessageRepository::open(&path),
            Err(StorageError::UnsupportedSchemaVersion(3))
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
            3
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
        let message = Message::new("keep on failure", 10).unwrap();
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
        let old = Message::new("old", 10).unwrap();
        let valid = Message::new("valid", 11).unwrap();
        repository.insert(&old).unwrap();
        repository.insert(&valid).unwrap();
        drop(repository);
        let mut service =
            MessageService::with_clock(SqliteMessageRepository::open(&path).unwrap(), FixedClock);
        assert_eq!(service.list_messages().unwrap(), vec![valid.clone()]);
        assert_eq!(service.cleanup_expired().unwrap(), vec![old.id()]);
        assert_eq!(service.list_messages().unwrap(), vec![valid]);
    }
}
