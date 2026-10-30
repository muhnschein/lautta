// SPDX-License-Identifier: LGPL-2.1-or-later
//! File operations: planning, conflicts, names, bulk rename. Types shared by the planner and the
//! transfer engine.

pub mod conflict;
pub mod names;
pub mod plan;

use crate::entry::Kind;
use crate::uri::Uri;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperationKind {
    Copy,
    Move,
    Delete,
    Compress,
    Extract,
}

/// The default is never `Replace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConflictChoice {
    Replace,
    Skip,
    KeepBoth,
    Merge,
    ReplaceIfNewer,
    Resume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub dst_is_dir: bool,
    pub src_is_dir: bool,
    pub src_size: Option<u64>,
    pub dst_size: Option<u64>,
    pub src_mtime_ms: Option<i64>,
    pub dst_mtime_ms: Option<i64>,
    pub resumable: bool,
    pub choices: Vec<ConflictChoice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanItem {
    #[serde(with = "uri_serde")]
    pub src: Uri,
    #[serde(with = "uri_serde")]
    pub dst: Uri,
    #[serde(with = "kind_serde")]
    pub kind: Kind,
    pub size: Option<u64>,
    pub mtime_ms: Option<i64>,
    pub mode: Option<u32>,
    pub link_target: Option<Vec<u8>>,
    pub conflict: Option<Conflict>,
    pub proposed_name: Option<Vec<u8>>,
    pub resolution: Option<ConflictChoice>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanTotals {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub conflicts: u64,
    pub renamed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub kind: OperationKind,
    #[serde(with = "uri_serde")]
    pub destination: Uri,
    pub items: Vec<PlanItem>,
    pub totals: PlanTotals,
    /// Over 1 000 items or 1 GB (configurable) a summary sheet is shown.
    pub needs_summary: bool,
}

pub mod uri_serde {
    use crate::uri::Uri;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(u: &Uri, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&u.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Uri, D::Error> {
        let s = String::deserialize(d)?;
        Uri::parse(&s).map_err(serde::de::Error::custom)
    }
}

pub mod kind_serde {
    use crate::entry::Kind;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(k: &Kind, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(k.to_wire())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Kind, D::Error> {
        Ok(Kind::from_wire(u8::deserialize(d)?))
    }
}
