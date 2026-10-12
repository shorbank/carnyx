use crate::message::{Message, MessageError};

pub trait MessageRepository {
    fn insert(&mut self, message: &Message) -> Result<(), MessageError>;
    fn find_by_id(&self, id: &str) -> Result<Option<Message>, MessageError>;
}

pub struct MessageService<R> {
    repository: R,
}

impl<R: MessageRepository> MessageService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn create_message(&mut self, text: &str) -> Result<Message, MessageError> {
        let message = Message::new(text)?;
        self.repository.insert(&message)?;
        Ok(message)
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
}
