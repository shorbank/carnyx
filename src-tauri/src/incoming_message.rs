use crate::{
    identity::PeerId,
    message::{Direction, Message, MessageError, MessageParts, normalize_text},
};
use uuid::{Uuid, Variant, Version};

/// Untrusted input for the domain service, not a network wire protocol.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub id: String,
    pub sender_id: String,
    pub recipient_id: String,
    pub text: String,
    pub created_at: i64,
}

impl IncomingMessage {
    pub(crate) fn validate(
        self,
        source_peer: PeerId,
        local_peer: PeerId,
        received_at: i64,
    ) -> Result<Message, MessageError> {
        let id = Uuid::parse_str(&self.id).map_err(|_| MessageError::InvalidMessageId)?;
        if id.get_version() != Some(Version::Random) || id.get_variant() != Variant::RFC4122 {
            return Err(MessageError::InvalidMessageId);
        }
        let sender_id = self
            .sender_id
            .parse::<PeerId>()
            .map_err(|_| MessageError::InvalidPeerId)?;
        let recipient_id = self
            .recipient_id
            .parse::<PeerId>()
            .map_err(|_| MessageError::InvalidPeerId)?;
        if sender_id != source_peer || sender_id == local_peer {
            return Err(MessageError::UnexpectedSender);
        }
        if recipient_id != local_peer {
            return Err(MessageError::InvalidRecipient);
        }
        Message::from_stored(MessageParts {
            id: id.to_string(),
            text: normalize_text(&self.text)?,
            created_at: self.created_at,
            sender_id,
            recipient_id: Some(recipient_id),
            direction: Direction::Incoming,
            received_at: Some(received_at),
        })
    }
}
