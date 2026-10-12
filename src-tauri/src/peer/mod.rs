pub mod commands;
pub mod coordinator;
pub mod delivery;
pub mod keys;
pub mod protocol;
#[cfg(test)]
mod tests;
pub mod transport;
pub mod types;

#[derive(Debug)]
pub struct PeerError {
    pub code: &'static str,
    detail: String,
}
impl PeerError {
    pub fn new(code: &'static str, detail: impl ToString) -> Self {
        Self {
            code,
            detail: detail.to_string(),
        }
    }
    pub fn transient(&self) -> bool {
        matches!(self.code, "network_error" | "timeout" | "storage_error")
    }
}
impl std::fmt::Display for PeerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}
impl std::error::Error for PeerError {}
impl From<std::io::Error> for PeerError {
    fn from(e: std::io::Error) -> Self {
        Self::new("network_error", e)
    }
}
impl From<crate::message::MessageError> for PeerError {
    fn from(e: crate::message::MessageError) -> Self {
        let code = match e {
            crate::message::MessageError::Storage(_) => "storage_error",
            crate::message::MessageError::InvalidSystemTime => "invalid_time",
            crate::message::MessageError::EmptyText => "empty_text",
            _ => "protocol_error",
        };
        Self::new(code, e)
    }
}
pub type Result<T> = std::result::Result<T, PeerError>;
