// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::entry::{ms_to_system_time, system_time_to_ms, Entry, EntryFlags, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::filter::fold_text;
use crate::uri::LocationId;
use crate::vpath::VPath;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

pub const SCHEMA_VERSION: i32 = 1;
pub const QUERY_LIMIT: usize = 2000;
const TRIGRAM_MIN: usize = 3;

const SCHEMA: &str = "
CREATE TABLE root (
  id INTEGER PRIMARY KEY,
  location_id TEXT NOT NULL,
  path BLOB NOT NULL,
  pass INTEGER NOT NULL DEFAULT 1,
  state TEXT NOT NULL DEFAULT 'pending',
  dir_version BLOB,
  checked_pass INTEGER NOT NULL DEFAULT 0,
  checked_ms INTEGER,
  error TEXT,
  UNIQUE(location_id, path)
);
CREATE TABLE node (
  id INTEGER PRIMARY KEY,
  root INTEGER NOT NULL REFERENCES root(id) ON DELETE CASCADE,
  parent BLOB NOT NULL,
  name BLOB NOT NULL,
  fold TEXT NOT NULL,
  kind INTEGER NOT NULL,
  flags INTEGER NOT NULL DEFAULT 0,
  size INTEGER,
  mtime_ms INTEGER,
  dir_version BLOB,
  checked_pass INTEGER NOT NULL DEFAULT 0,
  UNIQUE(root, parent, name)
);
CREATE INDEX node_todo ON node(root, checked_pass) WHERE kind = 1;
CREATE VIRTUAL TABLE node_fts USING fts5(
  fold, content='node', content_rowid='id',
  tokenize='trigram case_sensitive 1', detail='none', columnsize=0);
CREATE TRIGGER node_ai AFTER INSERT ON node BEGIN
  INSERT INTO node_fts(rowid, fold) VALUES (new.id, new.fold);
END;
CREATE TRIGGER node_ad AFTER DELETE ON node BEGIN
  INSERT INTO node_fts(node_fts, rowid, fold) VALUES ('delete', old.id, old.fold);
END;
CREATE TRIGGER node_au AFTER UPDATE OF fold ON node BEGIN
  INSERT INTO node_fts(node_fts, rowid, fold) VALUES ('delete', old.id, old.fold);
  INSERT INTO node_fts(rowid, fold) VALUES (new.id, new.fold);
END;
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootState {
    Pending,
    Crawling,
    Ready,
    Failed,
}

impl RootState {
    pub fn name(self) -> &'static str {
        match self {
            RootState::Pending => "pending",
            RootState::Crawling => "crawling",
            RootState::Ready => "ready",
            RootState::Failed => "failed",
        }
    }

    fn parse(s: &str) -> RootState {
        match s {
            "crawling" => RootState::Crawling,
            "ready" => RootState::Ready,
            "failed" => RootState::Failed,
            _ => RootState::Pending,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootInfo {
    pub id: i64,
    pub location: LocationId,
    pub path: VPath,
    pub pass: i64,
    pub state: RootState,
    pub names: i64,
    pub checked_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub path: VPath,
    pub version: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedEntry {
    pub root: i64,
    pub folder: VPath,
    pub entry: Entry,
}

#[derive(Clone)]
pub struct IndexStore {
    conn: Arc<Mutex<Connection>>,
    ready: Arc<Mutex<HashMap<i64, RootInfo>>>,
    path: PathBuf,
}

fn kind_code(kind: Kind) -> i64 {
    match kind {
        Kind::File => 0,
        Kind::Dir => 1,
        Kind::Symlink => 2,
        Kind::Special => 3,
        Kind::Unknown => 4,
    }
}

fn kind_of(code: i64) -> Kind {
    match code {
        0 => Kind::File,
        1 => Kind::Dir,
        2 => Kind::Symlink,
        3 => Kind::Special,
        _ => Kind::Unknown,
    }
}

fn sql(e: rusqlite::Error) -> Error {
    let full = matches!(
        &e,
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::DiskFull
    );
    if full {
        Error::new(ErrorKind::NoSpace, "the search index has no room to grow")
    } else {
        Error::new(ErrorKind::Io, format!("search index: {e}"))
    }
}

fn mark_one(
    conn: &Connection,
    root: i64,
    root_path: &VPath,
    folder: &VPath,
    pass: i64,
    version: Option<&[u8]>,
) -> Result<()> {
    if folder == root_path {
        conn.execute(
            "UPDATE root SET checked_pass = ?2, dir_version = ?3 WHERE id = ?1",
            params![root, pass, version],
        )
        .map_err(sql)?;
        return Ok(());
    }
    let (Some(parent), Some(name)) = (folder.parent(), folder.name()) else {
        return Ok(());
    };
    conn.execute(
        "UPDATE node SET checked_pass = ?4, dir_version = ?5 WHERE root = ?1 AND parent = ?2 AND name = ?3",
        params![root, parent.as_bytes(), name, pass, version],
    )
    .map_err(sql)?;
    Ok(())
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA secure_delete = OFF;
         PRAGMA journal_size_limit = 4194304;
         PRAGMA foreign_keys = ON;",
    )
}

fn fresh(path: &Path) -> rusqlite::Result<Connection> {
    for suffix in ["", "-wal", "-shm"] {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(p));
    }
    let conn = Connection::open(path)?;
    configure(&conn)?;
    conn.execute_batch(SCHEMA)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(conn)
}

fn reopen(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version != SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let ok: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if ok != "ok" {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(conn)
}

fn ready_where(conn: &Connection, cond: &str, args: impl rusqlite::Params) -> Result<HashMap<i64, RootInfo>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, location_id, path, pass, state, checked_ms, 0 FROM root
             WHERE checked_ms IS NOT NULL AND {cond}"
        ))
        .map_err(sql)?;
    let rows = stmt.query_map(args, IndexStore::root_row).map_err(sql)?;
    let mut out = HashMap::new();
    for row in rows {
        let root = row.map_err(sql)?;
        out.insert(root.id, root);
    }
    Ok(out)
}

fn ready_roots(conn: &Connection) -> Result<HashMap<i64, RootInfo>> {
    ready_where(conn, "1", [])
}

fn needle_glob(fold: &str) -> String {
    let mut out = String::from("*");
    for c in fold.chars() {
        match c {
            '*' | '?' | '[' => {
                out.push('[');
                out.push(c);
                out.push(']');
            }
            _ => out.push(c),
        }
    }
    out.push('*');
    out
}

fn subtree_bounds(folder: &VPath) -> (Vec<u8>, Vec<u8>) {
    let mut low = folder.as_bytes().to_vec();
    if !folder.is_root() {
        low.push(b'/');
    }
    let mut high = low.clone();
    if let Some(last) = high.last_mut() {
        *last += 1;
    }
    (low, high)
}

impl IndexStore {
    pub fn open(path: &Path) -> Result<IndexStore> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = match reopen(path) {
            Ok(c) => c,
            Err(_) => fresh(path).map_err(sql)?,
        };
        let ready = ready_roots(&conn)?;
        Ok(IndexStore {
            conn: Arc::new(Mutex::new(conn)),
            ready: Arc::new(Mutex::new(ready)),
            path: path.to_path_buf(),
        })
    }

    pub fn rebuild(&self) -> Result<()> {
        let mut conn = self.lock();
        *conn = fresh(&self.path).map_err(sql)?;
        self.ready().clear();
        Ok(())
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        match self.conn.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    fn ready(&self) -> MutexGuard<'_, HashMap<i64, RootInfo>> {
        match self.ready.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    pub fn ready_root(&self, location: &str, path: &VPath) -> Option<RootInfo> {
        self.ready()
            .values()
            .find(|r| r.location == location && &r.path == path)
            .cloned()
    }

    pub fn root(&self, location: &str, path: &VPath) -> Result<RootInfo> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO root(location_id, path) VALUES (?1, ?2) ON CONFLICT(location_id, path) DO NOTHING",
            params![location, path.as_bytes()],
        )
        .map_err(sql)?;
        Self::root_where(
            &conn,
            "location_id = ?1 AND path = ?2",
            params![location, path.as_bytes()],
        )?
        .ok_or_else(|| Error::new(ErrorKind::Internal, "index root vanished"))
    }

    pub fn roots(&self) -> Result<Vec<RootInfo>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT r.id, r.location_id, r.path, r.pass, r.state, r.checked_ms,
                        (SELECT COUNT(*) FROM node n WHERE n.root = r.id)
                 FROM root r ORDER BY r.id",
            )
            .map_err(sql)?;
        let rows = stmt.query_map([], Self::root_row).map_err(sql)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(sql)
    }

    fn root_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RootInfo> {
        let path: Vec<u8> = r.get(2)?;
        let state: String = r.get(4)?;
        Ok(RootInfo {
            id: r.get(0)?,
            location: r.get(1)?,
            path: VPath::parse(&path).unwrap_or_else(|_| VPath::root()),
            pass: r.get(3)?,
            state: RootState::parse(&state),
            checked_ms: r.get(5)?,
            names: r.get(6)?,
        })
    }

    fn root_where(conn: &Connection, cond: &str, args: impl rusqlite::Params) -> Result<Option<RootInfo>> {
        conn.query_row(
            &format!(
                "SELECT r.id, r.location_id, r.path, r.pass, r.state, r.checked_ms,
                        (SELECT COUNT(*) FROM node n WHERE n.root = r.id)
                 FROM root r WHERE {cond}"
            ),
            args,
            Self::root_row,
        )
        .optional()
        .map_err(sql)
    }

    pub fn root_info(&self, id: i64) -> Result<Option<RootInfo>> {
        Self::root_where(&self.lock(), "r.id = ?1", params![id])
    }

    pub fn remove_root(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM node WHERE root = ?1", params![id])
            .map_err(sql)?;
        conn.execute("DELETE FROM root WHERE id = ?1", params![id])
            .map_err(sql)?;
        self.ready().remove(&id);
        Ok(())
    }

    pub fn set_state(&self, root: i64, state: RootState, error: Option<&str>) -> Result<()> {
        self.lock()
            .execute(
                "UPDATE root SET state = ?2, error = ?3 WHERE id = ?1",
                params![root, state.name(), error],
            )
            .map_err(sql)?;
        Ok(())
    }

    pub fn begin_pass(&self, root: i64) -> Result<i64> {
        let conn = self.lock();
        conn.execute(
            "UPDATE root SET pass = pass + 1 WHERE id = ?1 AND checked_pass >= pass
               AND NOT EXISTS (SELECT 1 FROM node WHERE root = ?1 AND kind = 1 AND checked_pass < root.pass)",
            params![root],
        )
        .map_err(sql)?;
        conn.query_row("SELECT pass FROM root WHERE id = ?1", params![root], |r| r.get(0))
            .map_err(sql)
    }

    pub fn finish_pass(&self, root: i64, now_ms: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE root SET state = 'ready', checked_ms = ?2, error = NULL WHERE id = ?1",
            params![root, now_ms],
        )
        .map_err(sql)?;
        let done = ready_where(&conn, "id = ?1", params![root])?;
        self.ready().extend(done);
        Ok(())
    }

    pub fn root_folder(&self, root: i64) -> Result<(Folder, i64)> {
        let conn = self.lock();
        conn.query_row(
            "SELECT path, dir_version, checked_pass FROM root WHERE id = ?1",
            params![root],
            |r| {
                let path: Vec<u8> = r.get(0)?;
                Ok((
                    Folder {
                        path: VPath::parse(&path).unwrap_or_else(|_| VPath::root()),
                        version: r.get(1)?,
                    },
                    r.get::<_, i64>(2)?,
                ))
            },
        )
        .map_err(sql)
    }

    pub fn pending_folders(&self, root: i64, pass: i64, limit: usize) -> Result<Vec<Folder>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT parent, name, dir_version FROM node
                 WHERE root = ?1 AND kind = 1 AND checked_pass < ?2 ORDER BY length(parent), id LIMIT ?3",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![root, pass, limit as i64], |r| {
                let parent: Vec<u8> = r.get(0)?;
                let name: Vec<u8> = r.get(1)?;
                Ok((parent, name, r.get::<_, Option<Vec<u8>>>(2)?))
            })
            .map_err(sql)?;
        let mut out = Vec::new();
        for row in rows {
            let (parent, name, version) = row.map_err(sql)?;
            let Ok(path) = VPath::parse(&parent).and_then(|p| p.join(&name)) else {
                continue;
            };
            out.push(Folder { path, version });
        }
        Ok(out)
    }

    pub fn mark_checked(
        &self,
        root: i64,
        root_path: &VPath,
        folder: &VPath,
        pass: i64,
        version: Option<&[u8]>,
    ) -> Result<()> {
        mark_one(&self.lock(), root, root_path, folder, pass, version)
    }

    pub fn mark_checked_many(
        &self,
        root: i64,
        root_path: &VPath,
        pass: i64,
        folders: &[(VPath, Option<Vec<u8>>)],
    ) -> Result<()> {
        if folders.is_empty() {
            return Ok(());
        }
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(sql)?;
        for (folder, version) in folders {
            mark_one(&tx, root, root_path, folder, pass, version.as_deref())?;
        }
        tx.commit().map_err(sql)
    }

    pub fn mark_subtree_checked(&self, root: i64, folder: &VPath, pass: i64) -> Result<usize> {
        let conn = self.lock();
        if folder.is_root() {
            return conn
                .execute(
                    "UPDATE node SET checked_pass = ?2 WHERE root = ?1 AND kind = 1 AND checked_pass < ?2",
                    params![root, pass],
                )
                .map_err(sql);
        }
        let (low, high) = subtree_bounds(folder);
        conn.execute(
            "UPDATE node SET checked_pass = ?2 WHERE root = ?1 AND kind = 1 AND checked_pass < ?2
             AND (parent = ?3 OR (parent >= ?4 AND parent < ?5))",
            params![root, pass, folder.as_bytes(), low, high],
        )
        .map_err(sql)
    }

    pub fn children(&self, root: i64, folder: &VPath) -> Result<Vec<Entry>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare("SELECT name, kind, flags, size, mtime_ms FROM node WHERE root = ?1 AND parent = ?2")
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![root, folder.as_bytes()], Self::entry_row)
            .map_err(sql)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(sql)
    }

    fn entry_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
        let name: Vec<u8> = r.get(0)?;
        let mut e = Entry::new(&name, kind_of(r.get(1)?));
        e.flags = EntryFlags::from_bits_truncate(r.get::<_, i64>(2)? as u16);
        e.size = r.get::<_, Option<i64>>(3)?.and_then(|s| u64::try_from(s).ok());
        e.modified = r.get::<_, Option<i64>>(4)?.map(ms_to_system_time);
        Ok(e)
    }

    pub fn replace_folder(&self, root: i64, folder: &VPath, entries: &[Entry]) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(sql)?;
        {
            let mut existing = tx
                .prepare("SELECT name, kind FROM node WHERE root = ?1 AND parent = ?2")
                .map_err(sql)?;
            let old: Vec<(Vec<u8>, i64)> = existing
                .query_map(params![root, folder.as_bytes()], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(sql)?
                .collect::<rusqlite::Result<_>>()
                .map_err(sql)?;
            let fresh: std::collections::HashMap<&[u8], &Entry> =
                entries.iter().map(|e| (e.name.as_slice(), e)).collect();
            for (name, kind) in &old {
                let gone = match fresh.get(name.as_slice()) {
                    None => true,
                    Some(e) => kind_code(e.kind) != *kind,
                };
                if gone {
                    if *kind == 1 {
                        if let Ok(child) = folder.join(name) {
                            let (low, high) = subtree_bounds(&child);
                            tx.execute(
                                "DELETE FROM node WHERE root = ?1 AND (parent = ?2 OR (parent >= ?3 AND parent < ?4))",
                                params![root, child.as_bytes(), low, high],
                            )
                            .map_err(sql)?;
                        }
                    }
                    tx.execute(
                        "DELETE FROM node WHERE root = ?1 AND parent = ?2 AND name = ?3",
                        params![root, folder.as_bytes(), name],
                    )
                    .map_err(sql)?;
                }
            }
            let mut upsert = tx
                .prepare(
                    "INSERT INTO node(root, parent, name, fold, kind, flags, size, mtime_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(root, parent, name) DO UPDATE SET
                       flags = excluded.flags, size = excluded.size, mtime_ms = excluded.mtime_ms",
                )
                .map_err(sql)?;
            for e in entries {
                upsert
                    .execute(params![
                        root,
                        folder.as_bytes(),
                        e.name,
                        fold_text(&e.display_name()),
                        kind_code(e.kind),
                        i64::from(e.flags.bits()),
                        e.size.and_then(|s| i64::try_from(s).ok()),
                        e.modified.map(system_time_to_ms),
                    ])
                    .map_err(sql)?;
            }
        }
        tx.commit().map_err(sql)
    }

    pub fn query(&self, roots: &[i64], text: &str, limit: usize) -> Result<Vec<IndexedEntry>> {
        let mut out = Vec::new();
        self.query_each(roots, text, |found| {
            out.push(found);
            out.len() < limit
        })?;
        Ok(out)
    }

    pub fn query_each(
        &self,
        roots: &[i64],
        text: &str,
        mut each: impl FnMut(IndexedEntry) -> bool,
    ) -> Result<()> {
        if roots.is_empty() {
            return Ok(());
        }
        let fold = fold_text(text.trim());
        let ids = roots.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        let conn = self.lock();
        let base = "SELECT n.root, n.parent, n.name, n.kind, n.flags, n.size, n.mtime_ms FROM node n";
        let (sql_text, arg) = if fold.chars().count() >= TRIGRAM_MIN {
            (
                format!(
                    "{base} JOIN node_fts f ON f.rowid = n.id
                     WHERE f.fold GLOB ?1 AND n.root IN ({ids})"
                ),
                needle_glob(&fold),
            )
        } else {
            (
                format!("{base} WHERE instr(n.fold, ?1) > 0 AND n.root IN ({ids})"),
                fold,
            )
        };
        let mut stmt = conn.prepare(&sql_text).map_err(sql)?;
        let rows = stmt
            .query_map(params![arg], |r| {
                let parent: Vec<u8> = r.get(1)?;
                let name: Vec<u8> = r.get(2)?;
                let mut e = Entry::new(&name, kind_of(r.get(3)?));
                e.flags = EntryFlags::from_bits_truncate(r.get::<_, i64>(4)? as u16);
                e.size = r.get::<_, Option<i64>>(5)?.and_then(|s| u64::try_from(s).ok());
                e.modified = r.get::<_, Option<i64>>(6)?.map(ms_to_system_time);
                Ok((r.get::<_, i64>(0)?, parent, e))
            })
            .map_err(sql)?;
        for row in rows {
            let (root, parent, entry) = row.map_err(sql)?;
            let Ok(folder) = VPath::parse(&parent) else {
                continue;
            };
            if !each(IndexedEntry { root, folder, entry }) {
                break;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn hold_until(
        &self,
        held: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    ) {
        let _conn = self.lock();
        let _ = held.send(());
        let _ = release.recv();
    }

    pub fn integrity_ok(&self) -> bool {
        self.lock()
            .execute("INSERT INTO node_fts(node_fts) VALUES ('integrity-check')", [])
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn store() -> (tempfile::TempDir, IndexStore) {
        let dir = tempfile::tempdir().unwrap();
        let s = IndexStore::open(&dir.path().join("names.db")).unwrap();
        (dir, s)
    }

    fn vp(s: &str) -> VPath {
        VPath::parse(s.as_bytes()).unwrap()
    }

    fn file(name: &str) -> Entry {
        let mut e = Entry::new(name.as_bytes(), Kind::File);
        e.size = Some(3);
        e.modified = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000));
        e
    }

    fn dir(name: &str) -> Entry {
        Entry::new(name.as_bytes(), Kind::Dir)
    }

    fn found(s: &IndexStore, roots: &[i64], text: &str) -> Vec<String> {
        let mut out: Vec<String> = s
            .query(roots, text, QUERY_LIMIT)
            .unwrap()
            .into_iter()
            .map(|h| h.folder.join(&h.entry.name).unwrap().display())
            .collect();
        out.sort();
        out
    }

    #[test]
    fn a_root_is_ready_once_its_first_pass_finished_until_removed_or_rebuilt() {
        let (dir, s) = store();
        let a = s.root("a", &VPath::root()).unwrap();
        let b = s.root("b", &VPath::root()).unwrap();
        assert_eq!(s.ready_root("a", &VPath::root()), None, "never crawled");
        s.finish_pass(a.id, 7).unwrap();
        s.finish_pass(b.id, 8).unwrap();
        let ready = s.ready_root("a", &VPath::root()).unwrap();
        assert_eq!((ready.id, ready.checked_ms), (a.id, Some(7)));
        assert_eq!(s.ready_root("a", &vp("sub")), None);
        let reopened = IndexStore::open(&dir.path().join("names.db")).unwrap();
        assert_eq!(reopened.ready_root("b", &VPath::root()).map(|r| r.id), Some(b.id));
        s.remove_root(a.id).unwrap();
        assert_eq!(s.ready_root("a", &VPath::root()), None);
        assert!(s.ready_root("b", &VPath::root()).is_some());
        s.rebuild().unwrap();
        assert_eq!(s.ready_root("b", &VPath::root()), None);
    }

    #[test]
    fn names_are_found_by_any_part_accents_and_case_ignored() {
        let (_d, s) = store();
        let r = s.root("user-documents", &VPath::root()).unwrap();
        s.replace_folder(
            r.id,
            &VPath::root(),
            &[file("Café Budget.ods"), dir("Uni"), file("a.txt")],
        )
        .unwrap();
        s.replace_folder(
            r.id,
            &vp("/Uni"),
            &[
                file("budget-2026.pdf"),
                file("x*y?.md"),
                file("xay1.md"),
                file("abc[1].txt"),
                file("abc1.txt"),
            ],
        )
        .unwrap();
        assert_eq!(
            found(&s, &[r.id], "budget"),
            vec!["Café Budget.ods", "Uni/budget-2026.pdf"]
        );
        assert_eq!(found(&s, &[r.id], "cafe"), vec!["Café Budget.ods"]);
        assert_eq!(found(&s, &[r.id], "UN"), vec!["Uni"], "short needles scan");
        assert_eq!(
            found(&s, &[r.id], "x*y?"),
            vec!["Uni/x*y?.md"],
            "glob characters are literal"
        );
        assert_eq!(
            found(&s, &[r.id], "c[1]"),
            vec!["Uni/abc[1].txt"],
            "brackets are literal"
        );
        assert!(found(&s, &[], "budget").is_empty());
        assert!(s.integrity_ok());
    }

    #[test]
    fn replacing_a_folder_keeps_only_what_it_lists_and_drops_vanished_subtrees() {
        let (_d, s) = store();
        let r = s.root("l", &VPath::root()).unwrap();
        s.replace_folder(
            r.id,
            &VPath::root(),
            &[dir("Old"), file("keep.txt"), file("gone.txt")],
        )
        .unwrap();
        s.replace_folder(r.id, &vp("/Old"), &[dir("Deep"), file("old-note.txt")])
            .unwrap();
        s.replace_folder(r.id, &vp("/Old/Deep"), &[file("deep-note.txt")])
            .unwrap();
        s.replace_folder(r.id, &vp("/Older"), &[file("older-note.txt")])
            .unwrap();
        s.replace_folder(r.id, &VPath::root(), &[file("keep.txt"), file("new.txt")])
            .unwrap();
        assert_eq!(
            found(&s, &[r.id], "note"),
            vec!["Older/older-note.txt"],
            "a sibling with a longer name survives"
        );
        assert_eq!(
            found(&s, &[r.id], ".txt"),
            vec!["Older/older-note.txt", "keep.txt", "new.txt"]
        );
        let kids: Vec<String> = s
            .children(r.id, &VPath::root())
            .unwrap()
            .iter()
            .map(|e| e.display_name())
            .collect();
        assert_eq!(kids.len(), 2);
        assert!(s.integrity_ok());
    }

    #[test]
    fn a_folder_turned_file_loses_its_subtree() {
        let (_d, s) = store();
        let r = s.root("l", &VPath::root()).unwrap();
        s.replace_folder(r.id, &VPath::root(), &[dir("x")]).unwrap();
        s.replace_folder(r.id, &vp("/x"), &[file("inside.txt")]).unwrap();
        s.replace_folder(r.id, &VPath::root(), &[file("x")]).unwrap();
        assert!(found(&s, &[r.id], "inside").is_empty());
        assert_eq!(found(&s, &[r.id], "x"), vec!["x"]);
    }

    #[test]
    fn roots_are_kept_apart_and_removable() {
        let (_d, s) = store();
        let a = s.root("a", &VPath::root()).unwrap();
        let b = s.root("b", &VPath::root()).unwrap();
        assert_eq!(
            s.root("a", &VPath::root()).unwrap().id,
            a.id,
            "the same root twice is one root"
        );
        s.replace_folder(a.id, &VPath::root(), &[file("note-a")]).unwrap();
        s.replace_folder(b.id, &VPath::root(), &[file("note-b")]).unwrap();
        assert_eq!(found(&s, &[a.id], "note"), vec!["note-a"]);
        assert_eq!(found(&s, &[a.id, b.id], "note"), vec!["note-a", "note-b"]);
        s.remove_root(a.id).unwrap();
        assert_eq!(found(&s, &[a.id, b.id], "note"), vec!["note-b"]);
        assert_eq!(s.roots().unwrap().len(), 1);
    }

    #[test]
    fn passes_track_checked_folders_and_only_restart_once_done() {
        let (_d, s) = store();
        let r = s.root("l", &VPath::root()).unwrap();
        assert_eq!(
            s.begin_pass(r.id).unwrap(),
            1,
            "nothing checked yet: the first pass goes on"
        );
        s.replace_folder(r.id, &VPath::root(), &[dir("a"), dir("b")])
            .unwrap();
        s.mark_checked(r.id, &VPath::root(), &VPath::root(), 1, Some(b"v1"))
            .unwrap();
        let todo: Vec<String> = s
            .pending_folders(r.id, 1, 10)
            .unwrap()
            .iter()
            .map(|f| f.path.display())
            .collect();
        assert_eq!(todo, vec!["a", "b"]);
        s.mark_checked(r.id, &VPath::root(), &vp("/a"), 1, Some(b"va"))
            .unwrap();
        assert_eq!(s.begin_pass(r.id).unwrap(), 1, "an unfinished pass resumes");
        let todo = s.pending_folders(r.id, 1, 10).unwrap();
        assert_eq!(todo.len(), 1);
        assert_eq!(todo[0].path.display(), "b");
        s.mark_checked(r.id, &VPath::root(), &vp("/b"), 1, None).unwrap();
        assert_eq!(
            s.begin_pass(r.id).unwrap(),
            2,
            "a finished pass starts the next one"
        );
        let (root, checked) = s.root_folder(r.id).unwrap();
        assert_eq!((root.version.as_deref(), checked), (Some(&b"v1"[..]), 1));
        let again: Vec<(String, Option<Vec<u8>>)> = s
            .pending_folders(r.id, 2, 10)
            .unwrap()
            .into_iter()
            .map(|f| (f.path.display(), f.version))
            .collect();
        assert_eq!(
            again,
            vec![("a".to_owned(), Some(b"va".to_vec())), ("b".to_owned(), None)]
        );
    }

    #[test]
    fn a_damaged_or_older_index_is_rebuilt_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("names.db");
        {
            let s = IndexStore::open(&path).unwrap();
            let r = s.root("l", &VPath::root()).unwrap();
            s.replace_folder(r.id, &VPath::root(), &[file("kept")]).unwrap();
        }
        let s = IndexStore::open(&path).unwrap();
        assert_eq!(s.roots().unwrap().len(), 1, "a sound index is reopened as it is");
        drop(s);
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        assert!(
            IndexStore::open(&path).unwrap().roots().unwrap().is_empty(),
            "another schema is rebuilt"
        );
        let s = IndexStore::open(&path).unwrap();
        let r = s.root("l", &VPath::root()).unwrap();
        s.replace_folder(r.id, &VPath::root(), &[file("kept")]).unwrap();
        drop(s);
        std::fs::write(&path, b"this is not a database at all, not even close........").unwrap();
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(dir.path().join(format!("names.db{suffix}")));
        }
        let s = IndexStore::open(&path).unwrap();
        assert!(s.roots().unwrap().is_empty(), "garbage is replaced");
        let r = s.root("l", &VPath::root()).unwrap();
        s.replace_folder(r.id, &VPath::root(), &[file("fresh")]).unwrap();
        assert_eq!(found(&s, &[r.id], "fresh"), vec!["fresh"]);
    }

    #[test]
    fn a_full_disk_reads_as_no_space() {
        let (_d, s) = store();
        let r = s.root("l", &VPath::root()).unwrap();
        s.lock().pragma_update(None, "max_page_count", 40).unwrap();
        let names: Vec<Entry> = (0..20_000)
            .map(|i| file(&format!("file-{i:05}-with-a-long-name.txt")))
            .collect();
        let err = s.replace_folder(r.id, &VPath::root(), &names).unwrap_err();
        assert_eq!(err.kind, ErrorKind::NoSpace);
        assert!(
            found(&s, &[r.id], "file-00001").is_empty(),
            "a failed folder leaves nothing half written"
        );
    }
}
