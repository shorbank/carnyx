use super::Result;
use crate::{
    clock::Clock,
    identity::PeerId,
    message::Message,
    message_service::{MessageRepository, MessageService},
};
use std::time::Duration;

#[derive(Debug)]
pub struct PendingMessage {
    pub message: Message,
    pub attempts: u32,
}
#[derive(Debug)]
pub struct DeliveredMessage {
    pub message: Message,
    pub acknowledged_at: Option<i64>,
}

pub trait DeliveryRepository: MessageRepository {
    fn pending(&self, peer: PeerId, cutoff: i64) -> Result<Option<PendingMessage>>;
    fn record_attempt(&mut self, id: &str, cutoff: i64) -> Result<bool>;
    fn acknowledge(&mut self, id: &str, peer: PeerId, cutoff: i64, now: i64) -> Result<bool>;
    fn peer_history(&self, peer: PeerId, cutoff: i64) -> Result<Vec<DeliveredMessage>>;
}
impl<R: DeliveryRepository, C: Clock> MessageService<R, C> {
    pub fn next_delivery(&self, peer: PeerId) -> Result<Option<PendingMessage>> {
        self.repository.pending(peer, self.cutoff()?)
    }
    pub fn begin_attempt(&mut self, id: &str) -> Result<bool> {
        let cutoff = self.cutoff()?;
        self.repository.record_attempt(id, cutoff)
    }
    pub fn confirm_delivery(&mut self, id: &str, peer: PeerId) -> Result<bool> {
        let now = self.now_millis()?;
        self.repository.acknowledge(
            id,
            peer,
            now - crate::message_service::RETENTION_MILLIS,
            now,
        )
    }
    pub fn peer_history(&self, peer: PeerId) -> Result<Vec<DeliveredMessage>> {
        self.repository.peer_history(peer, self.cutoff()?)
    }
}
pub fn retry_delay(attempt: u32) -> Duration {
    Duration::from_secs(if attempt >= 5 { 30 } else { 1u64 << attempt })
}
