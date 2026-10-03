// SPDX-License-Identifier: LGPL-2.1-or-later
//! Typed signals of the bridge. Consumers read them from one ordered stream
//! (`Connection::signals`), because per-signal proxy streams may be observed
//! out of order relative to each other (`ListBatch` before `ListDone`).

use crate::errors::BridgeError;
use crate::wire::{WireEntry, WireNearby, WireWalkItem, INTERFACE};
use std::collections::HashMap;
use zbus::message::Type as MessageType;
use zbus::zvariant::OwnedValue;
use zbus::Message;

#[derive(Debug)]
pub enum Signal {
    ConsentChanged(String),
    LocationsChanged,
    NearbyChanged(Vec<WireNearby>),
    ListBatch {
        req: u32,
        entries: Vec<WireEntry>,
    },
    ListDone {
        req: u32,
        error: String,
        message: String,
    },
    JobProgress {
        job: u32,
        done: i64,
        total: i64,
    },
    WalkBatch {
        job: u32,
        entries: Vec<WireWalkItem>,
    },
    JobFinished {
        job: u32,
        error: String,
        message: String,
        extra: HashMap<String, OwnedValue>,
    },
    Question {
        id: String,
        kind: String,
        details: HashMap<String, OwnedValue>,
    },
}

fn body<T>(msg: &Message) -> Result<T, BridgeError>
where
    T: serde::de::DeserializeOwned + zbus::zvariant::Type,
{
    msg.body()
        .deserialize::<T>()
        .map_err(|e| BridgeError::Protocol(e.to_string()))
}

/// Decodes a message into a [`Signal`]. `Ok(None)` for messages that are not
/// signals of `org.netvfs.Bridge1` (and for signals of a newer protocol that
/// this client does not know, which are ignored).
pub fn decode_signal(msg: &Message) -> Result<Option<Signal>, BridgeError> {
    let header = msg.header();
    if header.primary().msg_type() != MessageType::Signal {
        return Ok(None);
    }
    if header.interface().map(|i| i.as_str()) != Some(INTERFACE) {
        return Ok(None);
    }
    let Some(member) = header.member() else {
        return Ok(None);
    };
    decode_member(member.as_str(), msg)
}

fn decode_member(member: &str, msg: &Message) -> Result<Option<Signal>, BridgeError> {
    Ok(Some(match member {
        "ConsentChanged" => Signal::ConsentChanged(body::<(String,)>(msg)?.0),
        "LocationsChanged" => Signal::LocationsChanged,
        "NearbyChanged" => Signal::NearbyChanged(body::<(Vec<WireNearby>,)>(msg)?.0),
        "ListBatch" => {
            let (req, entries) = body::<(u32, Vec<WireEntry>)>(msg)?;
            Signal::ListBatch { req, entries }
        }
        "ListDone" => {
            let (req, error, message) = body::<(u32, String, String)>(msg)?;
            Signal::ListDone { req, error, message }
        }
        "JobProgress" => {
            let (job, done, total) = body::<(u32, i64, i64)>(msg)?;
            Signal::JobProgress { job, done, total }
        }
        "WalkBatch" => {
            let (job, entries) = body::<(u32, Vec<WireWalkItem>)>(msg)?;
            Signal::WalkBatch { job, entries }
        }
        "JobFinished" => {
            let (job, error, message, extra) =
                body::<(u32, String, String, HashMap<String, OwnedValue>)>(msg)?;
            Signal::JobFinished {
                job,
                error,
                message,
                extra,
            }
        }
        "Question" => {
            let (id, kind, details) = body::<(String, String, HashMap<String, OwnedValue>)>(msg)?;
            Signal::Question { id, kind, details }
        }
        _ => return Ok(None),
    }))
}
