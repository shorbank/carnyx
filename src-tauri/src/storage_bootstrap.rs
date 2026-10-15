//! Startup gate: no chat, retention or peer services exist before storage is ready.
use crate::{
    chat_state,
    credentials::OsCredentials,
    encrypted_storage::{self, StorageFailure},
    peer, retention,
    sqlite_repository::SqliteMessageRepository,
};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Initializing,
    Ready,
    Blocked,
}
#[derive(Clone, Serialize)]
pub(crate) struct StorageStatus {
    phase: Phase,
    code: Option<StorageFailure>,
    revision: u64,
}
struct State {
    status: StorageStatus,
    running: bool,
    stopped: bool,
}
impl State {
    fn begin(&mut self) -> bool {
        if self.running || self.stopped || self.status.phase == Phase::Ready {
            return false;
        }
        self.running = true;
        self.status = StorageStatus {
            phase: Phase::Initializing,
            code: None,
            revision: self.status.revision + 1,
        };
        true
    }
    fn finish(&mut self, code: Option<StorageFailure>) {
        self.running = false;
        self.status = StorageStatus {
            phase: if code.is_some() {
                Phase::Blocked
            } else {
                Phase::Ready
            },
            code,
            revision: self.status.revision + 1,
        };
    }
}
pub(crate) struct StorageBootstrap(Mutex<State>);
impl Default for StorageBootstrap {
    fn default() -> Self {
        Self(Mutex::new(State {
            status: StorageStatus {
                phase: Phase::Initializing,
                code: None,
                revision: 0,
            },
            running: false,
            stopped: false,
        }))
    }
}
impl StorageBootstrap {
    pub fn stop(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.stopped = true;
        }
    }
}
fn publish(app: &tauri::AppHandle, state: &State) {
    let _ = app.emit_to("main", "storage_status_changed", state.status.clone());
}

pub(crate) fn start(app: tauri::AppHandle) {
    let storage = app.state::<StorageBootstrap>();
    let Ok(mut state) = storage.0.lock() else {
        return;
    };
    if !state.begin() {
        return;
    }
    publish(&app, &state);
    drop(state);
    let worker_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = tauri::async_runtime::spawn_blocking(move || {
            let path = worker_app
                .path()
                .app_data_dir()
                .map_err(|_| StorageFailure::StorageUnavailable)?
                .join("messages.sqlite");
            let connection =
                encrypted_storage::open(&path, &OsCredentials, &worker_app.config().identifier)?;
            let repository = SqliteMessageRepository::from_connection(connection)
                .map_err(|_| StorageFailure::UnlockFailed)?;
            chat_state::from_repository(repository).map_err(|_| StorageFailure::StorageUnavailable)
        })
        .await
        .unwrap_or(Err(StorageFailure::StorageUnavailable));
        let storage = app.state::<StorageBootstrap>();
        let Ok(mut state) = storage.0.lock() else {
            return;
        };
        // Exit and initialization are serialized; an exiting app never starts peer/retention tasks.
        if state.stopped {
            return;
        }
        let code = match result {
            Ok(chat) => {
                app.manage(chat.clone());
                app.manage(peer::commands::PeerApp::new(app.clone(), chat.clone()));
                app.manage(retention::RetentionTask::start(app.clone(), chat));
                None
            }
            Err(code) => Some(code),
        };
        state.finish(code);
        publish(&app, &state);
    });
}

#[tauri::command]
pub(crate) fn get_storage_status(
    storage: tauri::State<'_, StorageBootstrap>,
) -> Result<StorageStatus, &'static str> {
    storage
        .0
        .lock()
        .map(|state| state.status.clone())
        .map_err(|_| "storage_unavailable")
}
#[tauri::command]
pub(crate) fn retry_storage_init(app: tauri::AppHandle) {
    start(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocked_start_can_retry_but_parallel_or_ready_start_cannot() {
        let storage = StorageBootstrap::default();
        let mut state = storage.0.lock().unwrap();
        assert!(state.begin());
        assert!(!state.begin());
        state.finish(Some(StorageFailure::KeyUnavailable));
        assert_eq!(state.status.phase, Phase::Blocked);
        assert_eq!(state.status.revision, 2);
        assert!(state.begin());
        assert_eq!(state.status.code, None);
        state.finish(None);
        assert_eq!(state.status.phase, Phase::Ready);
        assert!(!state.begin());
    }
    #[test]
    fn exit_prevents_another_storage_worker() {
        let storage = StorageBootstrap::default();
        storage.stop();
        assert!(!storage.0.lock().unwrap().begin());
    }
    #[test]
    fn ipc_status_contains_no_error_details_or_key_material() {
        let status = StorageStatus {
            phase: Phase::Blocked,
            code: Some(StorageFailure::KeyMissing),
            revision: 2,
        };
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            r#"{"phase":"blocked","code":"key_missing","revision":2}"#
        );
    }
}
