//! Tauri adapters and platform composition; domain and transport do not depend on IPC.
use super::{
    PeerError, Result,
    coordinator::{Coordinator, Events, Persistence},
    delivery::PendingMessage,
    keys::{KeyRepository, Keys, OsSecretStore, SecretStore, restore_keys},
    protocol::{Frame, WireMessage},
    types::{Invitation, PeerState, SessionSettings, local_addresses, local_endpoint},
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
    identifier: String,
    internet_secrets: Arc<dyn crate::credentials::Credentials>,
    internet_keys: tokio::sync::OnceCell<iroh::SecretKey>,
    keys: tokio::sync::OnceCell<(PeerId, Keys)>,
}
impl PeerApp {
    pub fn new(app: tauri::AppHandle, state: ChatState) -> Self {
        let secrets = Arc::new(OsSecretStore::new(&app.config().identifier));
        let database = Arc::new(Database(state));
        let identifier = app.config().identifier.clone();
        let events = Arc::new(AppEvents(app));
        Self {
            coordinator: Coordinator::new(database.clone(), events.clone()),
            database,
            secrets,
            events,
            operation: tokio::sync::Mutex::new(()),
            identifier,
            internet_secrets: Arc::new(crate::credentials::OsCredentials),
            internet_keys: tokio::sync::OnceCell::new(),
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
    async fn internet_key(&self) -> Result<iroh::SecretKey> {
        self.internet_keys
            .get_or_try_init(|| async {
                let (own, expected) = self
                    .work(|s| Ok((s.local_peer_id(), s.repository.internet_public_key()?)))
                    .await?;
                let store = self.internet_secrets.clone();
                let identifier = self.identifier.clone();
                let key = tokio::task::spawn_blocking(move || {
                    super::internet_keys::restore(store.as_ref(), &identifier, own, expected)
                })
                .await
                .map_err(|_| PeerError::new("key_unavailable", "internet key worker failed"))??;
                if expected.is_none() {
                    let public = *key.public().as_bytes();
                    self.work(move |s| s.repository.save_internet_public_key(&public))
                        .await?;
                }
                Ok(key)
            })
            .await
            .cloned()
    }
    async fn code(&self, settings: SessionSettings) -> Result<String> {
        let _guard = self.operation.lock().await;
        let endpoint = if let SessionSettings::Lan {
            local_address,
            local_port,
            ..
        } = &settings
        {
            let (address, port) = (*local_address, *local_port);
            Some(
                tokio::task::spawn_blocking(move || local_endpoint(&address.to_string(), port))
                    .await
                    .map_err(|e| PeerError::new("internal_error", e))??,
            )
        } else {
            None
        };
        let (own, keys) = self.keys().await?;
        let invitation = if let Some(endpoint) = endpoint {
            Invitation::new(own, &keys.public, endpoint)
        } else {
            Invitation::internet(own, &keys.public, self.internet_key().await?.public())
        };
        Ok(invitation.encode())
    }
    async fn read_setup(&self) -> Result<Setup> {
        // Interface enumeration and secure-store calls never hold the chat lock.
        let addresses = tokio::task::spawn_blocking(local_addresses)
            .await
            .map_err(|e| PeerError::new("internal_error", e))?
            .unwrap_or_default();
        self.work(move |s| {
            let own = s.local_peer_id();
            let selected_peer = s.repository.selected_peer()?;
            let session = selected_peer
                .as_ref()
                .map(|peer| s.repository.saved_session(peer.peer()?))
                .transpose()?
                .flatten();
            let address = std::net::SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 32147);
            Ok(Setup {
                peer_id: own.to_string(),
                public_key: s
                    .repository
                    .public_key()?
                    .map(|key| Invitation::new(own, &key, address).public_key),
                selected_peer,
                session,
                addresses,
            })
        })
        .await
    }
    async fn begin_session(&self, peer: Invitation, settings: SessionSettings) -> Result<()> {
        if self.coordinator.state().phase != "disconnected" {
            return Err(PeerError::new(
                "session_active",
                "disconnect before selecting peer",
            ));
        }
        settings.validate_for(&peer)?;
        peer.validate(true)?;
        if let SessionSettings::Lan {
            local_address,
            local_port,
            ..
        } = &settings
        {
            let (address, port) = (*local_address, *local_port);
            tokio::task::spawn_blocking(move || local_endpoint(&address.to_string(), port))
                .await
                .map_err(|e| PeerError::new("internal_error", e))??;
        }
        let own = self.work(|s| Ok(s.local_peer_id())).await?;
        if peer.peer()? == own {
            return Err(PeerError::new("invalid_code", "cannot connect to self"));
        }
        let (_, keys) = self.keys().await?;
        let internet_key = if matches!(settings, SessionSettings::Internet) {
            Some(self.internet_key().await?)
        } else {
            None
        };
        let copy = peer.clone();
        let saved = settings.clone();
        self.work(move |s| s.repository.select_session(&copy, &saved))
            .await?;
        self.coordinator
            .start_session(keys, own, peer, settings, internet_key)
            .await
    }
    async fn connect(&self, code: &str, settings: SessionSettings) -> Result<()> {
        let _guard = self.operation.lock().await;
        self.begin_session(Invitation::decode(code)?, settings)
            .await
    }
    async fn reconnect(&self) -> Result<()> {
        let _guard = self.operation.lock().await;
        let (peer, settings) = self
            .work(|s| {
                let peer = s
                    .repository
                    .selected_peer()?
                    .ok_or_else(|| PeerError::new("peer_missing", "no saved peer"))?;
                let settings = s.repository.saved_session(peer.peer()?)?.ok_or_else(|| {
                    PeerError::new("session_missing", "no saved session settings")
                })?;
                Ok((peer, settings))
            })
            .await?;
        self.begin_session(peer, settings).await
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
    public_key: Option<String>,
    session: Option<SessionSettings>,
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
    state.read_setup().await.map_err(Into::into)
}
#[tauri::command]
pub async fn create_connection_code(
    settings: SessionSettings,
    state: tauri::State<'_, PeerApp>,
) -> std::result::Result<String, IpcError> {
    state.code(settings).await.map_err(Into::into)
}
#[tauri::command]
pub async fn start_peer_session(
    code: String,
    settings: SessionSettings,
    state: tauri::State<'_, PeerApp>,
) -> std::result::Result<(), IpcError> {
    state.connect(&code, settings).await.map_err(Into::into)
}
#[tauri::command]
pub async fn reconnect_peer(state: tauri::State<'_, PeerApp>) -> std::result::Result<(), IpcError> {
    state.reconnect().await.map_err(Into::into)
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
    use super::super::types::{ConnectionTarget, Role};
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
            identifier: "test.carnyx".into(),
            internet_secrets: Arc::new(crate::credentials::OsCredentials),
            internet_keys: tokio::sync::OnceCell::new(),
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
    #[derive(Default)]
    struct MemorySecrets {
        values: std::sync::Mutex<std::collections::HashMap<PeerId, Vec<u8>>>,
        reads: AtomicUsize,
    }
    impl SecretStore for MemorySecrets {
        fn load(&self, peer: PeerId) -> Result<Option<Vec<u8>>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self.values.lock().unwrap().get(&peer).cloned())
        }
        fn save(&self, peer: PeerId, key: &[u8]) -> Result<()> {
            self.values.lock().unwrap().insert(peer, key.to_vec());
            Ok(())
        }
    }
    fn app(path: &std::path::Path, secrets: Arc<MemorySecrets>) -> PeerApp {
        let database = Arc::new(Database(crate::chat_state::initialize(path).unwrap()));
        let events = Arc::new(NoEvents);
        PeerApp {
            coordinator: Coordinator::new(database.clone(), events.clone()),
            database,
            secrets,
            events,
            operation: tokio::sync::Mutex::new(()),
            identifier: "test.carnyx".into(),
            internet_secrets: Arc::new(crate::credentials::OsCredentials),
            internet_keys: tokio::sync::OnceCell::new(),
            keys: tokio::sync::OnceCell::new(),
        }
    }
    struct InternetSecrets {
        database: ChatState,
        value: std::sync::Mutex<Option<Vec<u8>>>,
        fail: bool,
    }
    impl crate::credentials::Credentials for InternetSecrets {
        fn load(&self, _: &str, _: &str) -> std::result::Result<Option<Vec<u8>>, ()> {
            assert!(
                self.database.try_lock().is_ok(),
                "OS calls must not hold the database lock"
            );
            if self.fail {
                Err(())
            } else {
                Ok(self.value.lock().unwrap().clone())
            }
        }
        fn save(&self, _: &str, _: &str, secret: &[u8]) -> std::result::Result<(), ()> {
            assert!(self.database.try_lock().is_ok());
            *self.value.lock().unwrap() = Some(secret.to_vec());
            Ok(())
        }
    }
    #[tokio::test]
    async fn internet_codes_use_persisted_os_identity_without_starting_a_session_or_blocking_lan() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("messages.sqlite");
        let noise = Arc::new(MemorySecrets::default());
        let mut first = app(&path, noise.clone());
        let credentials = Arc::new(InternetSecrets {
            database: first.database.0.clone(),
            value: std::sync::Mutex::new(None),
            fail: false,
        });
        first.internet_secrets = credentials.clone();
        let code = first.code(SessionSettings::Internet).await.unwrap();
        assert_eq!(first.coordinator.state().phase, "disconnected");
        assert!(first.read_setup().await.unwrap().selected_peer.is_none());
        let invitation = Invitation::decode(&code).unwrap();
        let ConnectionTarget::Internet { endpoint_id, .. } = invitation.target else {
            panic!("internet code")
        };
        assert_eq!(
            first
                .work(|s| s.repository.internet_public_key())
                .await
                .unwrap()
                .unwrap(),
            *endpoint_id.parse::<iroh::EndpointId>().unwrap().as_bytes()
        );
        drop(first);
        let mut restored = app(&path, noise);
        restored.internet_secrets = Arc::new(InternetSecrets {
            database: restored.database.0.clone(),
            value: std::sync::Mutex::new(credentials.value.lock().unwrap().clone()),
            fail: false,
        });
        assert_eq!(
            restored.code(SessionSettings::Internet).await.unwrap(),
            code
        );
        // A broken internet credential store must not prevent LAN or local use.
        restored.internet_keys = tokio::sync::OnceCell::new();
        restored.internet_secrets = Arc::new(InternetSecrets {
            database: restored.database.0.clone(),
            value: std::sync::Mutex::new(None),
            fail: true,
        });
        assert_eq!(
            restored
                .code(SessionSettings::Internet)
                .await
                .unwrap_err()
                .code,
            "key_unavailable"
        );
        assert!(
            restored
                .code(settings(32147, Role::Initiator))
                .await
                .unwrap()
                .starts_with("carnyx:v1:")
        );
        assert_eq!(restored.coordinator.state().phase, "disconnected");
    }
    fn settings(port: u16, role: Role) -> SessionSettings {
        SessionSettings::Lan {
            local_address: std::net::Ipv4Addr::LOCALHOST,
            local_port: port,
            role,
        }
    }
    async fn free_port() -> u16 {
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }
    async fn connected(a: &PeerApp, b: &PeerApp) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while a.coordinator.state().phase != "connected"
                || b.coordinator.state().phase != "connected"
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn setup_is_read_only_and_reconnect_validates_before_accessing_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecrets::default());
        let app = app(&dir.path().join("messages.sqlite"), secrets.clone());
        let setup = app.read_setup().await.unwrap();
        assert!(setup.public_key.is_none());
        assert!(setup.selected_peer.is_none());
        assert!(setup.session.is_none());
        assert_eq!(secrets.reads.load(Ordering::SeqCst), 0);
        assert_eq!(app.coordinator.state().phase, "disconnected");
        assert_eq!(app.reconnect().await.unwrap_err().code, "peer_missing");
        let peer = Invitation::new(
            PeerId::generate(),
            &[1; 32],
            "127.0.0.1:32147".parse().unwrap(),
        );
        let copy = peer.clone();
        app.work(move |s| s.repository.select_peer(&copy))
            .await
            .unwrap();
        assert_eq!(app.reconnect().await.unwrap_err().code, "session_missing");
        let unavailable = ["203.0.113.1", "198.51.100.1", "192.0.2.1"]
            .into_iter()
            .map(|v| v.parse::<std::net::Ipv4Addr>().unwrap())
            .find(|ip| !local_addresses().unwrap().contains(ip))
            .unwrap();
        app.work(move |s| {
            s.repository.select_session(
                &peer,
                &SessionSettings::Lan {
                    local_address: unavailable,
                    local_port: 32147,
                    role: Role::Initiator,
                },
            )
        })
        .await
        .unwrap();
        assert_eq!(app.reconnect().await.unwrap_err().code, "invalid_address");
        assert_eq!(secrets.reads.load(Ordering::SeqCst), 0);
        assert_eq!(app.coordinator.state().phase, "disconnected");
    }
    #[tokio::test]
    async fn occupied_port_preserves_saved_intent_and_failed_storage_never_starts_session() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(
            &dir.path().join("messages.sqlite"),
            Arc::new(MemorySecrets::default()),
        );
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let settings = settings(listener.local_addr().unwrap().port(), Role::Responder);
        let peer = Invitation::new(
            PeerId::generate(),
            &[1; 32],
            "127.0.0.1:32147".parse().unwrap(),
        );
        assert_eq!(
            app.connect(&peer.encode(), settings.clone())
                .await
                .unwrap_err()
                .code,
            "network_error"
        );
        assert_eq!(
            app.read_setup().await.unwrap().session,
            Some(settings.clone())
        );
        assert_eq!(app.reconnect().await.unwrap_err().code, "network_error");
        drop(listener);
        app.work(|s| { s.repository.connection.execute_batch("CREATE TRIGGER fail_settings BEFORE INSERT ON connection_profile BEGIN SELECT RAISE(ABORT,'fail'); END;").map_err(|e| PeerError::new("storage_error", e))?; Ok(()) }).await.unwrap();
        assert_eq!(
            app.connect(&peer.encode(), settings)
                .await
                .unwrap_err()
                .code,
            "storage_error"
        );
        assert_eq!(app.coordinator.state().phase, "disconnected");
    }
    #[tokio::test]
    async fn both_roles_reconnect_after_full_restart_and_resume_original_pending_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let apath = dir.path().join("a.sqlite");
        let bpath = dir.path().join("b.sqlite");
        let asecrets = Arc::new(MemorySecrets::default());
        let bsecrets = Arc::new(MemorySecrets::default());
        let a = app(&apath, asecrets.clone());
        let b = app(&bpath, bsecrets.clone());
        let (aid, ak) = a.keys().await.unwrap();
        let (bid, bk) = b.keys().await.unwrap();
        let asettings = settings(free_port().await, Role::Initiator);
        let bsettings = settings(free_port().await, Role::Responder);
        let acode = Invitation::new(aid, &ak.public, asettings.endpoint().unwrap()).encode();
        let bcode = Invitation::new(bid, &bk.public, bsettings.endpoint().unwrap()).encode();
        b.connect(&acode, bsettings.clone()).await.unwrap();
        a.connect(&bcode, asettings.clone()).await.unwrap();
        connected(&a, &b).await;
        assert_eq!(a.reconnect().await.unwrap_err().code, "session_active");
        a.coordinator.disconnect().await;
        b.coordinator.disconnect().await;
        let original = a
            .work(move |s| {
                s.create_outgoing(bid, "resume original")
                    .map_err(Into::into)
            })
            .await
            .unwrap();
        drop(a);
        drop(b);
        let a = app(&apath, asecrets.clone());
        let b = app(&bpath, bsecrets.clone());
        assert_eq!(a.read_setup().await.unwrap().session, Some(asettings));
        assert_eq!(b.read_setup().await.unwrap().session, Some(bsettings));
        assert_eq!(a.coordinator.state().phase, "disconnected");
        assert_eq!(asecrets.reads.load(Ordering::SeqCst), 1);
        b.reconnect().await.unwrap();
        a.reconnect().await.unwrap();
        connected(&a, &b).await;
        let id = original.id().to_owned();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let acknowledged = a
                    .work(move |s| {
                        Ok(s.peer_history(bid)?
                            .first()
                            .is_some_and(|m| m.acknowledged_at.is_some()))
                    })
                    .await
                    .unwrap();
                if acknowledged {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let received = b
            .work(|s| s.list_messages().map_err(Into::into))
            .await
            .unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].id(), id);
        assert_eq!(received[0].created_at(), original.created_at());
        assert_eq!(asecrets.reads.load(Ordering::SeqCst), 2);
        a.coordinator.disconnect().await;
        b.coordinator.disconnect().await;
    }
}
