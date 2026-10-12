use crate::identity::PeerId;
use std::{error::Error, fmt};
use uuid::Uuid;

pub const MAX_TIMESTAMP_MILLIS: i64 = 8_640_000_000_000_000;

#[derive(Debug)]
pub enum MessageError {
    EmptyText,
    InvalidSystemTime,
    InvalidPeerId,
    InvalidMessageId,
    UnexpectedSender,
    InvalidRecipient,
    InvalidMetadata,
    MessageConflict,
    Storage(Box<dyn Error + Send + Sync>),
}
impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match self {
            Self::EmptyText => "message text must not be empty",
            Self::InvalidSystemTime => "timestamp is outside the supported range",
            Self::InvalidPeerId => "invalid peer ID",
            Self::InvalidMessageId => "invalid incoming message ID",
            Self::UnexpectedSender => "message sender does not match the source peer",
            Self::InvalidRecipient => "invalid message recipient",
            Self::InvalidMetadata => "inconsistent message metadata",
            Self::MessageConflict => "message ID conflicts with existing content",
            Self::Storage(error) => return write!(f, "message storage failed: {error}"),
        };
        write!(f, "{description}")
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Local,
    Outgoing,
    Incoming,
}
impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Outgoing => "outgoing",
            Self::Incoming => "incoming",
        }
    }
    pub(crate) fn from_stored(value: &str) -> Result<Self, MessageError> {
        match value {
            "local" => Ok(Self::Local),
            "outgoing" => Ok(Self::Outgoing),
            "incoming" => Ok(Self::Incoming),
            _ => Err(MessageError::InvalidMetadata),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    parts: MessageParts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MessageParts {
    pub id: String,
    pub text: String,
    pub created_at: i64,
    pub sender_id: PeerId,
    pub recipient_id: Option<PeerId>,
    pub direction: Direction,
    pub received_at: Option<i64>,
}

pub(crate) fn validate_time(value: i64) -> Result<(), MessageError> {
    if !(0..=MAX_TIMESTAMP_MILLIS).contains(&value) {
        return Err(MessageError::InvalidSystemTime);
    }
    Ok(())
}
pub(crate) fn normalize_text(value: &str) -> Result<String, MessageError> {
    let text = value.trim();
    if text.is_empty() {
        return Err(MessageError::EmptyText);
    }
    Ok(text.to_owned())
}

impl Message {
    pub(crate) fn local(
        text: &str,
        created_at: i64,
        sender_id: PeerId,
    ) -> Result<Self, MessageError> {
        Self::from_stored(MessageParts {
            id: Uuid::new_v4().to_string(),
            text: normalize_text(text)?,
            created_at,
            sender_id,
            recipient_id: None,
            direction: Direction::Local,
            received_at: None,
        })
    }
    pub(crate) fn outgoing(
        text: &str,
        created_at: i64,
        sender_id: PeerId,
        recipient_id: PeerId,
    ) -> Result<Self, MessageError> {
        if sender_id == recipient_id {
            return Err(MessageError::InvalidRecipient);
        }
        Self::from_stored(MessageParts {
            id: Uuid::new_v4().to_string(),
            text: normalize_text(text)?,
            created_at,
            sender_id,
            recipient_id: Some(recipient_id),
            direction: Direction::Outgoing,
            received_at: None,
        })
    }
    // Hydration preserves stored text and legacy IDs; untrusted input uses its own validator.
    pub(crate) fn from_stored(parts: MessageParts) -> Result<Self, MessageError> {
        validate_time(parts.created_at)?;
        if let Some(received_at) = parts.received_at {
            validate_time(received_at)?;
        }
        if parts.id.is_empty() || parts.text.trim().is_empty() {
            return Err(MessageError::InvalidMetadata);
        }
        let valid = match parts.direction {
            Direction::Local => parts.recipient_id.is_none() && parts.received_at.is_none(),
            Direction::Outgoing => parts.recipient_id.is_some() && parts.received_at.is_none(),
            Direction::Incoming => parts.recipient_id.is_some() && parts.received_at.is_some(),
        };
        if !valid || parts.recipient_id == Some(parts.sender_id) {
            return Err(MessageError::InvalidMetadata);
        }
        Ok(Self { parts })
    }
    pub fn id(&self) -> &str {
        &self.parts.id
    }
    pub fn text(&self) -> &str {
        &self.parts.text
    }
    pub fn created_at(&self) -> i64 {
        self.parts.created_at
    }
    pub fn sender_id(&self) -> PeerId {
        self.parts.sender_id
    }
    pub fn recipient_id(&self) -> Option<PeerId> {
        self.parts.recipient_id
    }
    pub fn direction(&self) -> Direction {
        self.parts.direction
    }
    pub fn received_at(&self) -> Option<i64> {
        self.parts.received_at
    }
    pub fn local_time(&self) -> i64 {
        self.received_at().unwrap_or(self.created_at())
    }
    pub(crate) fn same_content(&self, other: &Self) -> bool {
        self.id() == other.id()
            && self.text() == other.text()
            && self.created_at() == other.created_at()
            && self.sender_id() == other.sender_id()
            && self.recipient_id() == other.recipient_id()
            && self.direction() == other.direction()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_inner_whitespace_unicode_and_literal_text() {
        let message = Message::local(
            " \nGrüß dich 🦀\n  <b>Welt</b>\t ",
            1234,
            PeerId::generate(),
        )
        .unwrap();
        assert_eq!(message.text(), "Grüß dich 🦀\n  <b>Welt</b>");
    }
    #[test]
    fn creates_distinct_v4_ids_and_millisecond_timestamps() {
        let sender = PeerId::generate();
        let first = Message::local("Hallo", 1234, sender).unwrap();
        let second = Message::local("Hallo", 1234, sender).unwrap();
        assert_ne!(first.id(), second.id());
        for message in [first, second] {
            assert_eq!(
                Uuid::parse_str(message.id()).unwrap().get_version(),
                Some(uuid::Version::Random)
            );
            assert_eq!(message.created_at(), 1234);
            assert_eq!(message.sender_id(), sender);
            assert_eq!(message.direction(), Direction::Local);
            assert_eq!(message.received_at(), None);
        }
    }
    #[test]
    fn rejects_invalid_times_and_direction_combinations() {
        let sender = PeerId::generate();
        for time in [-1, MAX_TIMESTAMP_MILLIS + 1] {
            assert!(matches!(
                Message::local("Hallo", time, sender),
                Err(MessageError::InvalidSystemTime)
            ));
        }
        assert!(Message::outgoing("Hallo", 1234, sender, sender).is_err());
        let local = Message::local("Hallo", 1234, sender).unwrap();
        let mut parts = local.parts;
        parts.direction = Direction::Incoming;
        assert!(matches!(
            Message::from_stored(parts),
            Err(MessageError::InvalidMetadata)
        ));
    }
}
