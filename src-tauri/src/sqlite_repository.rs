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
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "database filesystem error: {error}"),
            Self::Sqlite(error) => write!(f, "SQLite error: {error}"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(f, "unsupported database schema version: {version}")
            }
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Sqlite(error) => Some(error),
            Self::UnsupportedSchemaVersion(_) => None,
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
                    PRAGMA user_version = 1;",
                )?;
            }
            1 => {}
            other => return Err(StorageError::UnsupportedSchemaVersion(other)),
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
            1
        );
    }

    #[test]
    fn duplicate_ids_do_not_replace_existing_messages() {
        let directory = tempfile::tempdir().unwrap();
        let mut repository =
            SqliteMessageRepository::open(directory.path().join("messages.sqlite")).unwrap();
        let first = Message::new("Original").unwrap();
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
        connection.execute_batch("CREATE TABLE preserved (value TEXT); INSERT INTO preserved VALUES ('keep'); PRAGMA user_version = 2;").unwrap();
        drop(connection);
        assert!(matches!(
            SqliteMessageRepository::open(&path),
            Err(StorageError::UnsupportedSchemaVersion(2))
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
            2
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
}
