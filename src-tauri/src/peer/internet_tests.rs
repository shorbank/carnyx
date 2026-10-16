use super::{
    coordinator::{Coordinator, Events},
    keys::{KeyRepository, Keys},
    network::NetworkConfig,
    types::{ConnectionMode, ConnectionPath, ConnectionTarget, Invitation, SessionSettings},
    *,
};
use crate::{credentials::Credentials, identity::PeerId, message_service::MessageRepository};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::time::{Duration, timeout};

#[derive(Default)]
struct Secrets {
    value: Mutex<Option<Vec<u8>>>,
    saves: AtomicUsize,
}
impl Credentials for Secrets {
    fn load(&self, service: &str, account: &str) -> std::result::Result<Option<Vec<u8>>, ()> {
        assert_eq!(service, "test.carnyx.internet-key.v1");
        assert!(account.parse::<PeerId>().is_ok());
        Ok(self.value.lock().unwrap().clone())
    }
    fn save(&self, _: &str, _: &str, secret: &[u8]) -> std::result::Result<(), ()> {
        *self.value.lock().unwrap() = Some(secret.to_vec());
        self.saves.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
#[test]
fn transport_keys_recover_orphaned_setup_but_never_replace_missing_invalid_or_changed_identity() {
    let store = Secrets::default();
    let own = PeerId::generate();
    let key = internet_keys::restore(&store, "test.carnyx", own, None).unwrap();
    let id = *key.public().as_bytes();
    assert_eq!(
        internet_keys::restore(&store, "test.carnyx", own, None)
            .unwrap()
            .public(),
        key.public()
    );
    assert_eq!(
        internet_keys::restore(&store, "test.carnyx", own, Some(id))
            .unwrap()
            .public(),
        key.public()
    );
    for raw in [None, Some(vec![9; 3]), Some(vec![8; 32])] {
        *store.value.lock().unwrap() = raw;
        assert_eq!(
            internet_keys::restore(&store, "test.carnyx", own, Some(id))
                .unwrap_err()
                .code,
            "key_unavailable"
        );
    }
    assert_eq!(store.saves.load(Ordering::SeqCst), 1);
}
#[test]
fn v2_codes_are_bounded_validate_relay_and_preserve_independent_noise_identity() {
    let own = PeerId::generate();
    let transport = iroh::SecretKey::generate();
    let internet = Invitation::internet(own, &[1; 32], transport.public());
    assert!(internet.encode().starts_with("carnyx:v2:"));
    assert_eq!(Invitation::decode(&internet.encode()).unwrap(), internet);
    assert!(SessionSettings::Internet.validate_for(&internet).is_ok());
    let lan = Invitation::new(own, &[1; 32], "127.0.0.1:32147".parse().unwrap());
    assert!(lan.encode().starts_with("carnyx:v1:"));
    assert_eq!(Invitation::decode(&lan.encode()).unwrap(), lan);
    assert_eq!(
        SessionSettings::Internet
            .validate_for(&lan)
            .unwrap_err()
            .code,
        "mode_mismatch"
    );
    let mut bad = internet.clone();
    if let ConnectionTarget::Internet { relay_url, .. } = &mut bad.target {
        *relay_url = "https://example.com/".into();
    }
    assert_eq!(
        Invitation::decode(&bad.encode()).unwrap_err().code,
        "unsupported_relay"
    );
    if let ConnectionTarget::Internet { relay_url, .. } = &mut bad.target {
        *relay_url = "http://127.0.0.1/".into();
    }
    assert_eq!(
        Invitation::decode(&bad.encode()).unwrap_err().code,
        "invalid_code"
    );
    assert!(Invitation::decode(&"x".repeat(2049)).is_err());
    let other = PeerId::generate();
    let inverse = Invitation::internet(other, &[2; 32], iroh::SecretKey::generate().public());
    assert_ne!(
        SessionSettings::Internet.role(own, &inverse),
        SessionSettings::Internet.role(other, &internet)
    );
}
#[test]
fn profiles_and_transport_identity_survive_reopen_and_failed_writes_keep_pins_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("messages.sqlite");
    let mut repo = crate::sqlite_repository::SqliteMessageRepository::open(&path).unwrap();
    let peer = Invitation::internet(
        PeerId::generate(),
        &[3; 32],
        iroh::SecretKey::generate().public(),
    );
    let identity = *iroh::SecretKey::generate().public().as_bytes();
    repo.save_internet_public_key(&identity).unwrap();
    repo.select_session(&peer, &SessionSettings::Internet)
        .unwrap();
    drop(repo);
    let mut repo = crate::sqlite_repository::SqliteMessageRepository::open(&path).unwrap();
    assert_eq!(repo.selected_peer().unwrap(), Some(peer.clone()));
    assert_eq!(
        repo.saved_session(peer.peer().unwrap()).unwrap(),
        Some(SessionSettings::Internet)
    );
    assert_eq!(repo.internet_public_key().unwrap(), Some(identity));
    repo.connection.execute_batch("CREATE TRIGGER fail_profile BEFORE INSERT ON connection_profile BEGIN SELECT RAISE(ABORT,'fail'); END").unwrap();
    let other = Invitation::internet(
        PeerId::generate(),
        &[4; 32],
        iroh::SecretKey::generate().public(),
    );
    assert_eq!(
        repo.select_session(&other, &SessionSettings::Internet)
            .unwrap_err()
            .code,
        "storage_error"
    );
    assert_eq!(repo.selected_peer().unwrap(), Some(peer));
    assert_eq!(
        repo.connection
            .query_row(
                "SELECT count(*) FROM peer_keys WHERE peer_id=?1",
                [other.peer_id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}
#[test]
fn v6_migration_rolls_back_and_rejects_corrupt_profiles_without_reset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("messages.sqlite");
    let mut c = crate::encrypted_storage::test_connection(&path).unwrap();
    crate::migrations::legacy_v5(&mut c).unwrap();
    c.execute_batch("CREATE TABLE internet_identity(broken TEXT)")
        .unwrap();
    assert!(crate::sqlite_repository::SqliteMessageRepository::open(&path).is_err());
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='trusted_peer'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    c.execute_batch("DROP TABLE internet_identity").unwrap();
    drop(c);
    let repo = crate::sqlite_repository::SqliteMessageRepository::open(&path).unwrap();
    repo.connection
        .execute(
            "INSERT INTO connection_profile VALUES(1,?1)",
            [r#"{"version":99}"#],
        )
        .unwrap();
    drop(repo);
    assert!(crate::sqlite_repository::SqliteMessageRepository::open(&path).is_err());
}
#[test]
fn v6_migration_rejects_inconsistent_legacy_session_without_losing_the_original_profile() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("messages.sqlite");
    let mut c = crate::encrypted_storage::test_connection(&path).unwrap();
    crate::migrations::legacy_v5(&mut c).unwrap();
    let peer = PeerId::generate().to_string();
    c.execute(
        "INSERT INTO trusted_peer VALUES(1,?1,?2,'127.0.0.1:32147')",
        rusqlite::params![peer, [3u8; 32].as_slice()],
    )
    .unwrap();
    c.execute(
        "INSERT INTO peer_session_settings VALUES(1,?1,'127.0.0.1',32147,'initiator')",
        [PeerId::generate().to_string()],
    )
    .unwrap();
    assert!(crate::sqlite_repository::SqliteMessageRepository::open(&path).is_err());
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        c.query_row("SELECT peer_id FROM trusted_peer", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        peer
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='connection_profile'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
#[derive(Default)]
struct Changes(Mutex<Vec<types::PeerState>>);
impl Events for Changes {
    fn state_changed(&self, s: types::PeerState) {
        self.0.lock().unwrap().push(s);
    }
    fn messages_changed(&self) {}
}
fn keys() -> Keys {
    let raw = snow::Builder::new(protocol::NOISE.parse().unwrap())
        .generate_keypair()
        .unwrap();
    Keys {
        private: zeroize::Zeroizing::new(raw.private),
        public: raw.public.try_into().unwrap(),
    }
}
fn database() -> (tempfile::TempDir, crate::chat_state::ChatState, PeerId) {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::chat_state::initialize(dir.path().join("messages.sqlite")).unwrap();
    let own = state.lock().unwrap().local_peer_id();
    (dir, state, own)
}
async fn relay() -> iroh_relay::server::Server {
    let mut config = iroh_relay::server::ServerConfig::default();
    config.relay = Some(iroh_relay::server::RelayConfig::new(
        "127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap(),
    ));
    iroh_relay::server::Server::spawn(config).await.unwrap()
}
async fn until(mut condition: impl FnMut() -> bool) {
    timeout(Duration::from_secs(25), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("condition became true");
}
#[tokio::test]
async fn local_relay_coordinators_authenticate_deliver_reconnect_and_release_endpoints() {
    let relay = relay().await;
    let options = NetworkConfig::local(relay.http_url().unwrap(), false);
    let (_ad, as_, aid) = database();
    let (_bd, bs, bid) = database();
    let ak = keys();
    let bk = keys();
    let at = iroh::SecretKey::generate();
    let bt = iroh::SecretKey::generate();
    let ai = Invitation::internet(aid, &ak.public, at.public());
    let bi = Invitation::internet(bid, &bk.public, bt.public());
    let ae = Arc::new(Changes::default());
    let be = Arc::new(Changes::default());
    let a = Coordinator::new(Arc::new(commands::Database(as_.clone())), ae.clone())
        .with_network(options.clone());
    let b = Coordinator::new(Arc::new(commands::Database(bs.clone())), be.clone())
        .with_network(options);
    // One side may start before the other. Only the elected initiator ever dials.
    a.start_session(
        ak.clone(),
        aid,
        bi.clone(),
        SessionSettings::Internet,
        Some(at.clone()),
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    b.start_session(
        bk.clone(),
        bid,
        ai.clone(),
        SessionSettings::Internet,
        Some(bt.clone()),
    )
    .await
    .unwrap();
    until(|| {
        a.connected_to(bid)
            && b.connected_to(aid)
            && a.state().path == Some(ConnectionPath::Relay)
            && b.state().path == Some(ConnectionPath::Relay)
    })
    .await;
    assert_eq!(a.state().mode, Some(ConnectionMode::Internet));
    let message = as_
        .lock()
        .unwrap()
        .create_outgoing(bid, "internet relay proof")
        .unwrap();
    a.wake();
    until(|| {
        bs.lock()
            .unwrap()
            .repository
            .find_by_id(message.id())
            .unwrap()
            .is_some()
    })
    .await;
    until(|| {
        as_.lock().unwrap().peer_history(bid).is_ok_and(|rows| {
            rows.iter()
                .any(|row| row.message.id() == message.id() && row.acknowledged_at.is_some())
        })
    })
    .await;
    a.disconnect().await;
    b.disconnect().await;
    let states = ae.0.lock().unwrap().len();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(ae.0.lock().unwrap().len(), states);
    assert_eq!(a.state().phase, "disconnected");
    assert_eq!(a.state().path, None);
    a.start_session(ak, aid, bi, SessionSettings::Internet, Some(at))
        .await
        .unwrap();
    b.start_session(bk, bid, ai, SessionSettings::Internet, Some(bt))
        .await
        .unwrap();
    until(|| a.connected_to(bid) && b.connected_to(aid)).await;
    let response = bs
        .lock()
        .unwrap()
        .create_outgoing(aid, "after reconnect")
        .unwrap();
    b.wake();
    until(|| {
        as_.lock()
            .unwrap()
            .repository
            .find_by_id(response.id())
            .unwrap()
            .is_some()
    })
    .await;
    let previous_a = a.state().revision;
    let previous_b = b.state().revision;
    // A lost relay must leave the authenticated state, then remain explicitly cancelable.
    relay.shutdown().await.unwrap();
    timeout(Duration::from_secs(40), async {
        loop {
            let failed_a =
                ae.0.lock()
                    .unwrap()
                    .iter()
                    .any(|s| s.revision > previous_a && s.phase == "reconnecting");
            let failed_b =
                be.0.lock()
                    .unwrap()
                    .iter()
                    .any(|s| s.revision > previous_b && s.phase == "reconnecting");
            if failed_a && failed_b {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(!a.connected_to(bid));
    assert!(!b.connected_to(aid));
    a.disconnect().await;
    b.disconnect().await;
    let revision = a.state().revision;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(a.state().revision, revision);
}
#[tokio::test]
async fn disconnect_cancels_an_internet_waiter_and_unknown_endpoint_is_rejected() {
    let relay = relay().await;
    let options = NetworkConfig::local(relay.http_url().unwrap(), false);
    let (_dir, state, own) = database();
    let own_key = keys();
    let transport = iroh::SecretKey::generate();
    // Select a smaller UUID so this side is the responder.
    let peer: PeerId = "00000000-0000-4000-8000-000000000001".parse().unwrap();
    let expected = iroh::SecretKey::generate();
    let invitation = Invitation::internet(peer, &[7; 32], expected.public());
    let coordinator = Coordinator::new(
        Arc::new(commands::Database(state)),
        Arc::new(Changes::default()),
    )
    .with_network(options);
    coordinator
        .start_session(
            own_key,
            own,
            invitation,
            SessionSettings::Internet,
            Some(transport.clone()),
        )
        .await
        .unwrap();
    until(|| coordinator.state().phase == "listening").await;
    let stranger = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .clear_ip_transports()
        .relay_mode(iroh::RelayMode::Custom(
            [iroh::RelayConfig::from(relay.http_url().unwrap())]
                .into_iter()
                .collect(),
        ))
        .bind()
        .await
        .unwrap();
    let addr =
        iroh::EndpointAddr::new(transport.public()).with_relay_url(relay.http_url().unwrap());
    let connection = timeout(
        Duration::from_secs(20),
        stranger.connect(addr, network::ALPN),
    )
    .await
    .unwrap()
    .unwrap();
    timeout(Duration::from_secs(5), connection.closed())
        .await
        .unwrap();
    assert!(!coordinator.connected_to(peer));
    stranger.close().await;
    // Even the selected transport peer cannot keep a canceled Noise handshake alive.
    let selected = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .secret_key(expected)
        .clear_ip_transports()
        .relay_mode(iroh::RelayMode::Custom(
            [iroh::RelayConfig::from(relay.http_url().unwrap())]
                .into_iter()
                .collect(),
        ))
        .bind()
        .await
        .unwrap();
    let addr =
        iroh::EndpointAddr::new(transport.public()).with_relay_url(relay.http_url().unwrap());
    let connection = timeout(
        Duration::from_secs(20),
        selected.connect(addr, network::ALPN),
    )
    .await
    .unwrap()
    .unwrap();
    let (mut send, _recv) = connection.open_bi().await.unwrap();
    // Announce the stream without completing the application handshake.
    tokio::io::AsyncWriteExt::write_all(&mut send, &[0])
        .await
        .unwrap();
    until(|| coordinator.state().phase == "handshaking").await;
    assert!(
        timeout(Duration::from_millis(100), connection.open_bi())
            .await
            .is_err(),
        "only one application stream is allowed"
    );
    timeout(Duration::from_secs(3), coordinator.disconnect())
        .await
        .unwrap();
    assert_eq!(coordinator.state().phase, "disconnected");
    timeout(Duration::from_secs(3), connection.closed())
        .await
        .unwrap();
    selected.close().await;
    relay.shutdown().await.unwrap();
}

#[tokio::test]
async fn disconnect_cancels_an_outbound_attempt_before_the_connect_timeout() {
    let relay = relay().await;
    let (_dir, state, own) = database();
    let events = Arc::new(Changes::default());
    let coordinator = Coordinator::new(Arc::new(commands::Database(state)), events.clone())
        .with_network(NetworkConfig::local(relay.http_url().unwrap(), false));
    let peer = Invitation::internet(
        "ffffffff-ffff-4fff-bfff-ffffffffffff".parse().unwrap(),
        &[7; 32],
        iroh::SecretKey::generate().public(),
    );
    coordinator
        .start_session(
            keys(),
            own,
            peer,
            SessionSettings::Internet,
            Some(iroh::SecretKey::generate()),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(coordinator.state().phase, "connecting");
    timeout(Duration::from_secs(3), coordinator.disconnect())
        .await
        .unwrap();
    let revision = coordinator.state().revision;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(coordinator.state().phase, "disconnected");
    assert_eq!(coordinator.state().revision, revision);
    assert_eq!(events.0.lock().unwrap().last().unwrap().revision, revision);
    relay.shutdown().await.unwrap();
}

#[tokio::test]
async fn authenticated_internet_endpoints_reject_wrong_noise_pins() {
    let relay = relay().await;
    let options = NetworkConfig::local(relay.http_url().unwrap(), false);
    let (_ad, as_, aid) = database();
    let (_bd, bs, bid) = database();
    let ak = keys();
    let bk = keys();
    let at = iroh::SecretKey::generate();
    let bt = iroh::SecretKey::generate();
    let ai = Invitation::internet(aid, &[9; 32], at.public());
    let bi = Invitation::internet(bid, &[8; 32], bt.public());
    let a = Coordinator::new(
        Arc::new(commands::Database(as_)),
        Arc::new(Changes::default()),
    )
    .with_network(options.clone());
    let b = Coordinator::new(
        Arc::new(commands::Database(bs)),
        Arc::new(Changes::default()),
    )
    .with_network(options);
    a.start_session(ak, aid, bi, SessionSettings::Internet, Some(at))
        .await
        .unwrap();
    b.start_session(bk, bid, ai, SessionSettings::Internet, Some(bt))
        .await
        .unwrap();
    until(|| {
        a.state().error == Some("authentication_failed")
            || b.state().error == Some("authentication_failed")
    })
    .await;
    assert!(!a.connected_to(bid));
    assert!(!b.connected_to(aid));
    a.disconnect().await;
    b.disconnect().await;
    relay.shutdown().await.unwrap();
}

#[tokio::test]
async fn direct_iroh_stream_keeps_noise_framing_and_reports_selected_direct_path() {
    use futures_util::StreamExt;
    use iroh::{Endpoint, EndpointAddr, RelayMode, endpoint::presets};
    let a = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .alpns(vec![network::ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let b = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .alpns(vec![network::ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let port = b
        .bound_sockets()
        .into_iter()
        .find(|a| a.is_ipv4())
        .unwrap()
        .port();
    let address = EndpointAddr::new(b.id()).with_ip_addr(std::net::SocketAddr::from((
        std::net::Ipv4Addr::LOCALHOST,
        port,
    )));
    let ak = keys();
    let bk = keys();
    let aid = PeerId::generate();
    let bid = PeerId::generate();
    let ai = Invitation::internet(aid, &ak.public, a.id());
    let bi = Invitation::internet(bid, &bk.public, b.id());
    timeout(Duration::from_secs(20), async {
        let (ca, cb) = tokio::join!(
            async { a.connect(address, network::ALPN).await.unwrap() },
            async { b.accept().await.unwrap().await.unwrap() }
        );
        let (send, recv) = ca.open_bi().await.unwrap();
        let identity_a = ai.identity().unwrap();
        let identity_b = bi.identity().unwrap();
        let (sa, sb) = tokio::join!(
            transport::handshake(tokio::io::join(recv, send), &ak, aid, &identity_b, true),
            async {
                let (send, recv) = cb.accept_bi().await.unwrap();
                transport::handshake(tokio::io::join(recv, send), &bk, bid, &identity_a, false)
                    .await
            }
        );
        let (mut sa, mut sb) = (sa.unwrap(), sb.unwrap());
        sa.send(&protocol::Frame::Ping { id: 42 }).await.unwrap();
        assert!(matches!(
            sb.receive().await.unwrap(),
            protocol::Frame::Ping { id: 42 }
        ));
        sb.send(&protocol::Frame::Pong { id: 42 }).await.unwrap();
        assert!(matches!(
            sa.receive().await.unwrap(),
            protocol::Frame::Pong { id: 42 }
        ));
        let mut snapshots = ca.paths_stream();
        let snapshot = snapshots.next().await.unwrap();
        assert!(
            snapshot
                .iter()
                .any(|p| p.is_selected() && p.remote_addr().is_ip())
        );
        ca.close(0u8.into(), b"done");
        cb.close(0u8.into(), b"done");
    })
    .await
    .unwrap();
    a.close().await;
    b.close().await;
}
