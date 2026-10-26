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
        local_size INTEGER NOT NULL,
        local_mtime_ms INTEGER NOT NULL,
        fetched_ms INTEGER NOT NULL
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
    // 2: where reading stopped in a viewer (PRV-12), forgotten with recents.
    r#"
    CREATE TABLE reading_positions (
        uri TEXT PRIMARY KEY,
        location_id TEXT NOT NULL,
        page INTEGER NOT NULL,
        at_ms INTEGER NOT NULL
    );
    "#,
    // 3: BRW-12, what the Browse page shows and in which order.
    r#"
    CREATE TABLE browse_layout (
        section TEXT NOT NULL,
        item TEXT NOT NULL,
        hidden INTEGER NOT NULL DEFAULT 0,
        position INTEGER,
        PRIMARY KEY (section, item)
    );
    "#,
    // 4: `trash_items.bin`, unused: nothing reads or writes it (OPS-8). The
    // column stays, as dropping it takes a migration of its own.
    r#"
    ALTER TABLE trash_items ADD COLUMN bin BLOB;
    "#,
    // 5: ORG-1, files can be favourites too; the rows before were folders.
    r#"
    ALTER TABLE favourites ADD COLUMN is_dir INTEGER NOT NULL DEFAULT 1;
    "#,
    // 6: ORG-1, the icon chosen for a favourite (a name of
    // `org::favourites::ICONS`). NULL for the rows before and for no choice:
    // the default icon.
    r#"
    ALTER TABLE favourites ADD COLUMN icon TEXT;
    "#,
    // 7: `trash_drain`, unused: nothing reads or writes it (OPS-8). The table
    // stays, as dropping it takes a migration of its own.
    r#"
    CREATE TABLE trash_drain (due INTEGER NOT NULL);
    INSERT INTO trash_drain VALUES (1);
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
                 'dircache','trash_items','reading_positions')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 8);
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
    fn favourites_from_before_files_stay_folders() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v4.db");
        {
            // A database as schema 4 left it, with one favourite.
            let c = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..4] {
                c.execute_batch(sql).unwrap();
            }
            c.pragma_update(None, "user_version", 4).unwrap();
            c.execute(
                "INSERT INTO favourites(uri, label, colour, position) \
                 VALUES ('lautta://user-documents/Uni', 'Uni', NULL, 0)",
                [],
            )
            .unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.version().unwrap(), SCHEMA_VERSION);
        let favourites = crate::org::favourites::Favourites::new(db.clone());
        let list = favourites.list().unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].is_dir, "rows from before are folders");
        let file = crate::uri::Uri::parse("lautta://user-documents/Uni/a.pdf").unwrap();
        assert!(!favourites.add(&file, "A", None, false).unwrap().is_dir);
    }

    #[test]
    fn favourites_from_before_icons_keep_their_default_icon() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v5.db");
        {
            // A database as schema 5 left it, with a folder and a file.
            let c = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..5] {
                c.execute_batch(sql).unwrap();
            }
            c.pragma_update(None, "user_version", 5).unwrap();
            c.execute_batch(
                "INSERT INTO favourites(uri, label, colour, position, is_dir) \
                 VALUES ('lautta://user-documents/Uni', 'Uni', '#e7a33c', 0, 1); \
                 INSERT INTO favourites(uri, label, colour, position, is_dir) \
                 VALUES ('lautta://user-documents/a.pdf', 'A', NULL, 1, 0);",
            )
            .unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.version().unwrap(), SCHEMA_VERSION);
        let favourites = crate::org::favourites::Favourites::new(db.clone());
        let list = favourites.list().unwrap();
        assert_eq!(
            list.iter()
                .map(|f| (f.label.as_str(), f.colour.as_deref(), f.icon.as_deref()))
                .collect::<Vec<_>>(),
            [("Uni", Some("#e7a33c"), None), ("A", None, None)],
            "rows from before keep label and colour and have no icon chosen"
        );
        assert_eq!(list[0].icon_source(), "image://theme/icon-m-favorite");
        assert_eq!(list[1].icon_source(), "file-document");
        let uni = crate::uri::Uri::parse("lautta://user-documents/Uni").unwrap();
        let saved = favourites
            .save(&uni, "Uni", Some("#e7a33c"), Some("home"), true)
            .unwrap();
        assert_eq!(saved.icon.as_deref(), Some("home"));
        drop(favourites);
        drop(db);
        let reopened = crate::org::favourites::Favourites::new(Db::open(&path).unwrap());
        let again = reopened.get_by_uri(&uni).unwrap().unwrap();
        assert_eq!(again.icon.as_deref(), Some("home"), "the icon is stored");
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
    fn older_schemas_migrate_forward_and_keep_their_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.db");
        {
            let c = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..3] {
                c.execute_batch(sql).unwrap();
            }
            c.pragma_update(None, "user_version", 3).unwrap();
            c.execute(
                "INSERT INTO trash_items(original_uri, trashed_at, stored_name) VALUES ('lautta://a/b', 1, 's')",
                [],
            )
            .unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.version().unwrap(), SCHEMA_VERSION);
        let uri: String = db
            .lock()
            .query_row(
                "SELECT original_uri FROM trash_items WHERE stored_name = 's'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(uri, "lautta://a/b");
    }

    #[test]
    fn a_device_database_at_schema_7_keeps_opening_with_its_trash_rows() {
        // The database on the device is at schema 7, with trash rows that
        // name a bin. A migration that ran there may not be dropped or
        // renumbered, or this app would call that database newer than itself.
        const DEVICE_SCHEMA: usize = 7;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v7.db");
        {
            let c = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..DEVICE_SCHEMA] {
                c.execute_batch(sql).unwrap();
            }
            c.pragma_update(None, "user_version", DEVICE_SCHEMA).unwrap();
            c.execute(
                "INSERT INTO trash_items(original_uri, trashed_at, stored_name, bin) \
                 VALUES ('lautta://user-documents/a', 7, 's', X'2f442f2e6c61757474612d7472617368')",
                [],
            )
            .unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.version().unwrap(), SCHEMA_VERSION);
        let row: (String, i64) = db
            .lock()
            .query_row(
                "SELECT original_uri, trashed_at FROM trash_items WHERE stored_name = 's'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, ("lautta://user-documents/a".to_owned(), 7));
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
