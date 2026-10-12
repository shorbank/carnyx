//! Tauri adapters and platform composition; domain and transport do not depend on IPC.
use super::{
    PeerError, Result,
    coordinator::{Coordinator, Events, Persistence},
    delivery::PendingMessage,
    keys::{KeyRepository, Keys, OsSecretStore, SecretStore, restore_keys},
    protocol::{Frame, WireMessage},
    types::{Invitation, PeerState, Role, local_addresses, local_endpoint},
};
use crate::{
    chat_state::ChatState,
    identity::PeerId,
    message_service::{MessageService, ReceiveOutcome},
    sqlite_repository::SqliteMessageRepository,
};
use serde::Serialize;
use std::sync::Arc;
use tauri::Emitter;

pub(super) struct Database(pub ChatState);
impl Database {
    fn work<T>(
        &self,
        op: impl FnOnce(&mut MessageService<SqliteMessageRepository>) -> Result<T>,
    ) -> Result<T> {
        let mut service = self
            .0
            .lock()
            .map_err(|_| PeerError::new("internal_error", "chat lock poisoned"))?;
        op(&mut service)
    }
}
impl Persistence for Database {
    fn receive(&self, peer: PeerId, message: WireMessage) -> Result<(String, bool)> {
        self.work(|s| match s.receive_message(peer, message.into())? {
            ReceiveOutcome::Stored(m) => Ok((m.id().into(), true)),
            ReceiveOutcome::Duplicate(m) => Ok((m.id().into(), false)),
        })
    }
    fn next(&self, peer: PeerId) -> Result<Option<PendingMessage>> {
        self.work(|s| s.next_delivery(peer))
    }
    fn attempt(&self, id: &str) -> Result<bool> {
        self.work(|s| s.begin_attempt(id))
    }
    fn acknowledge(&self, peer: PeerId, id: &str) -> Result<bool> {
        self.work(|s| s.confirm_delivery(id, peer))
    }
}
struct AppEvents(tauri::AppHandle);
impl Events for AppEvents {
    fn state_changed(&self, state: PeerState) {
        if let Err(e) = self.0.emit_to("main", "peer_state_changed", state) {
            eprintln!("Peer event: {e}");
        }
    }
    fn messages_changed(&self) {
        if let Err(e) = self.0.emit_to("main", "messages_changed", ()) {
            eprintln!("Message event: {e}");
        }
    }
}
pub struct PeerApp {
    database: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    events: Arc<dyn Events>,
    pub coordinator: Coordinator,
    operation: tokio::sync::Mutex<()>,
    keys: tokio::sync::OnceCell<(PeerId, Keys)>,
}
impl PeerApp {
    pub fn new(app: tauri::AppHandle, state: ChatState) -> Self {
        let secrets = Arc::new(OsSecretStore::new(&app.config().identifier));
        let database = Arc::new(Database(state));
        let events = Arc::new(AppEvents(app));
        Self {
            coordinator: Coordinator::new(database.clone(), events.clone()),
            database,
            secrets,
            events,
            operation: tokio::sync::Mutex::new(()),
            keys: tokio::sync::OnceCell::new(),
        }
    }
    // Secret-store UI/OS calls must never hold the message service lock.
    async fn keys(&self) -> Result<(PeerId, Keys)> {
        self.keys
            .get_or_try_init(|| async {
                let (own, expected) = self
                    .work(|s| Ok((s.local_peer_id(), s.repository.public_key()?)))
                    .await?;
                let secrets = self.secrets.clone();
                let keys = tokio::task::spawn_blocking(move || {
                    restore_keys(secrets.as_ref(), own, expected)
                })
                .await
                .map_err(|e| PeerError::new("key_unavailable", e))??;
                if expected.is_none() {
                    let public = keys.public;
                    self.work(move |s| s.repository.save_public_key(&public))
                        .await?;
                }
                Ok((own, keys))
            })
            .await
            .cloned()
    }
    async fn work<T: Send + 'static>(
        &self,
        op: impl FnOnce(&mut MessageService<SqliteMessageRepository>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let db = self.database.clone();
        tokio::task::spawn_blocking(move || db.work(op))
            .await
            .map_err(|e| PeerError::new("internal_error", e))?
    }
}
#[derive(Serialize)]
pub struct IpcError {
    code: &'static str,
}
impl From<PeerError> for IpcError {
    fn from(e: PeerError) -> Self {
        eprintln!("Peer command: {e}");
        Self { code: e.code }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    peer_id: String,
    public_key: String,
    selected_peer: Option<Invitation>,
    addresses: Vec<std::net::Ipv4Addr>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerMessageDto {
    #[serde(flatten)]
    message: crate::commands::MessageDto,
    delivery_status: Option<&'static str>,
    acknowledged_at: Option<i64>,
}
impl From<super::delivery::DeliveredMessage> for PeerMessageDto {
    fn from(m: super::delivery::DeliveredMessage) -> Self {
        let status = if m.message.direction() == crate::message::Direction::Outgoing {
            Some(if m.acknowledged_at.is_some() {
                "acknowledged"
            } else {
                "pending"
            })
        } else {
            None
        };
        Self {
            message: m.message.into(),
            delivery_status: status,
            acknowledged_at: m.acknowledged_at,
        }
    }
}
#[tauri::command]
pub async fn get_peer_setup(
    state: tauri::State<'_, PeerApp>,
) -> std::result::Result<Setup, IpcError> {
    let _guard = state.operation.lock().await;
    let (peer, keys) = state.keys().await?;
    state
        .work(move |s| {
            let address = std::net::SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 32147);
            Ok(Setup {
                peer_id: peer.to_string(),
                public_key: Invitation::new(peer, &keys.public, address).public_key,
                selected_peer: s.repository.selected_peer()?,
                addresses: local_addresses()?,
            })
        })
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub async fn create_connection_code(
    address: String,
    port: u16,
    state: tauri::State<'_, PeerApp>,
) -> std::result::Result<String, IpcError> {
    let _guard = state.operation.lock().await;
    let (own, keys) = state.keys().await?;
    state
        .work(move |_| {
            let endpoint = local_endpoint(&address, port)?;
            Ok(Invitation::new(own, &keys.public, endpoint).encode())
        })
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub async fn start_peer_session(
    code: String,
    role: Role,
    address: String,
    port: u16,
    state: tauri::State<'_, PeerApp>,
) -> std::result::Result<(), IpcError> {
    let _guard = state.operation.lock().await;
    if state.coordinator.state().phase != "disconnected" {
        return Err(PeerError::new("session_active", "disconnect before selecting peer").into());
    }
    let peer = Invitation::decode(&code)?;
    let copy = peer.clone();
    let (own, keys) = state.keys().await?;
    let endpoint = state
        .work(move |s| {
            if copy.peer()? == own {
                return Err(PeerError::new("invalid_code", "cannot connect to self"));
            }
            let endpoint = local_endpoint(&address, port)?;
            s.repository.select_peer(&copy)?;
            Ok(endpoint)
        })
        .await?;
    state
        .coordinator
        .start(keys, own, peer, role, endpoint)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub async fn disconnect_peer(
    state: tauri::State<'_, PeerApp>,
) -> std::result::Result<(), IpcError> {
    let _guard = state.operation.lock().await;
    state.coordinator.disconnect().await;
    Ok(())
}
#[tauri::command]
pub fn get_peer_state(state: tauri::State<'_, PeerApp>) -> PeerState {
    state.coordinator.state()
}
#[tauri::command]
pub async fn send_peer_message(
    peer_id: String,
    text: String,
    state: tauri::State<'_, PeerApp>,
) -> std::result::Result<PeerMessageDto, IpcError> {
    let _guard = state.operation.lock().await;
    let peer = peer_id
        .parse::<PeerId>()
        .map_err(|e| PeerError::new("invalid_code", e))?;
    if !state.coordinator.connected_to(peer) {
        return Err(PeerError::new("not_connected", "peer session unavailable").into());
    }
    let message = state
        .work(move |s| {
            s.create_outgoing_validated(peer, &text, |m| {
                Frame::Message {
                    message: WireMessage::from(m),
                }
                .encode()
                .map(|_| ())
            })
        })
        .await?;
    state.events.messages_changed();
    state.coordinator.wake();
    Ok(super::delivery::DeliveredMessage {
        message,
        acknowledged_at: None,
    }
    .into())
}
#[tauri::command]
pub async fn list_peer_messages(
    peer_id: String,
    state: tauri::State<'_, PeerApp>,
) -> std::result::Result<Vec<PeerMessageDto>, IpcError> {
    let peer = peer_id
        .parse::<PeerId>()
        .map_err(|e| PeerError::new("invalid_code", e))?;
    state
        .work(move |s| Ok(s.peer_history(peer)?.into_iter().map(Into::into).collect()))
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Barrier,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    struct NoEvents;
    impl Events for NoEvents {
        fn state_changed(&self, _: PeerState) {}
        fn messages_changed(&self) {}
    }
    struct BlockingSecrets {
        entered: mpsc::Sender<()>,
        release: Arc<Barrier>,
        reads: AtomicUsize,
    }
    impl SecretStore for BlockingSecrets {
        fn load(&self, _: PeerId) -> Result<Option<Vec<u8>>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.entered.send(()).unwrap();
            self.release.wait();
            Ok(None)
        }
        fn save(&self, _: PeerId, _: &[u8]) -> Result<()> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn slow_secret_store_does_not_lock_messages_and_success_is_cached() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::chat_state::initialize(dir.path().join("messages.sqlite")).unwrap();
        let (tx, rx) = mpsc::channel();
        let release = Arc::new(Barrier::new(2));
        let secrets = Arc::new(BlockingSecrets {
            entered: tx,
            release: release.clone(),
            reads: AtomicUsize::new(0),
        });
        let database = Arc::new(Database(state.clone()));
        let events = Arc::new(NoEvents);
        let app = Arc::new(PeerApp {
            coordinator: Coordinator::new(database.clone(), events.clone()),
            database,
            secrets: secrets.clone(),
            events,
            operation: tokio::sync::Mutex::new(()),
            keys: tokio::sync::OnceCell::new(),
        });
        let loading = {
            let app = app.clone();
            tokio::spawn(async move { app.keys().await })
        };
        tokio::task::spawn_blocking(move || {
            rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap()
        })
        .await
        .unwrap();
        let mut service = state
            .try_lock()
            .expect("key lookup must not hold chat lock");
        service.create_message("local remains usable").unwrap();
        drop(service);
        release.wait();
        let first = loading.await.unwrap().unwrap();
        let second = app.keys().await.unwrap();
        assert_eq!(first.0, second.0);
        assert_eq!(first.1.public, second.1.public);
        assert_eq!(secrets.reads.load(Ordering::SeqCst), 1);
        assert_eq!(app.coordinator.state().phase, "disconnected");
    }
}
