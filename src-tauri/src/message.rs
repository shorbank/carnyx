use std::{error::Error, fmt};
use uuid::Uuid;

#[derive(Debug)]
pub enum MessageError {
    EmptyText,
    InvalidSystemTime,
    Storage(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyText => write!(f, "message text must not be empty"),
            Self::InvalidSystemTime => write!(f, "system time is outside the supported Unix range"),
            Self::Storage(error) => write!(f, "message storage failed: {error}"),
        }
    }
}

impl Error for MessageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    id: String,
    text: String,
    created_at: i64,
}

impl Message {
    pub(crate) fn new(text: &str, created_at: i64) -> Result<Self, MessageError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(MessageError::EmptyText);
        }
        if created_at < 0 {
            return Err(MessageError::InvalidSystemTime);
        }
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            text: text.to_owned(),
            created_at,
        })
    }

    // Rehydration preserves the exact identity and timestamp already stored.
    pub(crate) fn from_stored(id: String, text: String, created_at: i64) -> Self {
        Self {
            id,
            text,
            created_at,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn created_at(&self) -> i64 {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_inner_whitespace_unicode_and_literal_text() {
        let message = Message::new(" \nGrüß dich 🦀\n  <b>Welt</b>\t ", 1234).unwrap();
        assert_eq!(message.text(), "Grüß dich 🦀\n  <b>Welt</b>");
    }

    #[test]
    fn creates_distinct_v4_ids_and_millisecond_timestamps() {
        let first = Message::new("Hallo", 1234).unwrap();
        let second = Message::new("Hallo", 1234).unwrap();
        assert_ne!(first.id(), second.id());
        for message in [first, second] {
            let id = Uuid::parse_str(message.id()).unwrap();
            assert_eq!(id.get_version(), Some(uuid::Version::Random));
            assert_eq!(message.created_at(), 1234);
        }
    }

    #[test]
    fn rejects_times_before_the_epoch() {
        assert!(matches!(
            Message::new("Hallo", -1),
            Err(MessageError::InvalidSystemTime)
        ));
    }
}
