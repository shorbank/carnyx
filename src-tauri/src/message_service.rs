use crate::clock::{Clock, SystemClock};
use crate::message::{Message, MessageError};
use crate::{identity::PeerId, incoming_message::IncomingMessage};

pub const RETENTION_MILLIS: i64 = 86_400_000;

#[derive(Debug, PartialEq, Eq)]
pub enum InsertResult {
    Inserted,
    Existing(Message),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReceiveOutcome {
    Stored(Message),
    Duplicate(Message),
}

pub trait MessageRepository {
    fn insert(&mut self, message: &Message) -> Result<(), MessageError>;
    fn insert_if_absent(&mut self, message: &Message) -> Result<InsertResult, MessageError>;
    fn find_by_id(&self, id: &str) -> Result<Option<Message>, MessageError>;
    fn list_since(&self, cutoff: i64) -> Result<Vec<Message>, MessageError>;
    fn delete_through(&mut self, cutoff: i64) -> Result<Vec<String>, MessageError>;
}

pub struct MessageService<R, C = SystemClock> {
    pub(crate) repository: R,
    clock: C,
    local_peer: PeerId,
}

impl<R: MessageRepository> MessageService<R> {
    pub fn new(repository: R, local_peer: PeerId) -> Self {
        Self::with_clock(repository, SystemClock, local_peer)
    }
}

impl<R: MessageRepository, C: Clock> MessageService<R, C> {
    pub fn with_clock(repository: R, clock: C, local_peer: PeerId) -> Self {
        Self {
            repository,
            clock,
            local_peer,
        }
    }

    pub fn create_message(&mut self, text: &str) -> Result<Message, MessageError> {
        let message = Message::local(text, self.clock.now_millis()?, self.local_peer)?;
        self.repository.insert(&message)?;
        Ok(message)
    }

    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer
    }

    pub fn create_outgoing(
        &mut self,
        recipient: PeerId,
        text: &str,
    ) -> Result<Message, MessageError> {
        self.create_outgoing_validated(recipient, text, |_| Ok::<_, MessageError>(()))
    }

    pub fn create_outgoing_validated<E: From<MessageError>>(
        &mut self,
        recipient: PeerId,
        text: &str,
        validate: impl FnOnce(&Message) -> Result<(), E>,
    ) -> Result<Message, E> {
        let message =
            Message::outgoing(text, self.clock.now_millis()?, self.local_peer, recipient)?;
        validate(&message)?;
        self.repository.insert(&message)?;
        Ok(message)
    }

    pub fn receive_message(
        &mut self,
        source_peer: PeerId,
        incoming: IncomingMessage,
    ) -> Result<ReceiveOutcome, MessageError> {
        let message = incoming.validate(source_peer, self.local_peer, self.clock.now_millis()?)?;
        match self.repository.insert_if_absent(&message)? {
            InsertResult::Inserted => Ok(ReceiveOutcome::Stored(message)),
            InsertResult::Existing(existing) if existing.same_content(&message) => {
                Ok(ReceiveOutcome::Duplicate(existing))
            }
            InsertResult::Existing(_) => Err(MessageError::MessageConflict),
        }
    }

    pub fn list_messages(&self) -> Result<Vec<Message>, MessageError> {
        self.repository.list_since(self.cutoff()?)
    }

    pub fn cleanup_expired(&mut self) -> Result<Vec<String>, MessageError> {
        self.repository.delete_through(self.cutoff()?)
    }

    pub(crate) fn cutoff(&self) -> Result<i64, MessageError> {
        let now = self.clock.now_millis()?;
        crate::message::validate_time(now)?;
        Ok(now - RETENTION_MILLIS)
    }
    pub(crate) fn now_millis(&self) -> Result<i64, MessageError> {
        let now = self.clock.now_millis()?;
        crate::message::validate_time(now)?;
        Ok(now)
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

        fn insert_if_absent(&mut self, message: &Message) -> Result<InsertResult, MessageError> {
            if let Some(existing) = self.find_by_id(message.id())? {
                return Ok(InsertResult::Existing(existing));
            }
            self.insert(message)?;
            Ok(InsertResult::Inserted)
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
                .filter(|m| m.local_time() > cutoff)
                .cloned()
                .collect();
            messages.sort_by_key(Message::local_time);
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
                .filter(|m| m.local_time() <= cutoff)
                .map(|m| m.id().to_owned())
                .collect();
            self.messages.retain(|m| m.local_time() > cutoff);
            Ok(ids)
        }
    }

    #[test]
    fn rejects_empty_text_before_accessing_storage() {
        let mut service = MessageService::new(RecordingRepository::default(), PeerId::generate());
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
        let mut service = MessageService::new(RecordingRepository::default(), PeerId::generate());
        let message = service.create_message("  Hallo  ").unwrap();
        assert_eq!(message.text(), "Hallo");
        assert_eq!(
            service.repository.find_by_id(message.id()).unwrap(),
            Some(message)
        );
    }

    #[test]
    fn propagates_storage_failure_without_returning_a_message() {
        let mut service = MessageService::new(
            RecordingRepository {
                fail: true,
                ..Default::default()
            },
            PeerId::generate(),
        );
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
        let mut service = MessageService::with_clock(
            RecordingRepository::default(),
            FixedClock(1234),
            PeerId::generate(),
        );
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
        let mut service = MessageService::with_clock(
            RecordingRepository::default(),
            FixedClock(0),
            PeerId::generate(),
        );
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

#[cfg(test)]
mod peer_tests {
    use super::*;
    use crate::{
        clock::Clock,
        message::{Direction, MAX_TIMESTAMP_MILLIS},
        sqlite_repository::SqliteMessageRepository,
    };
    use uuid::Uuid;

    struct FixedClock(i64);
    impl Clock for FixedClock {
        fn now_millis(&self) -> Result<i64, MessageError> {
            Ok(self.0)
        }
    }
    type Service = MessageService<SqliteMessageRepository, FixedClock>;
    fn service(path: &std::path::Path, time: i64) -> Service {
        let repository = SqliteMessageRepository::open(path).unwrap();
        let own = repository.local_identity().peer_id();
        MessageService::with_clock(repository, FixedClock(time), own)
    }
    fn input(source: PeerId, local: PeerId) -> IncomingMessage {
        IncomingMessage {
            id: Uuid::new_v4().to_string(),
            sender_id: source.to_string(),
            recipient_id: local.to_string(),
            text: " \nHallo 🦀\n  Welt \t".into(),
            created_at: 1,
        }
    }

    #[test]
    fn stores_incoming_original_id_and_time_but_uses_local_receipt_for_order_and_expiry() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = service(&directory.path().join("messages.sqlite"), 100);
        let source = PeerId::generate();
        let local = service.create_message("local").unwrap();
        service.clock.0 = 101;
        let mut incoming = input(source, service.local_peer_id());
        incoming.id = incoming.id.to_uppercase();
        incoming.sender_id = incoming.sender_id.to_uppercase();
        incoming.created_at = MAX_TIMESTAMP_MILLIS;
        let outcome = service.receive_message(source, incoming.clone()).unwrap();
        let ReceiveOutcome::Stored(received) = outcome else {
            panic!("expected stored message")
        };
        assert_eq!(received.id(), incoming.id.to_lowercase());
        assert_eq!(received.created_at(), MAX_TIMESTAMP_MILLIS);
        assert_eq!(received.received_at(), Some(101));
        assert_eq!(received.direction(), Direction::Incoming);
        assert_eq!(received.text(), "Hallo 🦀\n  Welt");
        assert_eq!(
            service.list_messages().unwrap(),
            vec![local, received.clone()]
        );
        service.clock.0 = 101 + RETENTION_MILLIS - 1;
        assert_eq!(service.list_messages().unwrap(), vec![received.clone()]);
        service.clock.0 += 1;
        assert!(service.list_messages().unwrap().is_empty());
        assert!(
            service
                .cleanup_expired()
                .unwrap()
                .contains(&received.id().to_owned())
        );
    }

    #[test]
    fn duplicates_never_extend_lifetime_and_may_be_received_again_only_after_purge() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = service(&directory.path().join("messages.sqlite"), 100);
        let source = PeerId::generate();
        let incoming = input(source, service.local_peer_id());
        let ReceiveOutcome::Stored(original) =
            service.receive_message(source, incoming.clone()).unwrap()
        else {
            panic!()
        };
        service.clock.0 = 100 + RETENTION_MILLIS;
        let repeated = service.receive_message(source, incoming.clone()).unwrap();
        assert_eq!(repeated, ReceiveOutcome::Duplicate(original.clone()));
        assert!(service.list_messages().unwrap().is_empty());
        assert_eq!(service.cleanup_expired().unwrap(), vec![original.id()]);
        let ReceiveOutcome::Stored(new_receipt) =
            service.receive_message(source, incoming).unwrap()
        else {
            panic!()
        };
        assert_eq!(new_receipt.received_at(), Some(100 + RETENTION_MILLIS));
    }

    #[test]
    fn validates_incoming_addresses_ids_text_and_times_before_storage() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = service(&directory.path().join("messages.sqlite"), 100);
        let source = PeerId::generate();
        let valid = input(source, service.local_peer_id());
        let mut invalid = valid.clone();
        invalid.id = "not-an-id".into();
        assert!(matches!(
            service.receive_message(source, invalid),
            Err(MessageError::InvalidMessageId)
        ));
        let mut invalid = valid.clone();
        invalid.sender_id = "bad-peer".into();
        assert!(matches!(
            service.receive_message(source, invalid),
            Err(MessageError::InvalidPeerId)
        ));
        let mut invalid = valid.clone();
        invalid.sender_id = PeerId::generate().to_string();
        assert!(matches!(
            service.receive_message(source, invalid),
            Err(MessageError::UnexpectedSender)
        ));
        let mut invalid = valid.clone();
        invalid.recipient_id = PeerId::generate().to_string();
        assert!(matches!(
            service.receive_message(source, invalid),
            Err(MessageError::InvalidRecipient)
        ));
        let mut invalid = valid.clone();
        invalid.text = " \n\t".into();
        assert!(matches!(
            service.receive_message(source, invalid),
            Err(MessageError::EmptyText)
        ));
        for time in [-1, MAX_TIMESTAMP_MILLIS + 1] {
            let mut invalid = valid.clone();
            invalid.created_at = time;
            assert!(matches!(
                service.receive_message(source, invalid),
                Err(MessageError::InvalidSystemTime)
            ));
        }
        let own = service.local_peer_id();
        assert!(matches!(
            service.receive_message(own, input(own, own)),
            Err(MessageError::UnexpectedSender)
        ));
        service.clock.0 = -1;
        assert!(matches!(
            service.receive_message(source, valid),
            Err(MessageError::InvalidSystemTime)
        ));
        assert!(service.repository.list_since(-1).unwrap().is_empty());
    }

    #[test]
    fn distinguishes_local_and_outgoing_and_rejects_self_addressing() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = service(&directory.path().join("messages.sqlite"), 100);
        let own = service.local_peer_id();
        let peer = PeerId::generate();
        let local = service.create_message("local").unwrap();
        let outgoing = service.create_outgoing(peer, "targeted").unwrap();
        assert_eq!(local.sender_id(), own);
        assert_eq!(local.recipient_id(), None);
        assert_eq!(outgoing.sender_id(), own);
        assert_eq!(outgoing.recipient_id(), Some(peer));
        assert_eq!(outgoing.direction(), Direction::Outgoing);
        assert_eq!(outgoing.received_at(), None);
        assert!(matches!(
            service.create_outgoing(own, "self"),
            Err(MessageError::InvalidRecipient)
        ));
        let mut collision = input(peer, own);
        collision.id = outgoing.id().into();
        assert!(matches!(
            service.receive_message(peer, collision),
            Err(MessageError::MessageConflict)
        ));
        assert_eq!(
            service.repository.find_by_id(outgoing.id()).unwrap(),
            Some(outgoing)
        );
    }

    #[test]
    fn changed_content_time_or_sender_conflicts_without_overwriting_original() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = service(&directory.path().join("messages.sqlite"), 100);
        let peer = PeerId::generate();
        let valid = input(peer, service.local_peer_id());
        let ReceiveOutcome::Stored(original) =
            service.receive_message(peer, valid.clone()).unwrap()
        else {
            panic!()
        };
        let mut changed = valid.clone();
        changed.text = "different".into();
        assert!(matches!(
            service.receive_message(peer, changed),
            Err(MessageError::MessageConflict)
        ));
        let mut changed = valid.clone();
        changed.created_at += 1;
        assert!(matches!(
            service.receive_message(peer, changed),
            Err(MessageError::MessageConflict)
        ));
        let other_peer = PeerId::generate();
        let mut changed = valid;
        changed.sender_id = other_peer.to_string();
        assert!(matches!(
            service.receive_message(other_peer, changed),
            Err(MessageError::MessageConflict)
        ));
        assert_eq!(service.list_messages().unwrap(), vec![original]);
    }

    #[test]
    fn independent_connections_receive_one_message_atomically() {
        use std::sync::{Arc, Barrier};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let first = service(&path, 100);
        let second = service(&path, 101);
        let own = first.local_peer_id();
        assert_eq!(own, second.local_peer_id());
        let peer = PeerId::generate();
        let incoming = input(peer, own);
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = [first, second]
            .into_iter()
            .map(|mut service| {
                let barrier = barrier.clone();
                let incoming = incoming.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    service.receive_message(peer, incoming).unwrap()
                })
            })
            .collect();
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert_eq!(
            results
                .iter()
                .filter(|r| matches!(r, ReceiveOutcome::Stored(_)))
                .count(),
            1
        );
        assert_eq!(
            results
                .iter()
                .filter(|r| matches!(r, ReceiveOutcome::Duplicate(_)))
                .count(),
            1
        );
        let messages = service(&path, 102).list_messages().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id(), incoming.id);
    }

    #[test]
    fn receive_storage_errors_are_not_reported_as_duplicates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let mut service = service(&path, 100);
        let connection = crate::encrypted_storage::test_connection(&path).unwrap();
        connection.execute_batch("CREATE TRIGGER fail_insert BEFORE INSERT ON messages BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        let peer = PeerId::generate();
        assert!(matches!(
            service.receive_message(peer, input(peer, service.local_peer_id())),
            Err(MessageError::Storage(_))
        ));
        assert!(service.list_messages().unwrap().is_empty());
    }
}
