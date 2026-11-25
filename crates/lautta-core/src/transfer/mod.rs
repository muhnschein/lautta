// SPDX-License-Identifier: LGPL-2.1-or-later

pub mod clock;
pub mod conflict;
pub mod engine;
pub mod exec;
pub mod model;
pub mod progress;
pub mod scheduler;
pub mod store;
pub mod throttle;

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

#[async_trait]
pub trait Trasher: Send + Sync {
    async fn trash(&self, uri: &Uri) -> Result<bool>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEvent {
    Added(TransferId),
    Changed(TransferId),
    Progress {
        id: TransferId,
        bytes_done: u64,
        rate: u64,
        eta_secs: Option<u64>,
    },
    Finished(TransferId),
    NeedsAnswer {
        id: TransferId,
        item: u32,
    },
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
            hold: Default::default(),
        }
    }
}
