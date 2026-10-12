use super::{PeerError, Result};
use crate::{incoming_message::IncomingMessage, message::Message};
use serde::{Deserialize, Serialize};

pub const MAX_JSON: usize = 60 * 1024;
pub const MAX_FRAME: usize = 65535;
pub const NOISE: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";
pub const PROLOGUE: &[u8] = b"Carnyx peer protocol v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WireMessage {
    pub id: String,
    pub text: String,
    pub sender_id: String,
    pub recipient_id: String,
    pub created_at: i64,
}
impl From<&Message> for WireMessage {
    fn from(m: &Message) -> Self {
        Self {
            id: m.id().into(),
            text: m.text().into(),
            sender_id: m.sender_id().to_string(),
            recipient_id: m
                .recipient_id()
                .expect("peer message has recipient")
                .to_string(),
            created_at: m.created_at(),
        }
    }
}
impl From<WireMessage> for IncomingMessage {
    fn from(m: WireMessage) -> Self {
        Self {
            id: m.id,
            text: m.text,
            sender_id: m.sender_id,
            recipient_id: m.recipient_id,
            created_at: m.created_at,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Frame {
    Ready {
        version: u8,
        peer_id: String,
        recipient_id: String,
    },
    Message {
        message: WireMessage,
    },
    Ack {
        id: String,
    },
    Ping {
        id: u64,
    },
    Pong {
        id: u64,
    },
    Close,
}
impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self).map_err(|e| PeerError::new("protocol_error", e))?;
        if bytes.len() > MAX_JSON {
            return Err(PeerError::new(
                "message_too_large",
                "peer frame exceeds 60 KiB",
            ));
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_JSON {
            return Err(PeerError::new("protocol_error", "oversized plaintext"));
        }
        serde_json::from_slice(bytes).map_err(|e| PeerError::new("protocol_error", e))
    }
}
