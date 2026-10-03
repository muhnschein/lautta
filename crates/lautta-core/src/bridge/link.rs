// SPDX-License-Identifier: LGPL-2.1-or-later
//! One live connection to the bridge: the proxy plus the routing of listing and
//! job signals to the requests that wait for them (NVB-7, NVB-10).

use super::convert::{entry_from_wire, finished_error, map_error};
use super::registry::Registry;
use crate::entry::Entry;
use crate::error::{Error, ErrorKind, Result};
use crate::provider::{Lane, ProgressSink};
use lautta_bridge_proto::{value_i64, Bridge1Proxy, BridgeError, Connection, ProxyResult, Signal, WireEntry};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Result of a finished job: the integers of `JobFinished.extra` (`bytes`,
/// `files`, `dirs`, `entries`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobOutcome {
    pub extra: HashMap<String, i64>,
}

impl JobOutcome {
    pub fn get(&self, key: &str) -> Option<u64> {
        self.extra.get(key).and_then(|v| u64::try_from(*v).ok())
    }
}

pub(crate) enum ListEvent {
    Batch(Vec<WireEntry>),
    Done(Option<Error>),
}

pub(crate) enum JobEvent {
    Progress { done: i64, total: i64 },
    Finished(Result<JobOutcome>),
}

/// Maps the result of a proxy call into the app's error model.
pub(crate) fn rpc<T>(r: ProxyResult<T>) -> Result<T> {
    r.map_err(|e| map_error(BridgeError::from(e)))
}

#[derive(Clone, Copy)]
enum Kind {
    List,
    Job,
}

pub struct Link {
    conn: Connection,
    lists: Registry<ListEvent>,
    jobs: Registry<JobEvent>,
}

impl Link {
    pub(crate) fn new(conn: Connection) -> Arc<Link> {
        Arc::new(Link {
            conn,
            lists: Registry::default(),
            jobs: Registry::default(),
        })
    }

    pub(crate) fn proxy(&self) -> &Bridge1Proxy<'static> {
        self.conn.proxy()
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Routes list and job signals; hands back the others.
    pub(crate) fn route(&self, signal: Signal) -> Option<Signal> {
        match signal {
            Signal::ListBatch { req, entries } => self.lists.push(req, ListEvent::Batch(entries)),
            Signal::ListDone { req, error, message } => {
                let err = finished_error(&error, &message, &HashMap::new());
                self.lists.push(req, ListEvent::Done(err));
            }
            Signal::JobProgress { job, done, total } => {
                self.jobs.push(job, JobEvent::Progress { done, total })
            }
            Signal::JobFinished {
                job,
                error,
                message,
                extra,
            } => {
                let result = match finished_error(&error, &message, &extra) {
                    Some(e) => Err(e),
                    None => Ok(JobOutcome {
                        extra: extra
                            .iter()
                            .filter_map(|(k, v)| value_i64(v).map(|n| (k.clone(), n)))
                            .collect(),
                    }),
                };
                self.jobs.push(job, JobEvent::Finished(result));
            }
            // Walk results are consumed by `Walk` users only, which this client has none of yet.
            Signal::WalkBatch { .. } => {}
            other => return Some(other),
        }
        None
    }

    /// The socket closed: everything waiting ends with `ConnectionLost`.
    pub(crate) fn close(&self) {
        self.lists.close();
        self.jobs.close();
    }

    fn forget(&self, kind: Kind, id: u32) {
        match kind {
            Kind::List => self.lists.forget(id),
            Kind::Job => self.jobs.forget(id),
        }
    }

    /// Asks the bridge to stop a request or job, without waiting for it.
    fn cancel_later(self: &Arc<Self>, id: u32) {
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            let link = self.clone();
            rt.spawn(async move {
                // The connection may be gone already, which cancels everything (XB-13).
                let _ = link.proxy().cancel(id).await;
            });
        }
    }

    /// Streams a listing into `out` until `ListDone` (NVB-7). Dropping the
    /// future cancels the request at the bridge.
    pub(crate) async fn list(
        self: &Arc<Self>,
        loc: &str,
        dir: &[u8],
        lane: Lane,
        out: &mpsc::Sender<Vec<Entry>>,
    ) -> Result<()> {
        let req = rpc(self.proxy().list(loc, dir, lane.wire(), 0).await)?;
        let mut rx = self.lists.subscribe(req);
        let mut guard = RequestGuard::new(self, Kind::List, req);
        while let Some(event) = rx.recv().await {
            match event {
                ListEvent::Batch(batch) => {
                    let entries = batch.iter().map(entry_from_wire).collect();
                    if out.send(entries).await.is_err() {
                        // The reader went away: stop the listing (the guard cancels).
                        return Err(Error::new(ErrorKind::Canceled, "listing no longer wanted"));
                    }
                }
                ListEvent::Done(None) => {
                    guard.finish();
                    return Ok(());
                }
                ListEvent::Done(Some(e)) => {
                    guard.finish();
                    return Err(e);
                }
            }
        }
        guard.finish();
        Err(lost())
    }

    /// Starts a job with `start` (which returns the job id) and waits for
    /// `JobFinished`, reporting `JobProgress` to `progress`. Dropping the
    /// future cancels the job at the bridge.
    pub(crate) async fn run_job<F>(self: &Arc<Self>, start: F, progress: &ProgressSink) -> Result<JobOutcome>
    where
        F: Future<Output = ProxyResult<u32>>,
    {
        let job = rpc(start.await)?;
        let mut rx = self.jobs.subscribe(job);
        let mut guard = RequestGuard::new(self, Kind::Job, job);
        while let Some(event) = rx.recv().await {
            match event {
                JobEvent::Progress { done, total } => {
                    progress(u64::try_from(done).unwrap_or(0), u64::try_from(total).ok());
                }
                JobEvent::Finished(result) => {
                    guard.finish();
                    return result;
                }
            }
        }
        guard.finish();
        Err(lost())
    }
}

pub(crate) fn lost() -> Error {
    Error::new(ErrorKind::ConnectionLost, "the connection to the bridge was lost")
}

/// Cancels the request at the bridge when its future is dropped early.
struct RequestGuard {
    link: Arc<Link>,
    kind: Kind,
    id: u32,
    done: bool,
}

impl RequestGuard {
    fn new(link: &Arc<Link>, kind: Kind, id: u32) -> RequestGuard {
        RequestGuard {
            link: link.clone(),
            kind,
            id,
            done: false,
        }
    }

    fn finish(&mut self) {
        self.done = true;
    }
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.link.forget(self.kind, self.id);
        if !self.done {
            self.link.cancel_later(self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::no_progress;
    use lautta_bridge_proto::fake::FakeBridge;
    use lautta_bridge_proto::OptsBuilder;

    async fn link() -> Arc<Link> {
        let conn = FakeBridge::new().connect().await.unwrap();
        Link::new(conn)
    }

    #[test]
    fn outcomes_expose_unsigned_counts() {
        let outcome = JobOutcome {
            extra: HashMap::from([("files".to_owned(), 2), ("neg".to_owned(), -1)]),
        };
        assert_eq!(outcome.get("files"), Some(2));
        assert_eq!(outcome.get("neg"), None);
        assert_eq!(outcome.get("missing"), None);
    }

    #[tokio::test]
    async fn listing_signals_go_to_their_request_even_when_early() {
        let link = link().await;
        let batch = vec![WireEntry::unknown(b"a", 1)];
        // The batch arrives before anyone has subscribed to request 4.
        assert!(link
            .route(Signal::ListBatch {
                req: 4,
                entries: batch
            })
            .is_none());
        let mut rx = link.lists.subscribe(4);
        assert!(link
            .route(Signal::ListDone {
                req: 4,
                error: "NotFound".into(),
                message: "gone".into(),
            })
            .is_none());
        assert!(matches!(rx.recv().await, Some(ListEvent::Batch(b)) if b.len() == 1));
        match rx.recv().await {
            Some(ListEvent::Done(Some(e))) => {
                assert_eq!((e.kind, e.message.as_str()), (ErrorKind::NotFound, "gone"))
            }
            other => panic!("unexpected event {}", other.is_some()),
        }
    }

    #[tokio::test]
    async fn job_signals_carry_progress_and_integer_extras() {
        let link = link().await;
        let mut rx = link.jobs.subscribe(9);
        link.route(Signal::JobProgress {
            job: 9,
            done: 5,
            total: -1,
        });
        let extra = OptsBuilder::new().int("bytes", 5).str("note", "x").build_owned();
        link.route(Signal::JobFinished {
            job: 9,
            error: String::new(),
            message: String::new(),
            extra,
        });
        assert!(matches!(
            rx.recv().await,
            Some(JobEvent::Progress { done: 5, total: -1 })
        ));
        match rx.recv().await {
            Some(JobEvent::Finished(Ok(outcome))) => {
                assert_eq!(outcome.get("bytes"), Some(5));
                assert!(!outcome.extra.contains_key("note"), "only integers are kept");
            }
            _ => panic!("expected a finished job"),
        }
    }

    #[tokio::test]
    async fn job_errors_keep_detail_and_retry_hint() {
        let link = link().await;
        let mut rx = link.jobs.subscribe(1);
        let extra = OptsBuilder::new()
            .str("detail", "d")
            .int("retryAfterMs", 700)
            .build_owned();
        link.route(Signal::JobFinished {
            job: 1,
            error: "RateLimited".into(),
            message: "m".into(),
            extra,
        });
        match rx.recv().await {
            Some(JobEvent::Finished(Err(e))) => {
                assert_eq!(e.kind, ErrorKind::RateLimited);
                assert_eq!((e.detail.as_deref(), e.retry_after_ms), (Some("d"), Some(700)));
            }
            _ => panic!("expected a failed job"),
        }
    }

    #[tokio::test]
    async fn other_signals_are_handed_back_and_walks_are_dropped() {
        let link = link().await;
        assert!(matches!(
            link.route(Signal::LocationsChanged),
            Some(Signal::LocationsChanged)
        ));
        assert!(link
            .route(Signal::WalkBatch {
                job: 1,
                entries: Vec::new()
            })
            .is_none());
    }

    #[tokio::test]
    async fn closing_ends_every_waiting_request_with_connection_lost() {
        let link = link().await;
        let mut lists = link.lists.subscribe(1);
        let mut jobs = link.jobs.subscribe(2);
        link.close();
        assert!(lists.recv().await.is_none());
        assert!(jobs.recv().await.is_none());
        let progress = no_progress();
        let e = link
            .run_job(async { ProxyResult::Ok(3u32) }, &progress)
            .await
            .unwrap_err();
        assert_eq!(e.kind, ErrorKind::ConnectionLost);
        assert_eq!(lost().kind, ErrorKind::ConnectionLost);
    }
}
