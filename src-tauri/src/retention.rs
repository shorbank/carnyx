use crate::chat_state::{ChatState, with_service};
use serde::Serialize;
use std::time::Duration;
use tauri::Emitter;

#[derive(Clone, Serialize)]
struct ExpiredMessages {
    ids: Vec<String>,
}

pub struct RetentionTask(tauri::async_runtime::JoinHandle<()>);

impl RetentionTask {
    pub fn start<R: tauri::Runtime>(app: tauri::AppHandle<R>, state: ChatState) -> Self {
        Self::start_with_interval(app, state, Duration::from_secs(60))
    }

    fn start_with_interval<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        state: ChatState,
        interval: Duration,
    ) -> Self {
        Self(tauri::async_runtime::spawn(async move {
            // Sleep after each pass: never replay missed ticks after standby.
            loop {
                tokio::time::sleep(interval).await;
                match with_service(state.clone(), |service| service.cleanup_expired()).await {
                    Ok(ids) => {
                        if !ids.is_empty() {
                            if let Err(error) =
                                app.emit_to("main", "messages_expired", ExpiredMessages { ids })
                            {
                                eprintln!("Could not notify chat of expired messages: {error}");
                            }
                        }
                    }
                    Err(error) => eprintln!("Message cleanup failed: {error}"),
                }
            }
        }))
    }

    pub fn stop(&self) {
        self.0.abort();
    }
}

impl Drop for RetentionTask {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        message::Message, message_service::MessageRepository,
        sqlite_repository::SqliteMessageRepository,
    };
    use std::sync::mpsc;
    use tauri::Listener;

    #[test]
    fn periodic_cleanup_emits_only_successful_deletions_and_stops_on_shutdown() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let state = crate::chat_state::initialize(&path).unwrap();
        let mut repository = SqliteMessageRepository::open(&path).unwrap();
        let old = Message::local("expire", 0, repository.local_identity().peer_id()).unwrap();
        repository.insert(&old).unwrap();
        let connection = crate::encrypted_storage::test_connection(&path).unwrap();
        connection.execute_batch("CREATE TRIGGER fail_delete BEFORE DELETE ON messages BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        let app = tauri::test::mock_app();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let (sender, receiver) = mpsc::channel();
        window.listen("messages_expired", move |event| {
            let _ = sender.send(event.payload().to_owned());
        });
        let task = RetentionTask::start_with_interval(
            app.handle().clone(),
            state,
            Duration::from_millis(50),
        );
        assert!(receiver.recv_timeout(Duration::from_millis(150)).is_err());
        assert!(repository.find_by_id(old.id()).unwrap().is_some());
        connection
            .execute_batch("DROP TRIGGER fail_delete;")
            .unwrap();
        let event: serde_json::Value =
            serde_json::from_str(&receiver.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap();
        assert_eq!(event, serde_json::json!({"ids": [old.id()]}));
        assert!(repository.find_by_id(old.id()).unwrap().is_none());
        task.stop();
        let after_stop = Message::local(
            "keep after shutdown",
            0,
            repository.local_identity().peer_id(),
        )
        .unwrap();
        repository.insert(&after_stop).unwrap();
        assert!(receiver.recv_timeout(Duration::from_millis(150)).is_err());
        assert!(repository.find_by_id(after_stop.id()).unwrap().is_some());
    }
}
