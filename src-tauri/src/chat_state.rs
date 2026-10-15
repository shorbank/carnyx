use crate::{
    message::MessageError, message_service::MessageService,
    sqlite_repository::SqliteMessageRepository,
};
use std::{
    fmt,
    sync::{Arc, Mutex},
};

pub type ChatState = Arc<Mutex<MessageService<SqliteMessageRepository>>>;

#[cfg(test)]
pub fn initialize(
    path: impl AsRef<std::path::Path>,
) -> Result<ChatState, Box<dyn std::error::Error + Send + Sync>> {
    from_repository(SqliteMessageRepository::open(path)?)
}

pub(crate) fn from_repository(
    repository: SqliteMessageRepository,
) -> Result<ChatState, Box<dyn std::error::Error + Send + Sync>> {
    let local_peer = repository.local_identity().peer_id();
    let mut service = MessageService::new(repository, local_peer);
    service.cleanup_expired()?;
    Ok(Arc::new(Mutex::new(service)))
}

#[derive(Debug)]
pub enum RuntimeError {
    Operation(MessageError),
    Unavailable,
    Worker(tauri::Error),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(error) => error.fmt(f),
            Self::Unavailable => write!(f, "chat state lock is poisoned"),
            Self::Worker(error) => write!(f, "chat worker failed: {error}"),
        }
    }
}

/// Acquire the shared service only on the blocking worker; never across an await.
pub async fn with_service<T: Send + 'static>(
    state: ChatState,
    operation: impl FnOnce(&mut MessageService<SqliteMessageRepository>) -> Result<T, MessageError>
    + Send
    + 'static,
) -> Result<T, RuntimeError> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut service = state.lock().map_err(|_| RuntimeError::Unavailable)?;
        operation(&mut service).map_err(RuntimeError::Operation)
    })
    .await
    .map_err(RuntimeError::Worker)?
}
