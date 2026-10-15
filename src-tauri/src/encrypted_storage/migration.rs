//! File promotion and recovery are independent of native credentials and chat services.
use super::{DatabaseKey, Result, StorageFailure, connection, integrity, key, plaintext};
use crate::credentials::Credentials;
use rusqlite::{Connection, OpenFlags, types::Value};
#[cfg(unix)]
use std::fs::File;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
const MARKER: &[u8] = b"Carnyx encrypted history migration v1\n";
pub(super) struct Paths {
    pub(super) main: PathBuf,
    pub(super) stage: PathBuf,
    pub(super) retired: PathBuf,
    pub(super) marker: PathBuf,
}
impl Paths {
    pub(super) fn new(path: &Path) -> Self {
        Self {
            main: path.into(),
            stage: path.with_extension("encrypted-pending.sqlite"),
            retired: path.with_extension("plaintext-retired.sqlite"),
            marker: path.with_extension("encryption-v1"),
        }
    }
}

pub(super) fn source(path: &Path) -> Result<Connection> {
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| StorageFailure::MigrationFailed)?;
    c.execute_batch("PRAGMA temp_store = MEMORY; BEGIN;")
        .map_err(|_| StorageFailure::MigrationFailed)?;
    integrity(&c).map_err(|_| StorageFailure::MigrationFailed)?;
    Ok(c)
}
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn schema(c: &Connection) -> rusqlite::Result<Vec<(String, String, String)>> {
    c.prepare("SELECT type,name,sql FROM sqlite_master WHERE name NOT GLOB 'sqlite_*' AND sql IS NOT NULL ORDER BY type,name")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?.collect()
}

// Streaming comparison includes rowid and every stored column. No hashes or plaintext backup files.
fn compare(original: &Path, encrypted: &Connection) -> Result<()> {
    let c = source(original)?;
    let source_schema = schema(&c).map_err(|_| StorageFailure::MigrationFailed)?;
    if source_schema != schema(encrypted).map_err(|_| StorageFailure::MigrationFailed)? {
        return Err(StorageFailure::MigrationFailed);
    }
    let version = |c: &Connection| c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0));
    if version(&c).map_err(|_| StorageFailure::MigrationFailed)?
        != version(encrypted).map_err(|_| StorageFailure::MigrationFailed)?
    {
        return Err(StorageFailure::MigrationFailed);
    }
    for (_, name, _) in source_schema.iter().filter(|(kind, _, _)| kind == "table") {
        let sql = format!("SELECT rowid,* FROM {} ORDER BY rowid", quote(name));
        let mut left = c
            .prepare(&sql)
            .map_err(|_| StorageFailure::MigrationFailed)?;
        let mut right = encrypted
            .prepare(&sql)
            .map_err(|_| StorageFailure::MigrationFailed)?;
        let count = left.column_count();
        let decode = |r: &rusqlite::Row<'_>| {
            (0..count)
                .map(|i| r.get::<_, Value>(i))
                .collect::<rusqlite::Result<Vec<_>>>()
        };
        let mut a = left
            .query_map([], decode)
            .map_err(|_| StorageFailure::MigrationFailed)?;
        let mut b = right
            .query_map([], decode)
            .map_err(|_| StorageFailure::MigrationFailed)?;
        loop {
            match (a.next(), b.next()) {
                (None, None) => break,
                (Some(Ok(a)), Some(Ok(b))) if a == b => {}
                _ => return Err(StorageFailure::MigrationFailed),
            }
        }
    }
    Ok(())
}

fn sync_file(path: &Path) -> Result<()> {
    // Windows FlushFileBuffers (used by sync_all) requires write access.
    // Open the existing candidate without creating or truncating it.
    OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|f| f.sync_all())
        .map_err(|_| StorageFailure::MigrationFailed)
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path.parent().ok_or(StorageFailure::MigrationFailed)?)
        .and_then(|f| f.sync_all())
        .map_err(|_| StorageFailure::MigrationFailed)?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn remove(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
pub(super) fn sidecars(path: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    ["-wal", "-shm", "-journal"].into_iter().map(|suffix| {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        PathBuf::from(name)
    })
}
fn remove_database(path: &Path) -> std::io::Result<()> {
    for sidecar in sidecars(path) {
        remove(&sidecar)?;
    }
    remove(path)
}

fn copy(paths: &Paths, key: &DatabaseKey) -> Result<()> {
    let run = || -> rusqlite::Result<()> {
        let original = Connection::open(&paths.main)?;
        original.execute_batch("PRAGMA temp_store = MEMORY;")?;
        // Flush an old WAL before retiring the source; never carry plaintext pages alongside the new file.
        let (busy, _, _): (i64, i64, i64) =
            original.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?;
        if busy != 0 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        drop(original);
        Ok(())
    };
    run().map_err(|_| StorageFailure::MigrationFailed)?;
    remove_database(&paths.stage).map_err(|_| StorageFailure::MigrationFailed)?;
    let mut target = connection(&paths.stage, key, true)?;
    target
        .execute_batch("PRAGMA foreign_keys = OFF;")
        .map_err(|_| StorageFailure::MigrationFailed)?;
    let path = paths.main.to_str().ok_or(StorageFailure::MigrationFailed)?;
    target
        .execute("ATTACH DATABASE ?1 AS legacy KEY ''", [path])
        .map_err(|_| StorageFailure::MigrationFailed)?;
    let run = || -> rusqlite::Result<()> {
        let transaction =
            target.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let entries = transaction.prepare("SELECT type,name,sql FROM legacy.sqlite_master WHERE name NOT GLOB 'sqlite_*' AND sql IS NOT NULL ORDER BY type,name")?
            .query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        // Carnyx uses ordinary rowid tables and indexes. Reject unfamiliar objects rather than dropping them.
        let sequences: i64 = transaction.query_row(
            "SELECT count(*) FROM legacy.sqlite_master WHERE name='sqlite_sequence'",
            [],
            |r| r.get(0),
        )?;
        if sequences != 0 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        if entries.iter().any(|(kind, _, sql)| {
            !["table", "index"].contains(&kind.as_str())
                || sql.to_uppercase().contains("WITHOUT ROWID")
                || sql.to_uppercase().contains("VIRTUAL TABLE")
        }) {
            return Err(rusqlite::Error::InvalidQuery);
        }
        for (_, name, ddl) in entries.iter().filter(|(kind, _, _)| kind == "table") {
            transaction.execute_batch(ddl)?;
            let name = quote(name);
            transaction.execute_batch(&format!("INSERT INTO main.{name}(rowid,{}) SELECT rowid,* FROM legacy.{name} ORDER BY rowid", {
                transaction.prepare(&format!("PRAGMA legacy.table_info({name})"))?.query_map([], |r| r.get::<_,String>(1))?
                    .collect::<rusqlite::Result<Vec<_>>>()?.iter().map(|column|quote(column)).collect::<Vec<_>>().join(",")
            }))?;
        }
        for (_, _, ddl) in entries.iter().filter(|(kind, _, _)| kind == "index") {
            transaction.execute_batch(ddl)?;
        }
        let version: i64 = transaction.query_row("PRAGMA legacy.user_version", [], |r| r.get(0))?;
        transaction.pragma_update(None, "user_version", version)?;
        transaction.commit()?;
        target.execute_batch("DETACH DATABASE legacy; PRAGMA foreign_keys = ON;")?;
        Ok(())
    };
    let mut run = run;
    run().map_err(|_| StorageFailure::MigrationFailed)?;
    drop(target);
    sync_file(&paths.stage)?;
    let mut verified = connection(&paths.stage, key, false)?;
    integrity(&verified)?;
    compare(&paths.main, &verified)?;
    crate::migrations::validate_upgrade(&mut verified)
        .map_err(|_| StorageFailure::MigrationFailed)?;
    drop(verified);
    sync_file(&paths.stage)?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum MigrationPoint {
    KeyPersisted,
    MarkerCreated,
    CopyVerified,
    SourceRetired,
    TargetPromoted,
    BeforeCleanup,
    SourceRemoved,
}
pub(super) fn open_observed(
    path: &Path,
    credentials: &dyn Credentials,
    identifier: &str,
    observe: impl Fn(MigrationPoint) -> Result<()>,
) -> Result<Connection> {
    let parent = path.parent().ok_or(StorageFailure::StorageUnavailable)?;
    fs::create_dir_all(parent).map_err(|_| StorageFailure::StorageUnavailable)?;
    let paths = Paths::new(path);
    let key = key(&paths, credentials, identifier)?;
    observe(MigrationPoint::KeyPersisted)?;
    let exists = |path: &Path| {
        path.try_exists()
            .map_err(|_| StorageFailure::StorageUnavailable)
    };
    let mut marked = exists(&paths.marker)?;
    if marked {
        let bytes = fs::read(&paths.marker).map_err(|_| StorageFailure::MigrationFailed)?;
        if bytes != MARKER {
            // Creation may have stopped before write_all/sync_all. Nothing else has
            // begun yet, so only a prefix of our own marker with an intact source is recoverable.
            if !MARKER.starts_with(&bytes)
                || !exists(&paths.main)?
                || !plaintext(&paths.main)?
                || exists(&paths.stage)?
                || exists(&paths.retired)?
            {
                return Err(StorageFailure::MigrationFailed);
            }
            remove(&paths.marker).map_err(|_| StorageFailure::MigrationFailed)?;
            sync_directory(&paths.marker)?;
            marked = false;
        }
    }
    if !marked && (exists(&paths.stage)? || exists(&paths.retired)?) {
        return Err(StorageFailure::MigrationFailed);
    }
    if exists(&paths.main)? && plaintext(&paths.main)? {
        if exists(&paths.retired)? {
            return Err(StorageFailure::MigrationFailed);
        }
        if !marked {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&paths.marker)
                .map_err(|_| StorageFailure::MigrationFailed)?;
            file.write_all(MARKER)
                .and_then(|_| file.sync_all())
                .map_err(|_| StorageFailure::MigrationFailed)?;
            sync_directory(&paths.marker)?;
        }
        observe(MigrationPoint::MarkerCreated)?;
        copy(&paths, &key)?;
        observe(MigrationPoint::CopyVerified)?;
        fs::rename(&paths.main, &paths.retired).map_err(|_| StorageFailure::MigrationFailed)?;
        sync_directory(&paths.main)?;
        observe(MigrationPoint::SourceRetired)?;
    }
    if exists(&paths.retired)? && !exists(&paths.main)? {
        if !exists(&paths.stage)? || !plaintext(&paths.retired)? {
            return Err(StorageFailure::MigrationFailed);
        }
        let staged = connection(&paths.stage, &key, false)?;
        integrity(&staged)?;
        compare(&paths.retired, &staged)?;
        drop(staged);
        for sidecar in sidecars(&paths.main) {
            remove(&sidecar).map_err(|_| StorageFailure::CleanupFailed)?;
        }
        fs::rename(&paths.stage, &paths.main).map_err(|_| StorageFailure::MigrationFailed)?;
        sync_directory(&paths.main)?;
        observe(MigrationPoint::TargetPromoted)?;
    }
    if exists(&paths.stage)? || (exists(&paths.marker)? && !exists(&paths.main)?) {
        return Err(StorageFailure::MigrationFailed);
    }
    let c = connection(&paths.main, &key, !exists(&paths.main)?)?;
    integrity(&c)?;
    if exists(&paths.retired)? {
        compare(&paths.retired, &c)?;
        observe(MigrationPoint::BeforeCleanup)?;
        // The encrypted database is closed before deleting the old main-name sidecars.
        drop(c);
        for sidecar in sidecars(&paths.main) {
            remove(&sidecar).map_err(|_| StorageFailure::CleanupFailed)?;
        }
        remove_database(&paths.retired).map_err(|_| StorageFailure::CleanupFailed)?;
        observe(MigrationPoint::SourceRemoved)?;
        sync_directory(&paths.main).map_err(|_| StorageFailure::CleanupFailed)?;
        remove(&paths.marker).map_err(|_| StorageFailure::CleanupFailed)?;
        sync_directory(&paths.main).map_err(|_| StorageFailure::CleanupFailed)?;
        return connection(&paths.main, &key, false);
    }
    if exists(&paths.marker)? {
        remove(&paths.marker).map_err(|_| StorageFailure::CleanupFailed)?;
        sync_directory(&paths.main).map_err(|_| StorageFailure::CleanupFailed)?;
    }
    Ok(c)
}
