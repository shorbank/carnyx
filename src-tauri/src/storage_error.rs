use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum StorageError {
    Io(io::Error),
    Sqlite(rusqlite::Error),
    UnsupportedSchemaVersion(i64),
    InvalidSchema,
    InvalidIdentity,
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
            Self::InvalidIdentity => write!(f, "invalid local identity"),
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Sqlite(error) => Some(error),
            Self::UnsupportedSchemaVersion(_) | Self::InvalidSchema | Self::InvalidIdentity => None,
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
