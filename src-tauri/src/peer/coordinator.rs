//! Session ownership and delivery orchestration. UI and SQL are adapters.
use super::{
    PeerError, Result,
    delivery::{PendingMessage, retry_delay},
    keys::Keys,
    protocol::{Frame, WireMessage},
    transport::{self, Session},
    types::{Invitation, PeerState, Role},
};
use crate::identity::PeerId;
use std::{
    net::SocketAddrV4,
    sync::{Arc, Mutex},
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex as AsyncMutex, Notify},
    task::JoinHandle,
    time::{Duration, Instant, timeout},
};
use tokio_util::sync::CancellationToken;

pub trait Persistence: Send + Sync {
    fn receive(&self, peer: PeerId, message: WireMessage) -> Result<(String, bool)>;
    fn next(&self, peer: PeerId) -> Result<Option<PendingMessage>>;
    fn attempt(&self, id: &str) -> Result<bool>;
    fn acknowledge(&self, peer: PeerId, id: &str) -> Result<bool>;
}
pub trait Events: Send + Sync {
    fn state_changed(&self, state: PeerState);
    fn messages_changed(&self);
}
async fn database<T: Send + 'static>(
    db: Arc<dyn Persistence>,
    f: impl FnOnce(&dyn Persistence) -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(move || f(db.as_ref()))
        .await
        .map_err(|e| PeerError::new("internal_error", e))?
}
struct Control {
    state: PeerState,
    generation: u64,
    active: Option<(CancellationToken, JoinHandle<()>)>,
}
struct Shared {
    control: Mutex<Control>,
    wake: Notify,
    events: Arc<dyn Events>,
}
impl Shared {
    fn publish(&self, generation: u64, phase: &'static str, error: Option<&'static str>) {
        let state = {
            let mut control = self.control.lock().expect("coordinator lock");
            if control.generation != generation {
                return;
            }
            control.state.phase = phase;
            control.state.error = error;
            control.state.revision += 1;
            control.state.clone()
        };
        self.events.state_changed(state);
    }
}
pub struct Coordinator {
    shared: Arc<Shared>,
    db: Arc<dyn Persistence>,
    lifecycle: AsyncMutex<()>,
}
impl Coordinator {
    pub fn new(db: Arc<dyn Persistence>, events: Arc<dyn Events>) -> Self {
        Self {
            shared: Arc::new(Shared {
                control: Mutex::new(Control {
                    state: PeerState::default(),
                    generation: 0,
                    active: None,
                }),
                wake: Notify::new(),
                events,
            }),
            db,
            lifecycle: AsyncMutex::new(()),
        }
    }
    pub fn state(&self) -> PeerState {
        self.shared
            .control
            .lock()
            .expect("coordinator lock")
            .state
            .clone()
    }
    pub fn wake(&self) {
        self.shared.wake.notify_one();
    }
    pub fn connected_to(&self, peer: PeerId) -> bool {
        let s = self.state();
        s.phase == "connected" && s.peer_id.as_deref() == Some(peer.to_string().as_str())
    }
    pub async fn start(
        &self,
        keys: Keys,
        own: PeerId,
        peer: Invitation,
        role: Role,
        endpoint: SocketAddrV4,
    ) -> Result<()> {
        let _guard = self.lifecycle.lock().await;
        if self.state().phase != "disconnected" {
            return Err(PeerError::new(
                "session_active",
                "disconnect before changing peer",
            ));
        }
        let listener = if role == Role::Responder {
            Some(TcpListener::bind(endpoint).await?)
        } else {
            None
        };
        let cancel = CancellationToken::new();
        let generation = {
            let mut c = self.shared.control.lock().expect("coordinator lock");
            c.generation += 1;
            c.state.peer_id = Some(peer.peer_id.clone());
            c.generation
        };
        self.shared.publish(
            generation,
            if role == Role::Responder {
                "listening"
            } else {
                "connecting"
            },
            None,
        );
        let shared = self.shared.clone();
        let db = self.db.clone();
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            run(
                shared, db, generation, token, keys, own, peer, role, endpoint, listener,
            )
            .await;
        });
        let old = self
            .shared
            .control
            .lock()
            .expect("coordinator lock")
            .active
            .replace((cancel, task));
        if let Some((token, task)) = old {
            token.cancel();
            task.abort();
        }
        Ok(())
    }
    pub async fn disconnect(&self) {
        let _guard = self.lifecycle.lock().await;
        let (generation, active) = {
            let mut c = self.shared.control.lock().expect("coordinator lock");
            (c.generation, c.active.take())
        };
        if let Some((token, mut task)) = active {
            self.shared.publish(generation, "disconnecting", None);
            token.cancel();
            if timeout(Duration::from_secs(2), &mut task).await.is_err() {
                task.abort();
                let _ = task.await;
            }
        }
        self.shared.publish(generation, "disconnected", None);
    }
    pub fn abort(&self) {
        if let Some((token, task)) = self
            .shared
            .control
            .lock()
            .expect("coordinator lock")
            .active
            .take()
        {
            token.cancel();
            task.abort();
        }
    }
}
impl Drop for Coordinator {
    fn drop(&mut self) {
        self.abort();
    }
}

#[allow(clippy::too_many_arguments)]
async fn run(
    shared: Arc<Shared>,
    db: Arc<dyn Persistence>,
    generation: u64,
    cancel: CancellationToken,
    keys: Keys,
    own: PeerId,
    peer: Invitation,
    role: Role,
    endpoint: SocketAddrV4,
    mut listener: Option<TcpListener>,
) {
    let mut attempt = 0u32;
    loop {
        let established = tokio::select! {
            _=cancel.cancelled()=>break,
            result=async {
                let stream=if role==Role::Initiator {
                    shared.publish(generation,"connecting",None);
                    timeout(Duration::from_secs(5),TcpStream::connect(peer.address)).await.map_err(|_|PeerError::new("timeout","connect timeout"))??
                } else {
                    shared.publish(generation,"listening",None);
                    let bound=match listener.take(){Some(l)=>l,None=>TcpListener::bind(endpoint).await?};
                    let (stream,_)=bound.accept().await?;stream
                };
                shared.publish(generation,"handshaking",None);
                transport::handshake(stream,&keys,own,&peer,role==Role::Initiator).await
            }=>result
        };
        let result = match established {
            Ok(session) => {
                attempt = 0;
                shared.publish(generation, "connected", None);
                exchange(session, db.clone(), shared.clone(), cancel.clone()).await
            }
            Err(e)
                if role == Role::Responder
                    && matches!(e.code, "authentication_failed" | "protocol_error") =>
            {
                eprintln!("Rejected connection: {e}");
                continue;
            }
            Err(e) => Err(e),
        };
        if cancel.is_cancelled() {
            break;
        }
        if let Err(error) = result {
            eprintln!("Peer session: {error}");
            if !error.transient() {
                shared.publish(generation, "disconnected", Some(error.code));
                return;
            }
            shared.publish(generation, "reconnecting", Some(error.code));
        }
        tokio::select! {_=cancel.cancelled()=>break,_=tokio::time::sleep(retry_delay(attempt))=>{}}
        attempt = attempt.saturating_add(1);
    }
}

struct Received {
    ack: Option<String>,
    pong: Option<u64>,
}

async fn receive_frame(
    session: &mut Session,
    frame: Frame,
    db: Arc<dyn Persistence>,
    events: &dyn Events,
) -> Result<Received> {
    let peer = session.peer;
    let mut received = Received {
        ack: None,
        pong: None,
    };
    match frame {
        Frame::Message { message } => {
            match database(db, move |db| db.receive(peer, message)).await {
                Ok((id, inserted)) => {
                    if inserted {
                        events.messages_changed();
                    }
                    session.send(&Frame::Ack { id }).await?;
                }
                Err(error) if error.code == "storage_error" => {
                    eprintln!("Incoming storage: {error}")
                }
                Err(error) => return Err(error),
            }
        }
        Frame::Ack { id } => {
            let copy = id.clone();
            match database(db, move |db| db.acknowledge(peer, &copy)).await {
                Ok(changed) => {
                    if changed {
                        events.messages_changed();
                    }
                    received.ack = Some(id);
                }
                Err(error) if error.code == "storage_error" => {
                    eprintln!("Receipt storage: {error}")
                }
                Err(error) => return Err(error),
            }
        }
        Frame::Ping { id } => session.send(&Frame::Pong { id }).await?,
        Frame::Pong { id } => received.pong = Some(id),
        Frame::Close => return Err(PeerError::new("network_error", "peer disconnected")),
        Frame::Ready { .. } => return Err(PeerError::new("protocol_error", "unexpected ready")),
    }
    Ok(received)
}

async fn exchange(
    mut session: Session,
    db: Arc<dyn Persistence>,
    shared: Arc<Shared>,
    cancel: CancellationToken,
) -> Result<()> {
    let peer = session.peer;
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut inflight: Option<(String, Instant, u32)> = None;
    let mut next_attempt = Instant::now();
    let mut scan_requested = true;
    let mut last_received = Instant::now();
    let mut ping: Option<(u64, Instant)> = None;
    let mut ping_id = 0u64;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                session.close().await;
                return Ok(());
            }
            frame = session.receive() => {
                let received = receive_frame(&mut session, frame?, db.clone(), shared.events.as_ref()).await?;
                last_received = Instant::now();
                if received.ack.as_ref().is_some_and(|id| inflight.as_ref().is_some_and(|(pending, _, _)| pending == id)) {
                    inflight = None;
                    scan_requested = true;
                    next_attempt = Instant::now();
                }
                if received.pong.is_some_and(|id| ping.is_some_and(|(expected, _)| expected == id)) {
                    ping = None;
                }
            }
            _ = shared.wake.notified() => {
                if inflight.is_none() {
                    scan_requested = true;
                    next_attempt = Instant::now();
                }
            }
            _ = tick.tick() => {
                let now = Instant::now();
                if ping.is_some_and(|(_, deadline)| now >= deadline) {
                    return Err(PeerError::new("timeout", "heartbeat timeout"));
                }
                if ping.is_none() && now.duration_since(last_received) >= Duration::from_secs(15) {
                    ping_id = ping_id.wrapping_add(1);
                    session.send(&Frame::Ping { id: ping_id }).await?;
                    ping = Some((ping_id, now + Duration::from_secs(10)));
                }
                if let Some((_, deadline, attempt)) = &inflight {
                    if now >= *deadline {
                        next_attempt = now + retry_delay(*attempt);
                        inflight = None;
                        scan_requested = true;
                    }
                }
                // Read the outbox only on connect, enqueue, ACK or retry, not on idle ticks.
                if inflight.is_none() && scan_requested && now >= next_attempt {
                    let pending = database(db.clone(), move |db| db.next(peer)).await?;
                    scan_requested = pending.is_some();
                    next_attempt = now + Duration::from_secs(1);
                    if let Some(pending) = pending {
                        let id = pending.message.id().to_owned();
                        let copy = id.clone();
                        if database(db.clone(), move |db| db.attempt(&copy)).await? {
                            session.send(&Frame::Message { message: WireMessage::from(&pending.message) }).await?;
                            inflight = Some((id, Instant::now() + Duration::from_secs(10), pending.attempts));
                            scan_requested = false;
                        }
                    }
                }
            }
        }
    }
}
