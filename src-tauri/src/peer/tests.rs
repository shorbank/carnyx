use super::*;
use crate::{
    clock::Clock,
    identity::PeerId,
    message_service::{MessageRepository, MessageService, RETENTION_MILLIS},
    sqlite_repository::SqliteMessageRepository,
};
use coordinator::{Coordinator, Events, Persistence};
use delivery::DeliveryRepository;
use keys::{KeyRepository, Keys, SecretStore, load_keys};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddrV4},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    net::{TcpListener, TcpStream},
    time::{Duration, timeout},
};
use types::{Invitation, PeerState, Role};

#[derive(Default)]
struct Secrets(Mutex<HashMap<PeerId, Vec<u8>>>);
impl SecretStore for Secrets {
    fn load(&self, p: PeerId) -> Result<Option<Vec<u8>>> {
        Ok(self.0.lock().unwrap().get(&p).cloned())
    }
    fn save(&self, p: PeerId, b: &[u8]) -> Result<()> {
        self.0.lock().unwrap().insert(p, b.to_vec());
        Ok(())
    }
}
fn repository() -> (tempfile::TempDir, SqliteMessageRepository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = SqliteMessageRepository::open(dir.path().join("messages.sqlite")).unwrap();
    (dir, repo)
}
fn identity(repo: &mut SqliteMessageRepository, secrets: &Secrets) -> (PeerId, Keys) {
    let own = repo.local_identity().peer_id();
    (own, load_keys(repo, secrets, own).unwrap())
}
fn endpoint(port: u16) -> SocketAddrV4 {
    SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)
}

#[test]
fn key_bootstrap_recovers_interrupted_setup_but_never_replaces_missing_or_mismatched_keys() {
    let (_dir, mut repo) = repository();
    let secrets = Secrets::default();
    let own = repo.local_identity().peer_id();
    let raw = snow::Builder::new(protocol::NOISE.parse().unwrap())
        .generate_keypair()
        .unwrap();
    secrets.save(own, &raw.private).unwrap();
    let first = load_keys(&mut repo, &secrets, own).unwrap();
    assert_eq!(first.private.as_slice(), raw.private);
    assert_eq!(
        load_keys(&mut repo, &secrets, own).unwrap().public,
        first.public
    );
    secrets.0.lock().unwrap().clear();
    assert_eq!(
        load_keys(&mut repo, &secrets, own).err().unwrap().code,
        "key_unavailable"
    );
    secrets.save(own, &[99; 32]).unwrap();
    assert_eq!(
        load_keys(&mut repo, &secrets, own).err().unwrap().code,
        "key_unavailable"
    );
    assert_eq!(repo.public_key().unwrap(), Some(first.public));
}
#[test]
fn invitation_roundtrip_and_pinning_reject_changed_keys() {
    let (_dir, mut repo) = repository();
    let peer = Invitation::new(PeerId::generate(), &[1; 32], endpoint(32147));
    assert_eq!(Invitation::decode(&peer.encode()).unwrap(), peer);
    assert!(Invitation::decode("carnyx:v2:bad").is_err());
    assert!(Invitation::decode(&"a".repeat(3000)).is_err());
    repo.select_peer(&peer).unwrap();
    let changed = Invitation::new(peer.peer().unwrap(), &[2; 32], endpoint(32148));
    assert_eq!(
        repo.select_peer(&changed).unwrap_err().code,
        "peer_key_changed"
    );
    assert_eq!(repo.selected_peer().unwrap(), Some(peer));
}

#[test]
fn switching_peer_does_not_forget_previous_key_pins() {
    let (_dir, mut repo) = repository();
    let first = Invitation::new(PeerId::generate(), &[1; 32], endpoint(32147));
    repo.select_peer(&first).unwrap();
    repo.select_peer(&Invitation::new(
        PeerId::generate(),
        &[2; 32],
        endpoint(32148),
    ))
    .unwrap();
    assert_eq!(
        repo.select_peer(&Invitation::new(
            first.peer().unwrap(),
            &[3; 32],
            endpoint(32147)
        ))
        .unwrap_err()
        .code,
        "peer_key_changed"
    );
}

#[test]
fn version_three_migration_preserves_identity_messages_and_backfills_pending_deliveries() {
    let (dir, repo) = repository();
    let path = dir.path().join("messages.sqlite");
    let own = repo.local_identity().peer_id();
    let peer = PeerId::generate();
    let mut service = MessageService::with_clock(repo, Time(100), own);
    let message = service.create_outgoing(peer, "before migration").unwrap();
    drop(service);
    let connection = crate::encrypted_storage::test_connection(&path).unwrap();
    connection.execute_batch("DROP TABLE peer_session_settings; DROP TABLE deliveries; DROP TABLE trusted_peer; DROP TABLE peer_keys; DROP TABLE transport_identity; PRAGMA user_version=3;").unwrap();
    drop(connection);
    let repository = SqliteMessageRepository::open(&path).unwrap();
    assert_eq!(repository.local_identity().peer_id(), own);
    assert_eq!(
        repository.find_by_id(message.id()).unwrap(),
        Some(message.clone())
    );
    assert_eq!(
        repository.pending(peer, 99).unwrap().unwrap().message,
        message
    );
    drop(repository);
    assert_eq!(
        SqliteMessageRepository::open(&path)
            .unwrap()
            .local_identity()
            .peer_id(),
        own
    );
}

#[test]
fn failed_version_four_upgrade_rolls_back_and_damaged_version_four_is_rejected() {
    let (dir, repository) = repository();
    let path = dir.path().join("messages.sqlite");
    drop(repository);
    let c = crate::encrypted_storage::test_connection(&path).unwrap();
    c.execute_batch("DROP TABLE peer_session_settings; DROP TABLE deliveries; DROP TABLE trusted_peer; DROP TABLE peer_keys; DROP TABLE transport_identity; PRAGMA user_version=3; CREATE TABLE deliveries(broken TEXT);").unwrap();
    drop(c);
    assert!(SqliteMessageRepository::open(&path).is_err());
    let c = crate::encrypted_storage::test_connection(&path).unwrap();
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='transport_identity'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    c.execute_batch("DROP TABLE deliveries").unwrap();
    drop(c);
    drop(SqliteMessageRepository::open(&path).unwrap());
    let c = crate::encrypted_storage::test_connection(&path).unwrap();
    c.execute_batch("DROP TABLE deliveries; CREATE TABLE deliveries(broken TEXT);")
        .unwrap();
    drop(c);
    assert!(SqliteMessageRepository::open(path).is_err());
}
struct Time(i64);
impl Clock for Time {
    fn now_millis(&self) -> std::result::Result<i64, crate::message::MessageError> {
        Ok(self.0)
    }
}
#[test]
fn delivery_is_atomic_persistent_scoped_idempotent_and_expires_exactly_at_boundary() {
    let (dir, repo) = repository();
    let own = repo.local_identity().peer_id();
    let peer = PeerId::generate();
    let mut service = MessageService::with_clock(repo, Time(100), own);
    let msg = service.create_outgoing(peer, "once").unwrap();
    assert_eq!(service.next_delivery(peer).unwrap().unwrap().message, msg);
    assert!(service.begin_attempt(msg.id()).unwrap());
    assert_eq!(service.next_delivery(peer).unwrap().unwrap().attempts, 1);
    assert!(
        !service
            .confirm_delivery(msg.id(), PeerId::generate())
            .unwrap()
    );
    assert!(!service.confirm_delivery("unknown", peer).unwrap());
    drop(service);
    let repo = SqliteMessageRepository::open(dir.path().join("messages.sqlite")).unwrap();
    assert_eq!(repo.pending(peer, 99).unwrap().unwrap().message, msg);
    let mut service = MessageService::with_clock(repo, Time(100 + RETENTION_MILLIS - 1), own);
    assert!(service.confirm_delivery(msg.id(), peer).unwrap());
    assert!(!service.confirm_delivery(msg.id(), peer).unwrap());
    assert!(service.next_delivery(peer).unwrap().is_none());
    assert!(
        service.peer_history(peer).unwrap()[0]
            .acknowledged_at
            .is_some()
    );
    service.cleanup_expired().unwrap();
    let other = service.create_outgoing(peer, "boundary").unwrap();
    // Repository cutoff excludes a message at the exact boundary, including delayed ACKs.
    assert!(
        !service
            .repository
            .acknowledge(
                other.id(),
                peer,
                other.created_at(),
                other.created_at() + RETENTION_MILLIS
            )
            .unwrap()
    );
    assert!(
        !service
            .repository
            .record_attempt(other.id(), other.created_at())
            .unwrap()
    );
    service
        .repository
        .delete_through(other.created_at())
        .unwrap();
    assert_eq!(
        service
            .repository
            .connection
            .query_row("SELECT COUNT(*) FROM deliveries", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn delivery_insert_and_ack_failures_preserve_atomic_state() {
    let (_dir, repo) = repository();
    let own = repo.local_identity().peer_id();
    let peer = PeerId::generate();
    let mut s = MessageService::with_clock(repo, Time(100), own);
    s.repository.connection.execute_batch("CREATE TRIGGER fail_delivery BEFORE INSERT ON deliveries BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    assert!(s.create_outgoing(peer, "not half saved").is_err());
    assert!(s.list_messages().unwrap().is_empty());
    s.repository
        .connection
        .execute_batch("DROP TRIGGER fail_delivery")
        .unwrap();
    let msg = s.create_outgoing(peer, "pending").unwrap();
    s.repository.connection.execute_batch("CREATE TRIGGER fail_ack BEFORE UPDATE ON deliveries BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    assert!(s.confirm_delivery(msg.id(), peer).is_err());
    assert!(s.peer_history(peer).unwrap()[0].acknowledged_at.is_none());
}
#[test]
fn oversized_payload_is_rejected_before_message_and_delivery_insert() {
    let (_dir, repo) = repository();
    let own = repo.local_identity().peer_id();
    let mut s = MessageService::with_clock(repo, Time(100), own);
    let result =
        s.create_outgoing_validated(PeerId::generate(), &"x".repeat(protocol::MAX_JSON), |m| {
            protocol::Frame::Message {
                message: protocol::WireMessage::from(m),
            }
            .encode()
            .map(|_| ())
        });
    assert_eq!(result.unwrap_err().code, "message_too_large");
    assert!(s.list_messages().unwrap().is_empty());
    assert!(s.create_message(&"x".repeat(protocol::MAX_JSON)).is_ok());
}

#[tokio::test]
async fn noise_transport_authenticates_both_sides_and_moves_frames_bidirectionally() {
    let (_a, mut a) = repository();
    let (_b, mut b) = repository();
    let secrets = Secrets::default();
    let (aid, ak) = identity(&mut a, &secrets);
    let (bid, bk) = identity(&mut b, &secrets);
    let listener = TcpListener::bind(endpoint(0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let address = match address {
        std::net::SocketAddr::V4(a) => a,
        _ => unreachable!(),
    };
    let ai = Invitation::new(aid, &ak.public, endpoint(32147));
    let bi = Invitation::new(bid, &bk.public, address);
    let (client, server) = tokio::join!(
        async {
            transport::handshake(
                TcpStream::connect(address).await.unwrap(),
                &ak,
                aid,
                &bi,
                true,
            )
            .await
        },
        async {
            let (stream, _) = listener.accept().await.unwrap();
            transport::handshake(stream, &bk, bid, &ai, false).await
        }
    );
    let mut client = client.unwrap();
    let mut server = server.unwrap();
    client
        .send(&protocol::Frame::Ping { id: 42 })
        .await
        .unwrap();
    assert!(matches!(
        server.receive().await.unwrap(),
        protocol::Frame::Ping { id: 42 }
    ));
    server
        .send(&protocol::Frame::Pong { id: 42 })
        .await
        .unwrap();
    assert!(matches!(
        client.receive().await.unwrap(),
        protocol::Frame::Pong { id: 42 }
    ));
    client.close().await;
    assert!(matches!(
        server.receive().await.unwrap(),
        protocol::Frame::Close
    ));
}
#[tokio::test]
async fn wrong_key_and_wrong_peer_identity_never_establish_a_session() {
    for wrong_identity in [false, true] {
        let (_a, mut a) = repository();
        let (_b, mut b) = repository();
        let secrets = Secrets::default();
        let (aid, ak) = identity(&mut a, &secrets);
        let (bid, bk) = identity(&mut b, &secrets);
        let listener = TcpListener::bind(endpoint(0)).await.unwrap();
        let address = match listener.local_addr().unwrap() {
            std::net::SocketAddr::V4(a) => a,
            _ => unreachable!(),
        };
        let ai = Invitation::new(aid, &ak.public, endpoint(32147));
        let bi = Invitation::new(
            if wrong_identity {
                PeerId::generate()
            } else {
                bid
            },
            if wrong_identity { &bk.public } else { &[3; 32] },
            address,
        );
        let (client, _server) = tokio::join!(
            async {
                transport::handshake(
                    TcpStream::connect(address).await.unwrap(),
                    &ak,
                    aid,
                    &bi,
                    true,
                )
                .await
            },
            async {
                let (stream, _) = listener.accept().await.unwrap();
                transport::handshake(stream, &bk, bid, &ai, false).await
            }
        );
        assert_eq!(client.err().unwrap().code, "authentication_failed");
    }
}

#[derive(Default)]
struct EventLog {
    states: Mutex<Vec<PeerState>>,
    changes: Mutex<usize>,
}
impl Events for EventLog {
    fn state_changed(&self, s: PeerState) {
        self.states.lock().unwrap().push(s);
    }
    fn messages_changed(&self) {
        *self.changes.lock().unwrap() += 1;
    }
}
struct LoseFirstAck {
    db: commands::Database,
    lose: AtomicBool,
}
impl Persistence for LoseFirstAck {
    fn receive(&self, p: PeerId, m: protocol::WireMessage) -> Result<(String, bool)> {
        let result = self.db.receive(p, m)?;
        if self.lose.swap(false, Ordering::SeqCst) {
            Err(PeerError::new(
                "storage_error",
                "simulate commit succeeded but ACK lost",
            ))
        } else {
            Ok(result)
        }
    }
    fn next(&self, p: PeerId) -> Result<Option<delivery::PendingMessage>> {
        self.db.next(p)
    }
    fn attempt(&self, id: &str) -> Result<bool> {
        self.db.attempt(id)
    }
    fn acknowledge(&self, p: PeerId, id: &str) -> Result<bool> {
        self.db.acknowledge(p, id)
    }
}
async fn until(mut ready: impl FnMut() -> bool) {
    timeout(Duration::from_secs(18), async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn two_database_coordinators_retry_lost_ack_resume_after_disconnect_and_stop_cleanly() {
    let (_ad, mut a) = repository();
    let (_bd, mut b) = repository();
    let secrets = Secrets::default();
    let (aid, ak) = identity(&mut a, &secrets);
    let (bid, bk) = identity(&mut b, &secrets);
    let listener = TcpListener::bind(endpoint(0)).await.unwrap();
    let endpoint = match listener.local_addr().unwrap() {
        std::net::SocketAddr::V4(a) => a,
        _ => unreachable!(),
    };
    drop(listener);
    let ai = Invitation::new(aid, &ak.public, super::tests::endpoint(32147));
    let bi = Invitation::new(bid, &bk.public, endpoint);
    let astate = Arc::new(Mutex::new(MessageService::new(a, aid)));
    let bstate = Arc::new(Mutex::new(MessageService::new(b, bid)));
    let ae = Arc::new(EventLog::default());
    let be = Arc::new(EventLog::default());
    let ac = Coordinator::new(Arc::new(commands::Database(astate.clone())), ae.clone());
    let bc = Coordinator::new(
        Arc::new(LoseFirstAck {
            db: commands::Database(bstate.clone()),
            lose: AtomicBool::new(true),
        }),
        be.clone(),
    );
    bc.start(bk, bid, ai.clone(), Role::Responder, endpoint)
        .await
        .unwrap();
    ac.start(
        ak,
        aid,
        bi.clone(),
        Role::Initiator,
        super::tests::endpoint(32147),
    )
    .await
    .unwrap();
    until(|| ac.connected_to(bid) && bc.connected_to(aid)).await;
    let message = astate
        .lock()
        .unwrap()
        .create_outgoing(bid, "Hallo 🦀\nzweimal übertragen, einmal gespeichert")
        .unwrap();
    ac.wake();
    until(|| !bstate.lock().unwrap().list_messages().unwrap().is_empty()).await;
    let received = bstate.lock().unwrap().list_messages().unwrap()[0].clone();
    until(|| {
        astate.lock().unwrap().peer_history(bid).unwrap()[0]
            .acknowledged_at
            .is_some()
    })
    .await;
    assert_eq!(
        bstate.lock().unwrap().list_messages().unwrap(),
        vec![received.clone()]
    );
    assert_eq!(received.id(), message.id());
    assert_eq!(
        astate
            .lock()
            .unwrap()
            .repository
            .pending(bid, 0)
            .unwrap()
            .map(|m| m.attempts),
        None
    );
    let reverse = bstate
        .lock()
        .unwrap()
        .create_outgoing(aid, "Antwort")
        .unwrap();
    bc.wake();
    until(|| {
        bstate
            .lock()
            .unwrap()
            .peer_history(aid)
            .unwrap()
            .iter()
            .any(|m| m.message.id() == reverse.id() && m.acknowledged_at.is_some())
    })
    .await;
    bc.disconnect().await;
    until(|| ac.state().phase != "connected").await;
    let (_own, bk) = identity(&mut bstate.lock().unwrap().repository, &secrets);
    bc.start(bk, bid, ai, Role::Responder, endpoint)
        .await
        .unwrap();
    until(|| ac.connected_to(bid) && bc.connected_to(aid)).await;
    ac.disconnect().await;
    bc.disconnect().await;
    assert_eq!(ac.state().phase, "disconnected");
    assert!(TcpListener::bind(endpoint).await.is_ok());
    let states = ae.states.lock().unwrap();
    assert!(states.iter().any(|s| s.phase == "reconnecting"));
    assert!(states.windows(2).all(|w| w[0].revision < w[1].revision));
}

#[test]
fn session_settings_migrate_from_v4_and_persist_atomically_with_pins() {
    use super::types::{Role, SessionSettings};
    let (dir, mut repo) = repository();
    let path = dir.path().join("messages.sqlite");
    let own = repo.local_identity().peer_id();
    let peer = Invitation::new(PeerId::generate(), &[1; 32], endpoint(32147));
    repo.select_peer(&peer).unwrap();
    let message = MessageService::new(repo, own)
        .create_outgoing(peer.peer().unwrap(), "preserved")
        .unwrap();
    let c = crate::encrypted_storage::test_connection(&path).unwrap();
    c.execute_batch("DROP TABLE peer_session_settings; PRAGMA user_version=4;")
        .unwrap();
    drop(c);
    let mut repo = SqliteMessageRepository::open(&path).unwrap();
    assert_eq!(repo.local_identity().peer_id(), own);
    assert_eq!(repo.selected_peer().unwrap(), Some(peer.clone()));
    assert!(repo.saved_session(peer.peer().unwrap()).unwrap().is_none());
    assert_eq!(repo.find_by_id(message.id()).unwrap(), Some(message));
    let settings = SessionSettings {
        local_address: Ipv4Addr::LOCALHOST,
        local_port: 32148,
        role: Role::Responder,
    };
    repo.select_session(&peer, &settings).unwrap();
    drop(repo);
    let mut repo = SqliteMessageRepository::open(&path).unwrap();
    assert_eq!(
        repo.saved_session(peer.peer().unwrap()).unwrap(),
        Some(settings.clone())
    );
    assert!(repo.saved_session(PeerId::generate()).unwrap().is_none());
    let changed_key = Invitation::new(peer.peer().unwrap(), &[2; 32], endpoint(32149));
    assert_eq!(
        repo.select_session(&changed_key, &settings)
            .unwrap_err()
            .code,
        "peer_key_changed"
    );
    repo.connection.execute_batch("CREATE TRIGGER fail_settings BEFORE INSERT ON peer_session_settings BEGIN SELECT RAISE(ABORT, 'fail'); END;").unwrap();
    let other = Invitation::new(PeerId::generate(), &[3; 32], endpoint(32150));
    assert_eq!(
        repo.select_session(&other, &settings).unwrap_err().code,
        "storage_error"
    );
    assert_eq!(repo.selected_peer().unwrap(), Some(peer.clone()));
    assert_eq!(
        repo.saved_session(peer.peer().unwrap()).unwrap(),
        Some(settings)
    );
    let pins: i64 = repo
        .connection
        .query_row(
            "SELECT count(*) FROM peer_keys WHERE peer_id=?1",
            [other.peer_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pins, 0);
}

#[test]
fn failed_v5_migration_rolls_back_and_damaged_v5_is_rejected() {
    let (dir, repo) = repository();
    let path = dir.path().join("messages.sqlite");
    drop(repo);
    let c = crate::encrypted_storage::test_connection(&path).unwrap();
    c.execute_batch("DROP TABLE peer_session_settings; CREATE TABLE peer_session_settings(broken TEXT); PRAGMA user_version=4;").unwrap();
    assert!(SqliteMessageRepository::open(&path).is_err());
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    c.execute_batch("DROP TABLE peer_session_settings").unwrap();
    drop(SqliteMessageRepository::open(&path).unwrap());
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
    c.execute_batch(
        "DROP TABLE peer_session_settings; CREATE TABLE peer_session_settings(broken TEXT)",
    )
    .unwrap();
    assert!(SqliteMessageRepository::open(&path).is_err());
}
