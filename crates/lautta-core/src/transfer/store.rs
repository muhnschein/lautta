// SPDX-License-Identifier: LGPL-2.1-or-later
//! Persistence of the queue in `transfers` / `transfer_items` (XFR-11, DAT-2).
//! All methods are blocking; callers run them on blocking threads (ARC-6).

use super::model::{
    kind_from_name, kind_name, operation_from_name, operation_name, ItemExtra, ItemState, Transfer,
    TransferId, TransferItem, TransferOptions, TransferState, TransferSummary,
};
use crate::db::Db;
use crate::error::{Error, ErrorKind, Result};
use crate::ops::PlanItem;
use crate::uri::Uri;
use rusqlite::{params, Connection, Row};
use std::collections::HashMap;

/// Progress is written at most this often per transfer (XFR-11).
pub const PROGRESS_WRITE_INTERVAL_MS: i64 = 2000;
/// History is kept this long (XFR-10).
pub const DEFAULT_RETENTION_DAYS: u32 = 30;

const DAY_MS: i64 = 86_400_000;

pub fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

pub fn to_u64(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

/// Decides when a progress write is due.
#[derive(Debug, Clone)]
pub struct ProgressThrottle {
    interval_ms: i64,
    last: HashMap<TransferId, i64>,
}

impl Default for ProgressThrottle {
    fn default() -> Self {
        ProgressThrottle::new(PROGRESS_WRITE_INTERVAL_MS)
    }
}

impl ProgressThrottle {
    pub fn new(interval_ms: i64) -> ProgressThrottle {
        ProgressThrottle {
            interval_ms,
            last: HashMap::new(),
        }
    }

    /// True (and remembered) when at least the interval passed since the
    /// last write for this transfer.
    pub fn due(&mut self, id: TransferId, now_ms: i64) -> bool {
        match self.last.get(&id) {
            Some(prev) if now_ms - prev < self.interval_ms => false,
            _ => {
                self.last.insert(id, now_ms);
                true
            }
        }
    }

    pub fn forget(&mut self, id: TransferId) {
        self.last.remove(&id);
    }
}

#[derive(Clone)]
pub struct Store {
    db: Db,
}

impl Store {
    pub fn new(db: Db) -> Store {
        Store { db }
    }

    /// Inserts the transfer and its items; assigns `id` and `position`.
    pub fn insert(&self, t: &mut Transfer) -> Result<()> {
        let conn = self.db.lock();
        let tx = conn.unchecked_transaction()?;
        let position: i64 =
            tx.query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM transfers", [], |r| {
                r.get(0)
            })?;
        let (state, reason) = t.state.to_db();
        tx.execute(
            "INSERT INTO transfers(kind,title,state,dest_location,dest_uri,position,created_ms,\
             finished_ms,bytes_total,bytes_done,items_total,items_done,items_failed,\
             waiting_reason,options,error) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                operation_name(t.kind),
                t.title,
                state,
                t.dest.location,
                t.dest.to_string(),
                position,
                t.created_ms,
                t.finished_ms,
                to_i64(t.bytes_total),
                to_i64(t.bytes_done),
                to_i64(t.items_total),
                to_i64(t.items_done),
                to_i64(t.items_failed),
                reason,
                options_json(&t.options)?,
                t.error,
            ],
        )?;
        t.id = tx.last_insert_rowid();
        t.position = position;
        for item in &t.items {
            insert_item(&tx, t.id, item)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn update_summary(&self, s: &TransferSummary) -> Result<()> {
        write_summary(&self.db.lock(), s)
    }

    pub fn update_item(&self, transfer: TransferId, item: &TransferItem) -> Result<()> {
        write_item(&self.db.lock(), transfer, item)
    }

    /// Applies a batch of writes in one transaction.
    pub fn apply(&self, writes: &[Write]) -> Result<()> {
        let conn = self.db.lock();
        let tx = conn.unchecked_transaction()?;
        for w in writes {
            match w {
                Write::Summary(s) => write_summary(&tx, s)?,
                Write::Item(id, item) => write_item(&tx, *id, item)?,
                Write::Delete(id) => {
                    tx.execute("DELETE FROM transfers WHERE id=?", [id])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete(&self, id: TransferId) -> Result<()> {
        self.apply(&[Write::Delete(id)])
    }

    /// Loads everything in queue order. Items are loaded for unfinished and
    /// failed transfers (resume, *Retry failed*); history rows come without
    /// items.
    pub fn load_all(&self) -> Result<Vec<Transfer>> {
        let conn = self.db.lock();
        let mut transfers = {
            let mut stmt = conn.prepare(
                "SELECT id,kind,title,state,waiting_reason,dest_uri,position,created_ms,\
                 finished_ms,bytes_total,bytes_done,items_total,items_done,items_failed,\
                 options,error FROM transfers ORDER BY position, id",
            )?;
            let rows = stmt.query_map([], read_transfer)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let mut items = load_items(&conn)?;
        for t in &mut transfers {
            let mine = items.remove(&t.id).unwrap_or_default();
            let wanted = !matches!(t.state, TransferState::Completed | TransferState::Canceled);
            if wanted {
                t.items = mine;
            }
        }
        Ok(transfers)
    }

    /// Deletes finished transfers older than the retention (XFR-10).
    /// Returns how many were removed.
    pub fn purge_history(&self, now_ms: i64, retention_days: u32) -> Result<usize> {
        let cutoff = now_ms - i64::from(retention_days) * DAY_MS;
        let n = self.db.lock().execute(
            "DELETE FROM transfers WHERE state IN ('completed','failed','canceled') \
             AND COALESCE(finished_ms, created_ms) < ?",
            [cutoff],
        )?;
        Ok(n)
    }
}

/// One persisted change.
#[derive(Debug, Clone)]
pub enum Write {
    Summary(TransferSummary),
    Item(TransferId, TransferItem),
    Delete(TransferId),
}

fn options_json(o: &TransferOptions) -> Result<String> {
    serde_json::to_string(o).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))
}

fn write_summary(conn: &Connection, s: &TransferSummary) -> Result<()> {
    let (state, reason) = s.state.to_db();
    conn.execute(
        "UPDATE transfers SET title=?,state=?,waiting_reason=?,position=?,finished_ms=?,\
         bytes_total=?,bytes_done=?,items_total=?,items_done=?,items_failed=?,options=?,error=? \
         WHERE id=?",
        params![
            s.title,
            state,
            reason,
            s.position,
            s.finished_ms,
            to_i64(s.bytes_total),
            to_i64(s.bytes_done),
            to_i64(s.items_total),
            to_i64(s.items_done),
            to_i64(s.items_failed),
            options_json(&s.options)?,
            s.error,
            s.id,
        ],
    )?;
    Ok(())
}

fn extra_json(item: &TransferItem) -> Result<String> {
    serde_json::to_string(&item.extra()).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))
}

fn insert_item(conn: &Connection, transfer: TransferId, item: &TransferItem) -> Result<()> {
    conn.execute(
        "INSERT INTO transfer_items(transfer_id,seq,src_uri,dst_uri,kind,size,state,committed,\
         temp_name,conflict,attempts,error) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            transfer,
            item.seq,
            item.plan.src.to_string(),
            item.plan.dst.to_string(),
            kind_name(item.plan.kind),
            item.plan.size.map(to_i64),
            item.state.name(),
            to_i64(item.committed),
            item.temp_name,
            extra_json(item)?,
            item.attempts,
            item.error,
        ],
    )?;
    Ok(())
}

fn write_item(conn: &Connection, transfer: TransferId, item: &TransferItem) -> Result<()> {
    conn.execute(
        "UPDATE transfer_items SET dst_uri=?,state=?,committed=?,temp_name=?,conflict=?,\
         attempts=?,error=? WHERE transfer_id=? AND seq=?",
        params![
            item.plan.dst.to_string(),
            item.state.name(),
            to_i64(item.committed),
            item.temp_name,
            extra_json(item)?,
            item.attempts,
            item.error,
            transfer,
            item.seq,
        ],
    )?;
    Ok(())
}

fn read_transfer(r: &Row<'_>) -> rusqlite::Result<Transfer> {
    let state: String = r.get(3)?;
    let reason: Option<String> = r.get(4)?;
    let dest: String = r.get(5)?;
    let options: String = r.get(14)?;
    let bad = |msg: String| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(Error::new(ErrorKind::Internal, msg)),
        )
    };
    Ok(Transfer {
        id: r.get(0)?,
        kind: operation_from_name(&r.get::<_, String>(1)?),
        title: r.get(2)?,
        state: TransferState::from_db(&state, reason.as_deref()).map_err(|e| bad(e.to_string()))?,
        dest: Uri::parse(&dest).map_err(|e| bad(e.to_string()))?,
        position: r.get(6)?,
        created_ms: r.get(7)?,
        finished_ms: r.get(8)?,
        bytes_total: to_u64(r.get(9)?),
        bytes_done: to_u64(r.get(10)?),
        items_total: to_u64(r.get(11)?),
        items_done: to_u64(r.get(12)?),
        items_failed: to_u64(r.get(13)?),
        options: serde_json::from_str(&options).unwrap_or_default(),
        error: r.get(15)?,
        items: Vec::new(),
    })
}

fn load_items(conn: &Connection) -> Result<HashMap<TransferId, Vec<TransferItem>>> {
    let mut stmt = conn.prepare(
        "SELECT transfer_id,seq,src_uri,dst_uri,kind,size,state,committed,temp_name,conflict,\
         attempts,error FROM transfer_items ORDER BY transfer_id, seq",
    )?;
    let mut rows = stmt.query([])?;
    let mut out: HashMap<TransferId, Vec<TransferItem>> = HashMap::new();
    while let Some(r) = rows.next()? {
        let transfer: TransferId = r.get(0)?;
        out.entry(transfer).or_default().push(read_item(r)?);
    }
    Ok(out)
}

fn read_item(r: &Row<'_>) -> Result<TransferItem> {
    let src: String = r.get(2)?;
    let dst: String = r.get(3)?;
    let extra: ItemExtra =
        serde_json::from_str(&r.get::<_, String>(9).unwrap_or_default()).unwrap_or_default();
    Ok(TransferItem {
        seq: r.get(1)?,
        plan: PlanItem {
            src: Uri::parse(&src)?,
            dst: Uri::parse(&dst)?,
            kind: kind_from_name(&r.get::<_, String>(4)?),
            size: r.get::<_, Option<i64>>(5)?.map(to_u64),
            mtime_ms: extra.mtime_ms,
            mode: extra.mode,
            link_target: extra.link_target,
            conflict: extra.conflict,
            proposed_name: extra.proposed_name,
            resolution: extra.resolution,
        },
        state: ItemState::from_name(&r.get::<_, String>(6)?),
        committed: to_u64(r.get(7)?),
        temp_name: r.get(8)?,
        attempts: r.get(10)?,
        error: r.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Kind;
    use crate::ops::{ConflictChoice, OperationKind};
    use crate::transfer::testutil::{plan, plan_item};

    fn sample(db: &Db, title: &str) -> Transfer {
        let items = vec![
            plan_item("a/x", "x", Kind::File, 10),
            plan_item("a/d", "d", Kind::Dir, 0),
        ];
        let mut t = Transfer::from_plan(
            0,
            plan(OperationKind::Move, items),
            title,
            TransferOptions::default(),
            1000,
        );
        Store::new(db.clone()).insert(&mut t).unwrap();
        t
    }

    #[test]
    fn throttle_allows_one_write_per_interval() {
        let mut th = ProgressThrottle::default();
        assert!(th.due(1, 0));
        assert!(!th.due(1, 1999));
        assert!(th.due(1, 2000));
        assert!(!th.due(1, 3999));
        assert!(th.due(2, 3999), "other transfers are independent");
        assert!(!th.due(1, 3000));
        th.forget(1);
        assert!(th.due(1, 3000), "a forgotten transfer is due at once");
    }

    #[test]
    fn insert_assigns_ids_and_positions() {
        let db = Db::open_in_memory().unwrap();
        let a = sample(&db, "A");
        let b = sample(&db, "B");
        assert!(a.id > 0 && b.id > a.id);
        assert_eq!((a.position, b.position), (1, 2));
    }

    #[test]
    fn round_trip_with_items_and_extras() {
        let db = Db::open_in_memory().unwrap();
        let store = Store::new(db.clone());
        let mut t = sample(&db, "Move 2");
        t.items[0].plan.resolution = Some(ConflictChoice::KeepBoth);
        t.items[0].plan.link_target = Some(b"tgt".to_vec());
        t.items[0].state = ItemState::Failed;
        t.items[0].temp_name = Some(b".x.lautta-1.part".to_vec());
        t.items[0].committed = 4;
        t.items[0].attempts = 3;
        t.items[0].error = Some("boom".into());
        t.state = TransferState::Waiting(crate::transfer::WaitReason::Volume);
        t.options.verify_checksums = true;
        t.recount();
        t.error = Some("e".into());
        store.update_item(t.id, &t.items[0]).unwrap();
        store.update_summary(&t.summary()).unwrap();
        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], t);
    }

    #[test]
    fn history_rows_load_without_items_but_failed_keep_them() {
        let db = Db::open_in_memory().unwrap();
        let store = Store::new(db.clone());
        let mut done = sample(&db, "done");
        done.state = TransferState::Completed;
        store.update_summary(&done.summary()).unwrap();
        let mut failed = sample(&db, "failed");
        failed.state = TransferState::Failed;
        store.update_summary(&failed.summary()).unwrap();
        let loaded = store.load_all().unwrap();
        assert!(loaded[0].items.is_empty());
        assert_eq!(loaded[0].items_total, 2, "counters come from the row");
        assert_eq!(loaded[1].items.len(), 2);
    }

    #[test]
    fn purge_removes_only_old_finished_transfers() {
        let db = Db::open_in_memory().unwrap();
        let store = Store::new(db.clone());
        let now = 100 * DAY_MS;
        let mut ids = Vec::new();
        for (state, finished) in [
            (TransferState::Completed, now - 31 * DAY_MS),
            (TransferState::Completed, now - 29 * DAY_MS),
            (TransferState::Failed, now - 40 * DAY_MS),
            (TransferState::Canceled, now - 30 * DAY_MS - 1),
            (TransferState::Paused, now - 90 * DAY_MS),
        ] {
            let mut t = sample(&db, "t");
            t.state = state;
            t.finished_ms = Some(finished);
            store.update_summary(&t.summary()).unwrap();
            ids.push(t.id);
        }
        assert_eq!(store.purge_history(now, 30).unwrap(), 3);
        let left: Vec<TransferId> = store.load_all().unwrap().iter().map(|t| t.id).collect();
        assert_eq!(left, vec![ids[1], ids[4]]);
        let n: i64 = db
            .lock()
            .query_row("SELECT count(*) FROM transfer_items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 4, "items cascade with their transfer");
    }

    #[test]
    fn apply_batches_and_delete() {
        let db = Db::open_in_memory().unwrap();
        let store = Store::new(db.clone());
        let mut t = sample(&db, "t");
        t.state = TransferState::Running;
        t.items[1].state = ItemState::Done;
        store
            .apply(&[Write::Summary(t.summary()), Write::Item(t.id, t.items[1].clone())])
            .unwrap();
        let loaded = store.load_all().unwrap();
        assert_eq!(loaded[0].state, TransferState::Running);
        assert_eq!(loaded[0].items[1].state, ItemState::Done);
        store.delete(t.id).unwrap();
        assert!(store.load_all().unwrap().is_empty());
    }

    #[test]
    fn integer_conversion_saturates() {
        assert_eq!(to_i64(u64::MAX), i64::MAX);
        assert_eq!(to_u64(-5), 0);
        assert_eq!(to_u64(7), 7);
    }
}
