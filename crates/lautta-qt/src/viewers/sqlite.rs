// SPDX-License-Identifier: LGPL-2.1-or-later
//! `SqliteModel`: the tables of a local database file and the rows of one
//! of them, read-only, 50 rows at a time (PRV-4). Remote files are copied
//! first (OpenRemote), never opened from a remote location.

use super::{blocking_run_then, parse_uri, qstr};
use crate::json::to_json;
use crate::runtime::core;
use lautta_core::org::recents::RecentKind;
use lautta_core::preview::sqlite::{Page, SqlitePreview, TableInfo, TableKind};
use qmetaobject::prelude::*;
use qmetaobject::qml_register_type;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub fn register() {
    qml_register_type::<SqliteModel>(&crate::qml_uri(), 1, 0, &crate::cstr("SqliteModel"));
}

/// Rows per page (the board says "showing first 50").
pub const PAGE_ROWS: u32 = 50;
const ROLE_CELLS: i32 = 0x100;
const ROLE_NUMBER: i32 = 0x101;

type Shared = Arc<Mutex<SqlitePreview>>;

/// `[{name, kind, rows, rowsExact, columns:[{name, type, primaryKey}]}]`
pub fn tables_json(tables: &[TableInfo]) -> String {
    let list: Vec<_> = tables
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "kind": if t.kind == TableKind::View { "view" } else { "table" },
                "rows": t.rows,
                "rowsExact": t.rows_exact,
                "columns": t.columns.iter().map(|c| serde_json::json!({
                    "name": c.name, "type": c.decl_type, "primaryKey": c.primary_key,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    to_json(&list)
}

#[derive(QObject, Default)]
pub struct SqliteModel {
    base: qt_base_class!(trait QAbstractListModel),
    uri: qt_property!(QString; WRITE setUri NOTIFY uriChanged),
    uriChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY stateChanged),
    /// JSON list of the tables and views (see `tables_json`).
    tablesJson: qt_property!(QString; NOTIFY stateChanged),
    table: qt_property!(QString; NOTIFY stateChanged),
    /// JSON list of the column names of the shown table.
    columnsJson: qt_property!(QString; NOTIFY stateChanged),
    /// Rows of the table (capped at one million), -1 when unknown.
    totalRows: qt_property!(f64; NOTIFY stateChanged),
    totalRowsExact: qt_property!(bool; NOTIFY stateChanged),
    count: qt_property!(i32; NOTIFY stateChanged),
    hasMore: qt_property!(bool; NOTIFY stateChanged),
    errorKind: qt_property!(QString; NOTIFY stateChanged),
    errorMessage: qt_property!(QString; NOTIFY stateChanged),
    stateChanged: qt_signal!(),
    selectTable: qt_method!(fn(&mut self, name: QString)),
    loadMore: qt_method!(fn(&mut self)),

    db: Option<Shared>,
    tables: Vec<TableInfo>,
    rows: Vec<Vec<String>>,
    generation: u64,
}

impl SqliteModel {
    fn setUri(&mut self, value: QString) {
        if self.uri == value {
            return;
        }
        self.uri = value;
        self.uriChanged();
        self.open();
    }

    fn fail(&mut self, e: &lautta_core::Error) {
        self.loading = false;
        self.errorKind = qstr(e.kind.name());
        self.errorMessage = qstr(&e.message);
        self.stateChanged();
    }

    fn clear_rows(&mut self) {
        self.begin_reset_model();
        self.rows.clear();
        self.count = 0;
        self.hasMore = false;
        self.end_reset_model();
    }

    fn open(&mut self) {
        self.generation += 1;
        let gen = self.generation;
        self.clear_rows();
        self.db = None;
        self.tables.clear();
        self.errorKind = QString::default();
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            self.stateChanged();
            return;
        };
        let path = match core.sqlite_file(&uri) {
            Ok(p) => p,
            Err(e) => return self.fail(&e),
        };
        self.loading = true;
        self.stateChanged();
        blocking_run_then(
            self,
            move || {
                let db = SqlitePreview::open(&path)?;
                let tables = db.tables()?;
                Ok((db, tables))
            },
            move |me, res| {
                if me.generation != gen {
                    return;
                }
                match res {
                    Ok((db, tables)) => {
                        core.note_viewed(&uri, RecentKind::Previewed);
                        me.db = Some(Arc::new(Mutex::new(db)));
                        me.tablesJson = QString::from(tables_json(&tables).as_str());
                        let first = tables.first().map(|t| t.name.clone());
                        me.tables = tables;
                        me.loading = false;
                        me.stateChanged();
                        if let Some(name) = first {
                            me.selectTable(QString::from(name.as_str()));
                        }
                    }
                    Err(e) => me.fail(&e),
                }
            },
        );
    }

    fn selectTable(&mut self, name: QString) {
        let name = name.to_string();
        let Some(info) = self.tables.iter().find(|t| t.name == name).cloned() else {
            return;
        };
        self.generation += 1;
        self.clear_rows();
        self.table = qstr(&name);
        self.totalRows = if info.rows_exact || info.rows > 0 {
            info.rows as f64
        } else {
            -1.0
        };
        self.totalRowsExact = info.rows_exact;
        let columns: Vec<&str> = info.columns.iter().map(|c| c.name.as_str()).collect();
        self.columnsJson = QString::from(to_json(&columns).as_str());
        self.stateChanged();
        self.fetch(0);
    }

    fn loadMore(&mut self) {
        if self.hasMore && !self.loading {
            self.fetch(self.rows.len() as u64);
        }
    }

    fn fetch(&mut self, offset: u64) {
        let Some(db) = self.db.clone() else {
            return;
        };
        let gen = self.generation;
        let table = self.table.to_string();
        self.loading = true;
        self.stateChanged();
        blocking_run_then(
            self,
            move || {
                let guard = db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                guard.page(&table, offset, PAGE_ROWS)
            },
            move |me, res| {
                if me.generation == gen {
                    me.page_arrived(res);
                }
            },
        );
    }

    fn page_arrived(&mut self, res: lautta_core::Result<Page>) {
        self.loading = false;
        match res {
            Ok(page) => {
                let first = i32::try_from(self.rows.len()).unwrap_or(i32::MAX);
                let n = i32::try_from(page.rows.len()).unwrap_or(0);
                if n > 0 {
                    self.begin_insert_rows(first, first + n - 1);
                    self.rows.extend(page.rows);
                    self.end_insert_rows();
                }
                self.count = first + n;
                self.hasMore = page.has_more;
                self.stateChanged();
            }
            Err(e) => self.fail(&e),
        }
    }
}

impl QAbstractListModel for SqliteModel {
    fn row_count(&self) -> i32 {
        i32::try_from(self.rows.len()).unwrap_or(i32::MAX)
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Ok(row) = usize::try_from(index.row()) else {
            return QVariant::default();
        };
        let Some(cells) = self.rows.get(row) else {
            return QVariant::default();
        };
        match role {
            ROLE_CELLS => qstr(&to_json(cells)).into(),
            ROLE_NUMBER => (row as i32 + 1).into(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        let mut m = HashMap::new();
        m.insert(ROLE_CELLS, "cellsJson".into());
        m.insert(ROLE_NUMBER, "number".into());
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lautta_core::preview::sqlite::ColumnInfo;

    #[test]
    fn tables_become_json() {
        let t = TableInfo {
            name: "m".into(),
            kind: TableKind::View,
            rows: 3,
            rows_exact: false,
            columns: vec![ColumnInfo {
                name: "id".into(),
                decl_type: "INTEGER".into(),
                not_null: true,
                primary_key: true,
            }],
        };
        let v: serde_json::Value = serde_json::from_str(&tables_json(&[t])).unwrap();
        assert_eq!(v[0]["kind"], "view");
        assert_eq!(v[0]["rows"], 3);
        assert_eq!(v[0]["rowsExact"], false);
        assert_eq!(v[0]["columns"][0]["primaryKey"], true);
        assert_eq!(v[0]["columns"][0]["type"], "INTEGER");
    }
}
