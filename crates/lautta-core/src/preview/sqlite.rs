// SPDX-License-Identifier: LGPL-2.1-or-later
//! SQLite viewer model (SPEC PRV-4): tables, columns and the first rows of a
//! *local* database file, strictly read-only.
//!
//! The file is opened with `SQLITE_OPEN_READ_ONLY` and the `immutable=1` URI
//! parameter, so SQLite never creates `-wal`/`-shm`/journal files next to it
//! (the viewer must not modify what it shows; changes still sitting in a WAL
//! are not visible). No caller-supplied SQL is ever executed: a table name
//! must be one found in `sqlite_master`, and is quoted as an identifier;
//! limit and offset are bound parameters.

use crate::error::{Error, ErrorKind, Result};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use std::io::Read;
use std::path::Path;

/// Row counts above this are reported as "at least".
pub const COUNT_CAP: u64 = 1_000_000;
/// Rows per page at most.
pub const MAX_PAGE_ROWS: u32 = 1000;
/// Text cells are cut after this many characters.
pub const MAX_CELL_CHARS: usize = 500;

const MAGIC: &[u8; 16] = b"SQLite format 3\0";
/// Everything but unreserved characters and `/` is escaped in the URI path.
const URI_PATH: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'/')
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnInfo {
    pub name: String,
    pub decl_type: String,
    pub not_null: bool,
    pub primary_key: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableKind {
    Table,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableInfo {
    pub name: String,
    pub kind: TableKind,
    /// Number of rows, capped at [`COUNT_CAP`].
    pub rows: u64,
    /// False when `rows` hit the cap (or a view could not be counted).
    pub rows_exact: bool,
    pub columns: Vec<ColumnInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub columns: Vec<String>,
    /// Cells rendered as text.
    pub rows: Vec<Vec<String>>,
    pub offset: u64,
    pub has_more: bool,
}

pub struct SqlitePreview {
    conn: Connection,
}

fn db_error(err: rusqlite::Error) -> Error {
    Error::new(ErrorKind::InvalidArgument, format!("sqlite: {err}"))
}

/// Quotes `name` as an SQL identifier.
pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn check_magic(path: &Path) -> Result<()> {
    let mut head = [0u8; 16];
    let mut file = std::fs::File::open(path)?;
    let n = file.read(&mut head)?;
    if n < head.len() || &head != MAGIC {
        return Err(Error::new(ErrorKind::Unsupported, "not an SQLite database"));
    }
    Ok(())
}

/// Renders one cell: NULL, numbers, text (cut), blobs as `<blob N bytes>`.
pub fn render_value(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "NULL".to_owned(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => f.to_string(),
        ValueRef::Text(t) => cut(&String::from_utf8_lossy(t)),
        ValueRef::Blob(b) => format!("<blob {} bytes>", b.len()),
    }
}

fn cut(text: &str) -> String {
    match text.char_indices().nth(MAX_CELL_CHARS) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

impl SqlitePreview {
    /// Opens a local database file read-only.
    pub fn open(path: &Path) -> Result<SqlitePreview> {
        check_magic(path)?;
        let text = path.to_string_lossy();
        let uri = format!("file:{}?immutable=1", utf8_percent_encode(&text, URI_PATH));
        let flags =
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(uri, flags).map_err(db_error)?;
        conn.execute_batch("PRAGMA query_only = ON; PRAGMA trusted_schema = OFF;")
            .map_err(db_error)?;
        Ok(SqlitePreview { conn })
    }

    /// Names and kinds from the schema, without counting rows.
    fn schema(&self) -> Result<Vec<(String, TableKind)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT name, type FROM sqlite_master WHERE type IN ('table', 'view') \
                 AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name",
            )
            .map_err(db_error)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(db_error)?;
        let mut out = Vec::new();
        for row in rows {
            let (name, kind) = row.map_err(db_error)?;
            let kind = if kind == "view" {
                TableKind::View
            } else {
                TableKind::Table
            };
            out.push((name, kind));
        }
        Ok(out)
    }

    /// Tables and views with row counts and columns.
    pub fn tables(&self) -> Result<Vec<TableInfo>> {
        let mut out = Vec::new();
        for (name, kind) in self.schema()? {
            let columns = self.columns(&name)?;
            let (rows, rows_exact) = self.count(&name).unwrap_or((0, false));
            out.push(TableInfo {
                name,
                kind,
                rows,
                rows_exact,
                columns,
            });
        }
        Ok(out)
    }

    fn columns(&self, table: &str) -> Result<Vec<ColumnInfo>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, type, \"notnull\", pk FROM pragma_table_info(?1)")
            .map_err(db_error)?;
        let rows = stmt
            .query_map([table], |r| {
                Ok(ColumnInfo {
                    name: r.get(0)?,
                    decl_type: r.get(1)?,
                    not_null: r.get::<_, i64>(2)? != 0,
                    primary_key: r.get::<_, i64>(3)? != 0,
                })
            })
            .map_err(db_error)?;
        rows.map(|r| r.map_err(db_error)).collect()
    }

    fn count(&self, table: &str) -> Result<(u64, bool)> {
        let sql = format!(
            "SELECT COUNT(*) FROM (SELECT 1 FROM {} LIMIT {})",
            quote_identifier(table),
            COUNT_CAP + 1
        );
        let n: i64 = self.conn.query_row(&sql, [], |r| r.get(0)).map_err(db_error)?;
        let n = u64::try_from(n).unwrap_or(0);
        Ok((n.min(COUNT_CAP), n <= COUNT_CAP))
    }

    /// `limit` rows of `table` from `offset` (limit is clamped to
    /// [`MAX_PAGE_ROWS`]). `table` must name an existing table or view.
    pub fn page(&self, table: &str, offset: u64, limit: u32) -> Result<Page> {
        if !self.schema()?.iter().any(|(n, _)| n == table) {
            return Err(Error::new(ErrorKind::NotFound, "no such table"));
        }
        let limit = limit.clamp(1, MAX_PAGE_ROWS);
        let sql = format!("SELECT * FROM {} LIMIT ?1 OFFSET ?2", quote_identifier(table));
        let mut stmt = self.conn.prepare(&sql).map_err(db_error)?;
        let columns: Vec<String> = stmt.column_names().into_iter().map(str::to_owned).collect();
        let width = columns.len();
        let offset_param = i64::try_from(offset).unwrap_or(i64::MAX);
        let mut rows = stmt
            .query([i64::from(limit) + 1, offset_param])
            .map_err(db_error)?;
        let mut out: Vec<Vec<String>> = Vec::new();
        while let Some(row) = rows.next().map_err(db_error)? {
            let mut cells = Vec::with_capacity(width);
            for i in 0..width {
                cells.push(render_value(row.get_ref(i).map_err(db_error)?));
            }
            out.push(cells);
        }
        let has_more = out.len() > limit as usize;
        out.truncate(limit as usize);
        Ok(Page {
            columns,
            rows: out,
            offset,
            has_more,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_db(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT NOT NULL, score REAL, photo BLOB, note TEXT);
             INSERT INTO people VALUES (1, 'Ada', 9.5, x'0102030405', NULL);
             INSERT INTO people VALUES (2, 'Bob', 7, NULL, 'hi');
             INSERT INTO people VALUES (3, 'Cy', 1.25, x'', 'x');
             CREATE TABLE \"we\"\"ird name\" (a);
             INSERT INTO \"we\"\"ird name\" VALUES ('quoted');
             CREATE VIEW names AS SELECT name FROM people;
             CREATE TABLE empty (z);",
        )
        .unwrap();
        path
    }

    #[test]
    fn lists_tables_views_counts_and_columns() {
        let tmp = tempfile::tempdir().unwrap();
        let db = SqlitePreview::open(&make_db(tmp.path(), "a.db")).unwrap();
        let tables = db.tables().unwrap();
        let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["empty", "names", "people", "we\"ird name"]);
        let people = tables.iter().find(|t| t.name == "people").unwrap();
        assert_eq!(
            (people.kind, people.rows, people.rows_exact),
            (TableKind::Table, 3, true)
        );
        let cols: Vec<(&str, &str, bool, bool)> = people
            .columns
            .iter()
            .map(|c| (c.name.as_str(), c.decl_type.as_str(), c.not_null, c.primary_key))
            .collect();
        assert_eq!(
            cols,
            [
                ("id", "INTEGER", false, true),
                ("name", "TEXT", true, false),
                ("score", "REAL", false, false),
                ("photo", "BLOB", false, false),
                ("note", "TEXT", false, false)
            ]
        );
        assert_eq!(
            tables.iter().find(|t| t.name == "names").unwrap().kind,
            TableKind::View
        );
        assert_eq!(tables.iter().find(|t| t.name == "empty").unwrap().rows, 0);
    }

    #[test]
    fn pages_render_values_as_text() {
        let tmp = tempfile::tempdir().unwrap();
        let db = SqlitePreview::open(&make_db(tmp.path(), "a.db")).unwrap();
        let page = db.page("people", 0, 2).unwrap();
        assert_eq!(page.columns, ["id", "name", "score", "photo", "note"]);
        assert_eq!(page.rows[0], ["1", "Ada", "9.5", "<blob 5 bytes>", "NULL"]);
        assert_eq!(page.rows[1], ["2", "Bob", "7", "NULL", "hi"]);
        assert!(page.has_more);
        let rest = db.page("people", 2, 2).unwrap();
        assert_eq!(rest.rows, [["3", "Cy", "1.25", "<blob 0 bytes>", "x"]]);
        assert!(!rest.has_more);
        assert_eq!(rest.offset, 2);
        assert!(db.page("people", 99, 5).unwrap().rows.is_empty());
    }

    #[test]
    fn page_limit_is_clamped() {
        let tmp = tempfile::tempdir().unwrap();
        let db = SqlitePreview::open(&make_db(tmp.path(), "a.db")).unwrap();
        assert_eq!(db.page("people", 0, 0).unwrap().rows.len(), 1);
        assert_eq!(db.page("people", 0, u32::MAX).unwrap().rows.len(), 3);
    }

    #[test]
    fn table_names_are_validated_and_quoted() {
        let tmp = tempfile::tempdir().unwrap();
        let db = SqlitePreview::open(&make_db(tmp.path(), "a.db")).unwrap();
        for evil in [
            "people; DROP TABLE people",
            "people\" --",
            "nonexistent",
            "sqlite_master",
            "",
        ] {
            assert_eq!(
                db.page(evil, 0, 5).err().map(|e| e.kind),
                Some(ErrorKind::NotFound),
                "{evil}"
            );
        }
        assert_eq!(db.page("we\"ird name", 0, 5).unwrap().rows, [["quoted"]]);
        assert_eq!(db.page("names", 0, 1).unwrap().rows, [["Ada"]]);
        assert_eq!(
            db.tables()
                .unwrap()
                .iter()
                .find(|t| t.name == "people")
                .unwrap()
                .rows,
            3
        );
    }

    #[test]
    fn opening_creates_no_side_files_and_does_not_modify_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = make_db(tmp.path(), "a.db");
        let before = std::fs::read(&path).unwrap();
        let listing = |d: &Path| {
            let mut v: Vec<String> = std::fs::read_dir(d)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            v.sort();
            v
        };
        let files = listing(tmp.path());
        {
            let db = SqlitePreview::open(&path).unwrap();
            db.tables().unwrap();
            db.page("people", 0, 10).unwrap();
        }
        assert_eq!(listing(tmp.path()), files);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn the_connection_cannot_write() {
        let tmp = tempfile::tempdir().unwrap();
        let db = SqlitePreview::open(&make_db(tmp.path(), "a.db")).unwrap();
        assert!(db.conn.execute("DELETE FROM people", []).is_err());
        assert!(db.conn.execute("CREATE TABLE x (a)", []).is_err());
    }

    #[test]
    fn paths_with_uri_special_characters_open() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("we?ird #dir%20");
        std::fs::create_dir(&dir).unwrap();
        let path = make_db(&dir, "a b?.db");
        assert_eq!(SqlitePreview::open(&path).unwrap().tables().unwrap().len(), 4);
    }

    #[test]
    fn non_database_files_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let txt = tmp.path().join("t.db");
        std::fs::write(&txt, b"this is not a database at all").unwrap();
        assert_eq!(
            SqlitePreview::open(&txt).err().map(|e| e.kind),
            Some(ErrorKind::Unsupported)
        );
        let short = tmp.path().join("s.db");
        std::fs::write(&short, b"SQLite").unwrap();
        assert!(SqlitePreview::open(&short).is_err());
        assert_eq!(
            SqlitePreview::open(&tmp.path().join("missing.db"))
                .err()
                .map(|e| e.kind),
            Some(ErrorKind::NotFound)
        );
    }

    #[test]
    fn long_text_is_cut_and_values_render() {
        assert_eq!(render_value(ValueRef::Integer(-4)), "-4");
        assert_eq!(render_value(ValueRef::Blob(&[1, 2])), "<blob 2 bytes>");
        let long = "é".repeat(MAX_CELL_CHARS + 10);
        let cell = render_value(ValueRef::Text(long.as_bytes()));
        assert_eq!(cell.chars().count(), MAX_CELL_CHARS + 1);
        assert!(cell.ends_with('…'));
        assert_eq!(render_value(ValueRef::Text(b"\xff")), "\u{FFFD}");
        assert_eq!(quote_identifier("a\"b"), "\"a\"\"b\"");
    }
}
