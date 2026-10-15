use super::migration::{MigrationPoint, Paths, open_observed};
use super::*;
use std::fs;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Default)]
struct Secrets {
    secret: Mutex<Option<Vec<u8>>>,
    saves: AtomicUsize,
}
impl Credentials for Secrets {
    fn load(&self, service: &str, account: &str) -> std::result::Result<Option<Vec<u8>>, ()> {
        assert_eq!(service, "test.carnyx.history-key.v1");
        assert_eq!(account, "messages.sqlite");
        Ok(self.secret.lock().unwrap().clone())
    }
    fn save(&self, _: &str, _: &str, secret: &[u8]) -> std::result::Result<(), ()> {
        self.saves.fetch_add(1, Ordering::SeqCst);
        *self.secret.lock().unwrap() = Some(secret.to_vec());
        Ok(())
    }
}
const CANARY: &str = "Carnyx_PRIVATE_canary_78a612f9_keine_Klartext_Historie";
fn open_test(path: &Path, secrets: &Secrets) -> Result<Connection> {
    open(path, secrets, "test.carnyx")
}
fn failure<T>(result: Result<T>) -> StorageFailure {
    result.err().expect("must fail closed")
}
fn legacy(path: &Path, version: i64) {
    let mut c = Connection::open(path).unwrap();
    if version == 0 {
        c.execute_batch("VACUUM;").unwrap();
        return;
    }
    if version <= 2 {
        c.execute_batch("CREATE TABLE messages(id TEXT PRIMARY KEY NOT NULL,text TEXT NOT NULL,created_at INTEGER NOT NULL);").unwrap();
        c.execute(
            "INSERT INTO messages(rowid,id,text,created_at) VALUES(9,'a',?1,100),(43,'b',?1,100)",
            [CANARY],
        )
        .unwrap();
        if version == 2 {
            c.execute_batch("CREATE INDEX idx_messages_created_at ON messages(created_at)")
                .unwrap();
        }
    } else {
        let identity = crate::migrations::migrate(&mut c).unwrap();
        let peer = "61f3f556-b896-4b43-bce0-507d166543e8";
        c.execute("INSERT INTO messages(rowid,id,text,created_at,sender_id,recipient_id,direction,received_at) VALUES(9,'a',?1,100,?2,NULL,'local',NULL),(43,'b',?1,100,?2,NULL,'local',NULL),(77,'out',?1,101,?2,?3,'outgoing',NULL)", rusqlite::params![CANARY,identity.peer_id().to_string(),peer]).unwrap();
        c.execute_batch("INSERT INTO transport_identity VALUES(1,zeroblob(32)); INSERT INTO deliveries VALUES('out','acknowledged',3,102);").unwrap();
        c.execute(
            "INSERT INTO trusted_peer VALUES(1,?1,zeroblob(32),'127.0.0.1:4250')",
            [peer],
        )
        .unwrap();
        c.execute("INSERT INTO peer_keys VALUES(?1,zeroblob(32))", [peer])
            .unwrap();
        c.execute(
            "INSERT INTO peer_session_settings VALUES(1,?1,'127.0.0.1',4250,'responder')",
            [peer],
        )
        .unwrap();
        if version <= 4 {
            c.execute_batch("DROP TABLE peer_session_settings").unwrap();
        }
        if version == 3 {
            c.execute_batch("DROP TABLE deliveries; DROP TABLE trusted_peer; DROP TABLE peer_keys; DROP TABLE transport_identity").unwrap();
        }
    }
    c.pragma_update(None, "user_version", version).unwrap();
}
fn assert_ciphertext(directory: &Path) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = fs::read(&path).unwrap();
            assert!(
                !bytes
                    .windows(CANARY.len())
                    .any(|chunk| chunk == CANARY.as_bytes()),
                "plaintext in {}",
                path.display()
            );
        }
    }
}

#[test]
fn creates_encrypted_history_reopens_and_rejects_unkeyed_sqlite() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("messages.sqlite");
    let secrets = Secrets::default();
    let c = open_test(&path, &secrets).unwrap();
    c.execute_batch("CREATE TABLE proof(text TEXT);").unwrap();
    c.execute("INSERT INTO proof VALUES(?1)", [CANARY]).unwrap();
    drop(c);
    assert!(!plaintext(&path).unwrap());
    assert_ciphertext(directory.path());
    assert_eq!(
        open_test(&path, &secrets)
            .unwrap()
            .query_row("SELECT text FROM proof", [], |r| r.get::<_, String>(0))
            .unwrap(),
        CANARY
    );
    assert_eq!(secrets.saves.load(Ordering::SeqCst), 1);
    assert!(
        Connection::open(&path)
            .unwrap()
            .query_row("SELECT count(*) FROM sqlite_master", [], |r| r
                .get::<_, i64>(0))
            .is_err()
    );
}

#[test]
fn missing_wrong_and_malformed_keys_never_replace_or_modify_ciphertext() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("messages.sqlite");
    let secrets = Secrets::default();
    let mut c = open_test(&path, &secrets).unwrap();
    crate::migrations::migrate(&mut c).unwrap();
    drop(c);
    let original = fs::read(&path).unwrap();
    for (secret, expected) in [
        (None, StorageFailure::KeyMissing),
        (Some(vec![7; 32]), StorageFailure::UnlockFailed),
        (Some(vec![7; 3]), StorageFailure::KeyUnavailable),
    ] {
        *secrets.secret.lock().unwrap() = secret;
        assert_eq!(failure(open_test(&path, &secrets)), expected);
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(secrets.saves.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn credential_failure_or_failed_readback_creates_no_database() {
    struct Unavailable(bool);
    impl Credentials for Unavailable {
        fn load(&self, _: &str, _: &str) -> std::result::Result<Option<Vec<u8>>, ()> {
            if self.0 { Err(()) } else { Ok(None) }
        }
        fn save(&self, _: &str, _: &str, _: &[u8]) -> std::result::Result<(), ()> {
            Ok(())
        }
    }
    for store in [Unavailable(true), Unavailable(false)] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        assert_eq!(
            failure(open(&path, &store, "test.carnyx")),
            StorageFailure::KeyUnavailable
        );
        assert!(!path.exists());
    }
}

#[test]
fn migrates_all_supported_versions_preserving_rows_order_identity_pins_and_deliveries() {
    for version in 0..=5 {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let secrets = Secrets::default();
        legacy(&path, version);
        let original = migration::source(&path).unwrap();
        let identity = if version >= 3 {
            Some(crate::sqlite_identity::load(&original).unwrap())
        } else {
            None
        };
        drop(original);
        let c = open_test(&path, &secrets).unwrap();
        assert_eq!(
            c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            version
        );
        let repository =
            crate::sqlite_repository::SqliteMessageRepository::from_connection(c).unwrap();
        if let Some(identity) = identity {
            assert_eq!(repository.local_identity().peer_id(), identity.peer_id());
        }
        if version > 0 {
            let rowids = repository
                .connection
                .prepare(
                    "SELECT rowid FROM messages ORDER BY COALESCE(received_at,created_at),rowid",
                )
                .unwrap()
                .query_map([], |r| r.get::<_, i64>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(&rowids[..2], &[9, 43]);
        }
        if version >= 4 {
            assert_eq!(repository.connection.query_row("SELECT state,attempts,acknowledged_at FROM deliveries WHERE message_id='out'",[],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?))).unwrap(),("acknowledged".into(),3,102));
            assert_eq!(
                repository
                    .connection
                    .query_row("SELECT address FROM trusted_peer", [], |r| r
                        .get::<_, String>(0))
                    .unwrap(),
                "127.0.0.1:4250"
            );
        }
        if version == 5 {
            assert_eq!(
                repository
                    .connection
                    .query_row("SELECT role FROM peer_session_settings", [], |r| r
                        .get::<_, String>(0))
                    .unwrap(),
                "responder"
            );
        }
        drop(repository);
        assert_ciphertext(directory.path());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

#[test]
fn each_interrupted_conversion_phase_resumes_without_losing_original_rows() {
    for point in [
        MigrationPoint::KeyPersisted,
        MigrationPoint::MarkerCreated,
        MigrationPoint::CopyVerified,
        MigrationPoint::SourceRetired,
        MigrationPoint::TargetPromoted,
        MigrationPoint::BeforeCleanup,
        MigrationPoint::SourceRemoved,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let secrets = Secrets::default();
        legacy(&path, 5);
        assert_eq!(
            failure(open_observed(&path, &secrets, "test.carnyx", |p| {
                if p == point {
                    Err(StorageFailure::MigrationFailed)
                } else {
                    Ok(())
                }
            })),
            StorageFailure::MigrationFailed
        );
        let c = open_test(&path, &secrets).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM messages", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        drop(c);
        assert_ciphertext(directory.path());
        assert_eq!(secrets.saves.load(Ordering::SeqCst), 1);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

#[test]
fn migration_artifacts_without_key_never_generate_a_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("messages.sqlite");
    let secrets = Secrets::default();
    legacy(&path, 5);
    assert!(
        open_observed(&path, &secrets, "test.carnyx", |p| {
            if p == MigrationPoint::CopyVerified {
                Err(StorageFailure::MigrationFailed)
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    *secrets.secret.lock().unwrap() = None;
    assert_eq!(
        failure(open_test(&path, &secrets)),
        StorageFailure::KeyMissing
    );
    assert!(plaintext(&path).unwrap());
    assert_eq!(secrets.saves.load(Ordering::SeqCst), 1);
}

#[test]
fn invalid_legacy_schema_is_not_retired_and_unknown_artifacts_are_not_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("messages.sqlite");
    let secrets = Secrets::default();
    legacy(&path, 5);
    Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    assert_eq!(
        failure(open_test(&path, &secrets)),
        StorageFailure::MigrationFailed
    );
    assert!(plaintext(&path).unwrap());
    assert!(!Paths::new(&path).retired.exists());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("messages.sqlite");
    let secrets = Secrets::default();
    legacy(&path, 5);
    *secrets.secret.lock().unwrap() = Some(vec![8; 32]);
    let stage = Paths::new(&path).stage;
    fs::write(&stage, b"unknown artifact").unwrap();
    assert_eq!(
        failure(open_test(&path, &secrets)),
        StorageFailure::MigrationFailed
    );
    assert_eq!(fs::read(stage).unwrap(), b"unknown artifact");
}

#[test]
fn failed_copy_keeps_source_and_cleanup_failure_blocks_until_retry_succeeds() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("messages.sqlite");
    let secrets = Secrets::default();
    legacy(&path, 5);
    let paths = Paths::new(&path);
    assert_eq!(
        failure(open_observed(&path, &secrets, "test.carnyx", |p| {
            if p == MigrationPoint::MarkerCreated {
                fs::create_dir(&paths.stage).unwrap();
            }
            Ok(())
        })),
        StorageFailure::MigrationFailed
    );
    assert!(plaintext(&path).unwrap());
    fs::remove_dir(&paths.stage).unwrap();
    let obstruction = migration::sidecars(&paths.retired).nth(1).unwrap();
    assert_eq!(
        failure(open_observed(&path, &secrets, "test.carnyx", |p| {
            if p == MigrationPoint::TargetPromoted {
                fs::create_dir(&obstruction).unwrap();
            }
            Ok(())
        })),
        StorageFailure::CleanupFailed
    );
    assert!(paths.retired.exists());
    assert!(paths.marker.exists());
    assert_eq!(
        failure(open_test(&path, &secrets)),
        StorageFailure::CleanupFailed
    );
    fs::remove_dir(obstruction).unwrap();
    drop(open_test(&path, &secrets).unwrap());
    assert_ciphertext(directory.path());
}

#[test]
fn corrupted_staged_copy_after_retirement_does_not_destroy_plaintext_source() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("messages.sqlite");
    let secrets = Secrets::default();
    legacy(&path, 5);
    assert!(
        open_observed(&path, &secrets, "test.carnyx", |p| {
            if p == MigrationPoint::SourceRetired {
                Err(StorageFailure::MigrationFailed)
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    let paths = Paths::new(&path);
    let original = fs::read(&paths.retired).unwrap();
    fs::write(&paths.stage, b"damaged encrypted copy").unwrap();
    assert_eq!(
        failure(open_test(&path, &secrets)),
        StorageFailure::UnlockFailed
    );
    assert_eq!(fs::read(&paths.retired).unwrap(), original);
    assert!(!path.exists());
}

#[test]
fn wal_journal_and_temp_storage_do_not_expose_message_text() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("messages.sqlite");
    let secrets = Secrets::default();
    let c = open_test(&path, &secrets).unwrap();
    assert_eq!(
        c.query_row("PRAGMA temp_store", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    c.execute_batch("PRAGMA journal_mode = WAL; CREATE TABLE proof(text TEXT);")
        .unwrap();
    c.execute("INSERT INTO proof VALUES(?1)", [CANARY]).unwrap();
    assert!(migration::sidecars(&path).next().unwrap().exists());
    assert_ciphertext(directory.path());
    c.execute_batch(
        "PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode = PERSIST; BEGIN IMMEDIATE;",
    )
    .unwrap();
    c.execute("UPDATE proof SET text=?1", [format!("{CANARY}_changed")])
        .unwrap();
    assert!(migration::sidecars(&path).last().unwrap().exists());
    assert_ciphertext(directory.path());
    c.execute_batch("ROLLBACK").unwrap();
    assert_ciphertext(directory.path());
}

#[test]
fn error_serialization_contains_only_sanitized_codes() {
    assert_eq!(
        serde_json::to_string(&StorageFailure::UnlockFailed).unwrap(),
        "\"unlock_failed\""
    );
    assert_eq!(
        serde_json::to_string(&StorageFailure::KeyMissing).unwrap(),
        "\"key_missing\""
    );
}

#[test]
fn incomplete_initial_marker_recovers_only_with_intact_plaintext_and_no_other_artifacts() {
    for prefix in [b"".as_slice(), b"Carnyx encrypted".as_slice()] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("messages.sqlite");
        let secrets = Secrets::default();
        legacy(&path, 5);
        *secrets.secret.lock().unwrap() = Some(vec![4; 32]);
        fs::write(Paths::new(&path).marker, prefix).unwrap();
        drop(open_test(&path, &secrets).unwrap());
        assert_ciphertext(directory.path());
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("messages.sqlite");
    let secrets = Secrets::default();
    legacy(&path, 5);
    *secrets.secret.lock().unwrap() = Some(vec![4; 32]);
    fs::write(Paths::new(&path).marker, b"unrecognized marker").unwrap();
    assert_eq!(
        failure(open_test(&path, &secrets)),
        StorageFailure::MigrationFailed
    );
    assert!(plaintext(&path).unwrap());
}

#[test]
fn legacy_wal_is_flushed_and_removed_without_losing_uncheckpointed_rows() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("messages.sqlite");
    let secrets = Secrets::default();
    legacy(&path, 5);
    let c = Connection::open(&path).unwrap();
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; INSERT INTO messages(rowid,id,text,created_at,sender_id,recipient_id,direction,received_at) SELECT 99,'wal-only','WAL_PRIVATE_canary',103,peer_id,NULL,'local',NULL FROM local_identity").unwrap();
    let wal = migration::sidecars(&path).next().unwrap();
    let database_bytes = fs::read(&path).unwrap();
    let wal_bytes = fs::read(&wal).unwrap();
    drop(c);
    // Model a process crash: database header plus the durable WAL, without a live handle.
    fs::write(&path, database_bytes).unwrap();
    fs::write(&wal, wal_bytes).unwrap();
    let encrypted = open_test(&path, &secrets).unwrap();
    assert_eq!(
        encrypted
            .query_row("SELECT text FROM messages WHERE rowid=99", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "WAL_PRIVATE_canary"
    );
    drop(encrypted);
    assert_ciphertext(directory.path());
    assert!(!wal.exists());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
