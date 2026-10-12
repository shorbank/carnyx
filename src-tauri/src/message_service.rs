use crate::clock::{Clock, SystemClock};
use crate::message::{Message, MessageError};

pub const RETENTION_MILLIS: i64 = 86_400_000;

pub trait MessageRepository {
    fn insert(&mut self, message: &Message) -> Result<(), MessageError>;
    fn find_by_id(&self, id: &str) -> Result<Option<Message>, MessageError>;
    fn list_since(&self, cutoff: i64) -> Result<Vec<Message>, MessageError>;
    fn delete_through(&mut self, cutoff: i64) -> Result<Vec<String>, MessageError>;
}

pub struct MessageService<R, C = SystemClock> {
    repository: R,
    clock: C,
}

impl<R: MessageRepository> MessageService<R> {
    pub fn new(repository: R) -> Self {
        Self::with_clock(repository, SystemClock)
    }
}

impl<R: MessageRepository, C: Clock> MessageService<R, C> {
    pub fn with_clock(repository: R, clock: C) -> Self {
        Self { repository, clock }
    }

    pub fn create_message(&mut self, text: &str) -> Result<Message, MessageError> {
        let message = Message::new(text, self.clock.now_millis()?)?;
        self.repository.insert(&message)?;
        Ok(message)
    }

    pub fn list_messages(&self) -> Result<Vec<Message>, MessageError> {
        self.repository.list_since(self.cutoff()?)
    }

    pub fn cleanup_expired(&mut self) -> Result<Vec<String>, MessageError> {
        self.repository.delete_through(self.cutoff()?)
    }

    fn cutoff(&self) -> Result<i64, MessageError> {
        let now = self.clock.now_millis()?;
        if now < 0 {
            return Err(MessageError::InvalidSystemTime);
        }
        Ok(now - RETENTION_MILLIS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[derive(Default)]
    struct RecordingRepository {
        messages: Vec<Message>,
        fail: bool,
    }

    impl MessageRepository for RecordingRepository {
        fn insert(&mut self, message: &Message) -> Result<(), MessageError> {
            if self.fail {
                return Err(MessageError::Storage(Box::new(io::Error::other(
                    "disk failure",
                ))));
            }
            self.messages.push(message.clone());
            Ok(())
        }

        fn find_by_id(&self, id: &str) -> Result<Option<Message>, MessageError> {
            Ok(self
                .messages
                .iter()
                .find(|message| message.id() == id)
                .cloned())
        }

        fn list_since(&self, cutoff: i64) -> Result<Vec<Message>, MessageError> {
            let mut messages: Vec<_> = self
                .messages
                .iter()
                .filter(|m| m.created_at() > cutoff)
                .cloned()
                .collect();
            messages.sort_by_key(Message::created_at);
            Ok(messages)
        }

        fn delete_through(&mut self, cutoff: i64) -> Result<Vec<String>, MessageError> {
            if self.fail {
                return Err(MessageError::Storage(Box::new(io::Error::other(
                    "disk failure",
                ))));
            }
            let ids = self
                .messages
                .iter()
                .filter(|m| m.created_at() <= cutoff)
                .map(|m| m.id().to_owned())
                .collect();
            self.messages.retain(|m| m.created_at() > cutoff);
            Ok(ids)
        }
    }

    #[test]
    fn rejects_empty_text_before_accessing_storage() {
        let mut service = MessageService::new(RecordingRepository::default());
        for text in ["", "  ", "\n\t\r", "\u{2003}"] {
            assert!(matches!(
                service.create_message(text),
                Err(MessageError::EmptyText)
            ));
        }
        assert!(service.repository.messages.is_empty());
    }

    #[test]
    fn returns_the_message_that_was_successfully_stored() {
        let mut service = MessageService::new(RecordingRepository::default());
        let message = service.create_message("  Hallo  ").unwrap();
        assert_eq!(message.text(), "Hallo");
        assert_eq!(
            service.repository.find_by_id(message.id()).unwrap(),
            Some(message)
        );
    }

    #[test]
    fn propagates_storage_failure_without_returning_a_message() {
        let mut service = MessageService::new(RecordingRepository {
            fail: true,
            ..Default::default()
        });
        assert!(matches!(
            service.create_message("Hallo"),
            Err(MessageError::Storage(_))
        ));
        assert!(service.repository.messages.is_empty());
    }

    struct FixedClock(i64);
    impl Clock for FixedClock {
        fn now_millis(&self) -> Result<i64, MessageError> {
            Ok(self.0)
        }
    }

    #[test]
    fn expires_at_exactly_24_hours_and_not_one_millisecond_earlier() {
        let mut service =
            MessageService::with_clock(RecordingRepository::default(), FixedClock(1234));
        let message = service.create_message("Hallo").unwrap();
        service.clock.0 += RETENTION_MILLIS - 1;
        assert_eq!(service.list_messages().unwrap(), vec![message.clone()]);
        assert!(service.cleanup_expired().unwrap().is_empty());
        service.clock.0 += 1;
        assert!(service.list_messages().unwrap().is_empty());
        assert_eq!(service.cleanup_expired().unwrap(), vec![message.id()]);
        service.clock.0 += 1;
        assert!(service.cleanup_expired().unwrap().is_empty());
    }

    #[test]
    fn cleanup_failure_preserves_messages_and_can_be_retried() {
        let mut service = MessageService::with_clock(RecordingRepository::default(), FixedClock(0));
        let message = service.create_message("Hallo").unwrap();
        service.clock.0 = RETENTION_MILLIS;
        service.repository.fail = true;
        assert!(matches!(
            service.cleanup_expired(),
            Err(MessageError::Storage(_))
        ));
        assert_eq!(service.repository.messages, vec![message.clone()]);
        service.repository.fail = false;
        assert_eq!(service.cleanup_expired().unwrap(), vec![message.id()]);
    }
}
