use crate::message::{Message, MessageError};
use serde::Serialize;

use crate::chat_state::{ChatState, RuntimeError, with_service};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDto {
    id: String,
    text: String,
    created_at: i64,
}

impl From<Message> for MessageDto {
    fn from(message: Message) -> Self {
        Self {
            id: message.id().to_owned(),
            text: message.text().to_owned(),
            created_at: message.created_at(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CommandError {
    code: &'static str,
}

impl From<MessageError> for CommandError {
    fn from(error: MessageError) -> Self {
        eprintln!("Chat operation failed: {error}");
        Self {
            code: match error {
                MessageError::EmptyText => "empty_text",
                MessageError::InvalidSystemTime => "invalid_time",
                MessageError::Storage(_) => "storage_error",
            },
        }
    }
}

impl From<RuntimeError> for CommandError {
    fn from(error: RuntimeError) -> Self {
        match error {
            RuntimeError::Operation(error) => Self::from(error),
            other => {
                eprintln!("Chat operation failed: {other}");
                Self {
                    code: "internal_error",
                }
            }
        }
    }
}

#[tauri::command]
pub async fn list_messages(
    state: tauri::State<'_, ChatState>,
) -> Result<Vec<MessageDto>, CommandError> {
    with_service(state.inner().clone(), |service| {
        Ok(service
            .list_messages()?
            .into_iter()
            .map(MessageDto::from)
            .collect())
    })
    .await
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn send_message(
    text: String,
    state: tauri::State<'_, ChatState>,
) -> Result<MessageDto, CommandError> {
    with_service(state.inner().clone(), move |service| {
        service.create_message(&text).map(MessageDto::from)
    })
    .await
    .map_err(CommandError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serializes_message_and_sanitizes_storage_errors() {
        let dto = MessageDto::from(Message::new("Hallo", 1234).unwrap());
        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["createdAt"], 1234);
        assert!(json.get("created_at").is_none());
        let error = CommandError::from(MessageError::Storage(Box::new(std::io::Error::other(
            "private path",
        ))));
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            serde_json::json!({"code": "storage_error"})
        );
    }

    fn app(path: &std::path::Path) -> tauri::App<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .manage(crate::chat_state::initialize(path).unwrap())
            .invoke_handler(tauri::generate_handler![list_messages, send_message])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap()
    }

    fn invoke(
        window: &tauri::WebviewWindow<tauri::test::MockRuntime>,
        command: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, serde_json::Value> {
        tauri::test::get_ipc_response(
            window,
            tauri::webview::InvokeRequest {
                cmd: command.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: if cfg!(any(windows, target_os = "android")) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .unwrap(),
                body: tauri::ipc::InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.into(),
            },
        )
        .map(|response| response.deserialize().unwrap())
    }

    #[test]
    fn real_commands_save_once_and_reload_identical_history_after_app_recreation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let first_app = app(&path);
        let window = tauri::WebviewWindowBuilder::new(&first_app, "main", Default::default())
            .build()
            .unwrap();
        let saved = invoke(
            &window,
            "send_message",
            serde_json::json!({"text": " Hallo 🦀\nWelt "}),
        )
        .unwrap();
        assert_eq!(saved["text"], "Hallo 🦀\nWelt");
        assert_eq!(
            invoke(&window, "list_messages", serde_json::json!({})).unwrap(),
            serde_json::json!([saved])
        );
        drop(window);
        drop(first_app);
        let second_app = app(&path);
        let window = tauri::WebviewWindowBuilder::new(&second_app, "main", Default::default())
            .build()
            .unwrap();
        assert_eq!(
            invoke(&window, "list_messages", serde_json::json!({})).unwrap(),
            serde_json::json!([saved])
        );
    }

    #[test]
    fn command_errors_are_sanitized_and_startup_physically_cleans_old_messages() {
        use crate::message_service::MessageRepository;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let mut repository =
            crate::sqlite_repository::SqliteMessageRepository::open(&path).unwrap();
        let old = Message::new("old", 0).unwrap();
        repository.insert(&old).unwrap();
        drop(repository);
        let app = app(&path);
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        assert_eq!(
            invoke(&window, "list_messages", serde_json::json!({})).unwrap(),
            serde_json::json!([])
        );
        let connection = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM messages", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            invoke(&window, "send_message", serde_json::json!({"text": "  "})),
            Err(serde_json::json!({"code": "empty_text"}))
        );
        connection.execute_batch("CREATE TRIGGER fail_insert BEFORE INSERT ON messages BEGIN SELECT RAISE(ABORT, 'private detail'); END;").unwrap();
        assert_eq!(
            invoke(
                &window,
                "send_message",
                serde_json::json!({"text": "keep input"})
            ),
            Err(serde_json::json!({"code": "storage_error"}))
        );
    }
}
