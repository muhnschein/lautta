// SPDX-License-Identifier: LGPL-2.1-or-later
//! The app database (SPEC DAT-1, DAT-2): SQLite in WAL mode, schema versioned
//! with `PRAGMA user_version`, forward migrations only.

use crate::error::{Error, ErrorKind, Result};
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

/// Migrations in order; index + 1 is the schema version they produce.
const MIGRATIONS: &[&str] = &[
    // 1: DAT-2 tables.
    r#"
    CREATE TABLE favourites (
        id INTEGER PRIMARY KEY,
        uri TEXT NOT NULL UNIQUE,
        label TEXT NOT NULL,
        colour TEXT,
        position INTEGER NOT NULL
    );
    CREATE TABLE recents (
        id INTEGER PRIMARY KEY,
        uri TEXT NOT NULL,
        name TEXT NOT NULL,
        kind TEXT NOT NULL,
        location_id TEXT NOT NULL,
        at_ms INTEGER NOT NULL,
        UNIQUE(uri, kind)
    );
    CREATE INDEX recents_at ON recents(at_ms DESC);
    CREATE TABLE transfers (
        id INTEGER PRIMARY KEY,
        kind TEXT NOT NULL,
        title TEXT NOT NULL,
        state TEXT NOT NULL,
        dest_location TEXT NOT NULL,
        dest_uri TEXT NOT NULL,
        position INTEGER NOT NULL,
        created_ms INTEGER NOT NULL,
        finished_ms INTEGER,
        bytes_total INTEGER NOT NULL DEFAULT 0,
        bytes_done INTEGER NOT NULL DEFAULT 0,
        items_total INTEGER NOT NULL DEFAULT 0,
        items_done INTEGER NOT NULL DEFAULT 0,
        items_failed INTEGER NOT NULL DEFAULT 0,
        waiting_reason TEXT,
        options TEXT NOT NULL DEFAULT '{}',
        error TEXT
    );
    CREATE TABLE transfer_items (
        id INTEGER PRIMARY KEY,
        transfer_id INTEGER NOT NULL REFERENCES transfers(id) ON DELETE CASCADE,
        seq INTEGER NOT NULL,
        src_uri TEXT NOT NULL,
        dst_uri TEXT NOT NULL,
        kind TEXT NOT NULL,
        size INTEGER,
        state TEXT NOT NULL,
        committed INTEGER NOT NULL DEFAULT 0,
        temp_name BLOB,
        conflict TEXT,
        attempts INTEGER NOT NULL DEFAULT 0,
        error TEXT,
        UNIQUE(transfer_id, seq)
    );
    CREATE TABLE working_copies (
        id INTEGER PRIMARY KEY,
        remote_uri TEXT NOT NULL UNIQUE,
        local_path BLOB NOT NULL,
        base_size INTEGER,
        base_mtime_ms INTEGER,
        base_etag BLOB,
        local_mtime_ms INTEGER,
        pinned INTEGER NOT NULL DEFAULT 0,
        last_upload_ms INTEGER,
        state TEXT NOT NULL
    );
    CREATE TABLE dircache (
        uri TEXT PRIMARY KEY,
        location_id TEXT NOT NULL,
        fetched_ms INTEGER NOT NULL,
        entries BLOB NOT NULL
    );
    CREATE INDEX dircache_loc ON dircache(location_id);
    CREATE TABLE trash_items (
        id INTEGER PRIMARY KEY,
        original_uri TEXT NOT NULL,
        trashed_at INTEGER NOT NULL,
        stored_name TEXT NOT NULL UNIQUE,
        is_dir INTEGER NOT NULL DEFAULT 0,
        size INTEGER
    );
    CREATE TABLE location_prefs (
        location_id TEXT PRIMARY KEY,
        display_name TEXT,
        start_folder TEXT,
        no_listing_cache INTEGER NOT NULL DEFAULT 0,
        no_thumb_cache INTEGER NOT NULL DEFAULT 0,
        bulk_lanes INTEGER
    );
    CREATE TABLE recent_searches (
        id INTEGER PRIMARY KEY,
        location_id TEXT NOT NULL,
        query TEXT NOT NULL,
        at_ms INTEGER NOT NULL,
        UNIQUE(location_id, query)
    );
    CREATE TABLE adhoc_servers (
        location_id TEXT PRIMARY KEY,
        url TEXT NOT NULL,
        name TEXT NOT NULL,
        last_used_ms INTEGER NOT NULL
    );
    "#,
];

pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

/// Shared handle to the database. All access is serialised; callers run on
/// blocking threads, never on the GUI thread.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        restrict_mode(path);
        Db::init(conn)
    }

    pub fn open_in_memory() -> Result<Db> {
        Db::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&conn)?;
        Ok(Db {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        match self.conn.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn version(&self) -> Result<u32> {
        let v: u32 = self
            .lock()
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        Ok(v)
    }
}

fn migrate(conn: &Connection) -> Result<()> {
    let current: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(Error::new(
            ErrorKind::Unsupported,
            format!("database schema {current} is newer than this app ({SCHEMA_VERSION})"),
        ));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let version = i as u32 + 1;
        conn.execute_batch(&format!(
            "BEGIN;\n{sql}\nPRAGMA user_version = {version};\nCOMMIT;"
        ))?;
    }
    Ok(())
}

/// SEC-5: private files are 0600.
fn restrict_mode(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_has_current_schema() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.version().unwrap(), SCHEMA_VERSION);
        let n: i64 = db
            .lock()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN \
                 ('favourites','recents','transfers','transfer_items','working_copies',\
                 'dircache','trash_items')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 7);
    }

    #[test]
    fn reopen_is_idempotent_and_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/lautta.db");
        drop(Db::open(&path).unwrap());
        let db = Db::open(&path).unwrap();
        assert_eq!(db.version().unwrap(), SCHEMA_VERSION);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.db");
        {
            let c = Connection::open(&path).unwrap();
            c.pragma_update(None, "user_version", SCHEMA_VERSION + 1).unwrap();
        }
        let err = Db::open(&path).err().unwrap();
        assert_eq!(err.kind, ErrorKind::Unsupported);
    }

    #[test]
    fn foreign_keys_cascade() {
        let db = Db::open_in_memory().unwrap();
        let c = db.lock();
        c.execute(
            "INSERT INTO transfers(id,kind,title,state,dest_location,dest_uri,position,created_ms) \
             VALUES (1,'copy','t','queued','x','lautta://x/',0,0)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO transfer_items(transfer_id,seq,src_uri,dst_uri,kind,state) \
             VALUES (1,0,'lautta://x/a','lautta://y/a','file','pending')",
            [],
        )
        .unwrap();
        c.execute("DELETE FROM transfers WHERE id=1", []).unwrap();
        let n: i64 = c
            .query_row("SELECT count(*) FROM transfer_items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
}
