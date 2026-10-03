// SPDX-License-Identifier: LGPL-2.1-or-later
//! Transfers: queue, scheduler, engine, progress (SPEC §11).
//!
//! - [`model`]: states and the transition table (XFR-1)
//! - [`store`]: persistence and retention (XFR-10, XFR-11)
//! - [`scheduler`]: pure scheduling decisions (XFR-2)
//! - [`conflict`]: run-time conflict decisions (OPS-2)
//! - [`exec`]: how one item moves its bytes (XFR-3, XFR-4, XFR-12, NVB-9, NVB-11)
//! - [`engine`]: runs everything with tokio (XFR-5, XFR-9, NVB-12)
//! - [`progress`]: rate and ETA (XFR-8)

pub mod clock;
pub mod conflict;
pub mod engine;
pub mod exec;
pub mod model;
pub mod progress;
pub mod scheduler;
pub mod store;

use crate::error::Result;
use crate::uri::Uri;
use async_trait::async_trait;

pub use clock::{Clock, ManualClock, SystemClock};
pub use engine::{Engine, EngineConfig, EngineDeps};
pub use model::{
    ItemState, Transfer, TransferId, TransferItem, TransferOptions, TransferState, TransferSummary,
    WaitReason,
};
pub use scheduler::Scheduler;
pub use store::Store;

/// Moves a local item to *Recently deleted* (OPS-8). Supplied by the caller
/// (`trash.rs`) so the engine stays free of that policy.
#[async_trait]
pub trait Trasher: Send + Sync {
    /// Returns `true` when the item (and everything below it) was trashed and
    /// `false` when this location does not trash, so the engine deletes
    /// permanently.
    async fn trash(&self, uri: &Uri) -> Result<bool>;
}

/// What the UI hears about (INT-2, INT-3, §15.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEvent {
    Added(TransferId),
    /// State, counters or order changed; re-read the summary.
    Changed(TransferId),
    /// Aggregate progress, at most 4 times a second per transfer (NVB-10).
    Progress {
        id: TransferId,
        bytes_done: u64,
        rate: u64,
        eta_secs: Option<u64>,
    },
    Finished(TransferId),
    /// A conflict in item `item` needs an answer.
    NeedsAnswer {
        id: TransferId,
        item: u32,
    },
    /// Whether any transfer is active: drives `KeepAlive` (XFR-5).
    Busy(bool),
}

#[cfg(test)]
pub(crate) mod testutil {
    use crate::entry::Kind;
    use crate::ops::{OperationKind, Plan, PlanItem, PlanTotals};
    use crate::uri::Uri;
    use crate::vpath::VPath;

    pub(crate) fn plan_item(src: &str, dst: &str, kind: Kind, size: u64) -> PlanItem {
        PlanItem {
            src: Uri::new("a", VPath::parse(src.as_bytes()).unwrap()),
            dst: Uri::new("b", VPath::parse(dst.as_bytes()).unwrap()),
            kind,
            size: Some(size),
            mtime_ms: Some(1000),
            mode: None,
            link_target: None,
            conflict: None,
            proposed_name: None,
            resolution: None,
        }
    }

    pub(crate) fn plan(kind: OperationKind, items: Vec<PlanItem>) -> Plan {
        Plan {
            kind,
            destination: Uri::root("b"),
            items,
            totals: PlanTotals::default(),
            needs_summary: false,
        }
    }
}
