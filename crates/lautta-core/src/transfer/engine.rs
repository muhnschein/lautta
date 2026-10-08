// SPDX-License-Identifier: LGPL-2.1-or-later
//! The transfer engine: owns the queue, starts items the scheduler picks,
//! reacts to their results, persists every change (XFR-11) and publishes
//! events. All queue state lives behind one `std::sync::Mutex` that is never
//! held across an `.await`; item tasks are plain tokio tasks that can be
//! aborted (pause, cancel, waiting) because every state change they make is
//! one synchronous function (so an abort never lands halfway through).

use super::clock::{Clock, SystemClock};
use super::exec::{self, ItemEnv, ItemJob, Outcome};
use super::model::{ItemState, Transfer, TransferId, TransferItem, TransferState, TransferSummary};
use super::model::{TransferOptions, WaitReason};
use super::progress::RateEstimator;
use super::scheduler::{is_bridge_location, ItemRef, Scheduler};
use super::store::{ProgressThrottle, Store, Write, DEFAULT_RETENTION_DAYS};
use super::{TransferEvent, Trasher};
use crate::db::Db;
use crate::entry::Kind;
use crate::error::{Error, ErrorKind, Result};
use crate::ops::{Conflict, ConflictChoice, OperationKind, Plan, PlanItem};
use crate::provider::{ProgressSink, ProviderResolver};
use crate::uri::Uri;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::AbortHandle;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// First retry delay; doubled for each further retry (XFR-9: 1 s, 2 s, 4 s).
    pub backoff_base: Duration,
    /// Retries of a transient error before the item fails or the transfer waits.
    pub max_retries: u32,
    /// Where anonymous scratch files go (the app's cache folder).
    pub scratch_dir: PathBuf,
    /// History retention (XFR-10).
    pub retention_days: u32,
    /// Minimum spacing of `Progress` events per transfer (NVB-10: 4 Hz).
    pub progress_event_interval_ms: i64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            backoff_base: Duration::from_secs(1),
            max_retries: 3,
            scratch_dir: std::env::temp_dir(),
            retention_days: DEFAULT_RETENTION_DAYS,
            progress_event_interval_ms: 250,
        }
    }
}

/// What the engine needs from the rest of the app.
#[derive(Clone)]
pub struct EngineDeps {
    pub db: Db,
    pub resolver: Arc<dyn ProviderResolver>,
    pub trasher: Option<Arc<dyn Trasher>>,
    pub clock: Arc<dyn Clock>,
}

impl EngineDeps {
    pub fn new(db: Db, resolver: Arc<dyn ProviderResolver>) -> EngineDeps {
        EngineDeps {
            db,
            resolver,
            trasher: None,
            clock: Arc::new(SystemClock),
        }
    }
}

struct Live {
    t: Transfer,
    running: HashMap<u32, AbortHandle>,
    item_bytes: HashMap<u32, u64>,
    rate: RateEstimator,
    last_event_ms: i64,
    finalizer: Option<AbortHandle>,
    locations: HashSet<String>,
}

impl Live {
    fn new(t: Transfer) -> Live {
        let mut locations: HashSet<String> = t
            .items
            .iter()
            .flat_map(|i| [i.plan.src.location.clone(), i.plan.dst.location.clone()])
            .collect();
        locations.insert(t.dest.location.clone());
        Live {
            t,
            running: HashMap::new(),
            item_bytes: HashMap::new(),
            rate: RateEstimator::default(),
            last_event_ms: i64::MIN,
            finalizer: None,
            locations,
        }
    }

    fn live_bytes(&self) -> u64 {
        self.item_bytes.values().sum()
    }

    fn summary(&self) -> TransferSummary {
        let mut s = self.t.summary();
        s.bytes_done = (s.bytes_done + self.live_bytes()).min(s.bytes_total.max(s.bytes_done));
        s
    }

    fn pending_items(&self, op: OperationKind) -> Vec<(u32, bool)> {
        self.t
            .items
            .iter()
            .filter(|i| i.state == ItemState::Pending)
            .map(|i| (i.seq, is_barrier(op, i.plan.kind)))
            .collect()
    }
}

/// Folders, links and deletes run alone and in order; files run in parallel.
fn is_barrier(op: OperationKind, kind: Kind) -> bool {
    op == OperationKind::Delete || kind != Kind::File
}

#[derive(Default)]
struct State {
    live: BTreeMap<TransferId, Live>,
    sched: Scheduler,
    bridge_up: bool,
    network_up: bool,
    volumes_gone: HashSet<String>,
    throttle: ProgressThrottle,
    busy: bool,
}

enum Cmd {
    Write(Box<Write>),
    Flush(oneshot::Sender<()>),
}

struct Inner {
    deps: EngineDeps,
    cfg: EngineConfig,
    store: Store,
    state: Mutex<State>,
    events: broadcast::Sender<TransferEvent>,
    writer: mpsc::UnboundedSender<Cmd>,
    /// So engine calls from threads outside the runtime (the Qt side) can spawn.
    rt: tokio::runtime::Handle,
}

/// The public handle; cheap to clone.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

/// What an item task does after a result was recorded.
enum Next {
    Stop,
    Rerun,
    Retry(Duration),
    Fail(Error),
}

struct Prepared {
    env: ItemEnv,
    job: ItemJob,
}

fn wait_reason_for(kind: ErrorKind) -> Option<WaitReason> {
    match kind {
        ErrorKind::ConnectionLost | ErrorKind::BridgeUnavailable => Some(WaitReason::Bridge),
        ErrorKind::NetworkUnreachable => Some(WaitReason::Network),
        _ => None,
    }
}

fn not_found(id: TransferId) -> Error {
    Error::new(ErrorKind::NotFound, format!("no transfer {id}"))
}

fn bad_state(what: &str, state: TransferState) -> Error {
    Error::new(
        ErrorKind::InvalidArgument,
        format!("cannot {what} a transfer that is {state:?}"),
    )
}

fn spawn_writer(store: Store) -> mpsc::UnboundedSender<Cmd> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Cmd>();
    tokio::spawn(async move {
        while let Some(first) = rx.recv().await {
            let mut writes = Vec::new();
            let mut flushes = Vec::new();
            let mut take = |cmd: Cmd| match cmd {
                Cmd::Write(w) => writes.push(*w),
                Cmd::Flush(f) => flushes.push(f),
            };
            take(first);
            while let Ok(more) = rx.try_recv() {
                take(more);
            }
            let s = store.clone();
            let applied = tokio::task::spawn_blocking(move || s.apply(&writes)).await;
            match applied {
                Ok(Ok(())) => {}
                Ok(Err(e)) => log::warn!("could not save the transfer queue: {e}"),
                Err(e) => log::warn!("queue writer failed: {e}"),
            }
            for f in flushes {
                let _ = f.send(());
            }
        }
    });
    tx
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?
}

impl Engine {
    /// Creates the engine. Must be called inside a tokio runtime. Call
    /// [`Engine::load`] afterwards to bring back the persisted queue.
    pub fn new(deps: EngineDeps, cfg: EngineConfig) -> Engine {
        let store = Store::new(deps.db.clone());
        let (events, _) = broadcast::channel(1024);
        let state = State {
            bridge_up: true,
            network_up: true,
            ..State::default()
        };
        Engine {
            inner: Arc::new(Inner {
                writer: spawn_writer(store.clone()),
                rt: tokio::runtime::Handle::current(),
                deps,
                cfg,
                store,
                state: Mutex::new(state),
                events,
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TransferEvent> {
        self.inner.events.subscribe()
    }

    /// Loads the persisted queue (XFR-11): unfinished transfers come back
    /// paused, resumed at once when `auto_resume`. Also purges old history
    /// (XFR-10). Returns the number of unfinished transfers.
    pub async fn load(&self, auto_resume: bool) -> Result<usize> {
        let store = self.inner.store.clone();
        let (now, days) = (self.inner.deps.clock.now_ms(), self.inner.cfg.retention_days);
        let all = blocking(move || {
            store.purge_history(now, days)?;
            store.load_all()
        })
        .await?;
        let mut st = self.inner.lock();
        let mut resumable = Vec::new();
        for t in all {
            let id = t.id;
            if !t.state.is_finished() {
                resumable.push(id);
            }
            self.inner.adopt(&mut st, t);
        }
        if auto_resume {
            for id in &resumable {
                if let Err(e) = self.inner.resume_locked(&mut st, *id) {
                    log::debug!("not resumed: {e}");
                }
            }
        }
        self.inner.refresh_busy(&mut st);
        Ok(resumable.len())
    }

    /// Queues a plan as a transfer and returns its id (XFR-1).
    pub async fn add(&self, plan: Plan, title: &str, options: TransferOptions) -> Result<TransferId> {
        let mut t = Transfer::from_plan(0, plan, title, options, self.inner.deps.clock.now_ms());
        let store = self.inner.store.clone();
        let t = blocking(move || {
            store.insert(&mut t)?;
            Ok(t)
        })
        .await?;
        let id = t.id;
        let mut st = self.inner.lock();
        self.inner.register(&mut st, t);
        self.inner.emit(TransferEvent::Added(id));
        self.inner.apply_gates(&mut st);
        self.inner.refresh_busy(&mut st);
        self.inner.pump(&mut st);
        self.inner.maybe_finish(&mut st, id);
        Ok(id)
    }

    /// All transfers in queue order.
    pub fn list(&self) -> Vec<TransferSummary> {
        let st = self.inner.lock();
        let mut all: Vec<TransferSummary> = st.live.values().map(Live::summary).collect();
        all.sort_by_key(|s| (s.position, s.id));
        all
    }

    pub fn get(&self, id: TransferId) -> Option<TransferSummary> {
        self.inner.lock().live.get(&id).map(Live::summary)
    }

    pub fn items(&self, id: TransferId) -> Result<Vec<TransferItem>> {
        let st = self.inner.lock();
        st.live
            .get(&id)
            .map(|l| l.t.items.clone())
            .ok_or_else(|| not_found(id))
    }

    /// The first item's plan, for the list's direction icon (XFR-8) without
    /// cloning a 100 000 item plan.
    pub fn first_item(&self, id: TransferId) -> Option<PlanItem> {
        let st = self.inner.lock();
        st.live.get(&id)?.t.items.first().map(|i| i.plan.clone())
    }

    /// Unanswered conflicts of a transfer: item index and the question.
    pub fn questions(&self, id: TransferId) -> Result<Vec<(u32, Conflict)>> {
        let st = self.inner.lock();
        let live = st.live.get(&id).ok_or_else(|| not_found(id))?;
        Ok(live
            .t
            .questions()
            .filter_map(|i| i.plan.conflict.clone().map(|c| (i.seq, c)))
            .collect())
    }

    /// Waits until the transfer's summary satisfies `pred`.
    pub async fn wait_for(
        &self,
        id: TransferId,
        pred: impl Fn(&TransferSummary) -> bool,
    ) -> Result<TransferSummary> {
        let mut rx = self.subscribe();
        loop {
            let s = self.get(id).ok_or_else(|| not_found(id))?;
            if pred(&s) {
                return Ok(s);
            }
            if let Err(broadcast::error::RecvError::Closed) = rx.recv().await {
                return Err(Error::new(ErrorKind::Internal, "engine stopped"));
            }
        }
    }

    pub fn pause(&self, id: TransferId) -> Result<()> {
        let mut st = self.inner.lock();
        let state = st.live.get(&id).ok_or_else(|| not_found(id))?.t.state;
        if state.is_finished() || state == TransferState::Paused {
            return Err(bad_state("pause", state));
        }
        self.inner.suspend(&mut st, id, TransferState::Paused)?;
        self.inner.pump(&mut st);
        Ok(())
    }

    pub fn resume(&self, id: TransferId) -> Result<()> {
        let mut st = self.inner.lock();
        self.inner.resume_locked(&mut st, id)
    }

    pub fn pause_all(&self) {
        let mut st = self.inner.lock();
        let ids: Vec<TransferId> = st
            .live
            .values()
            .filter(|l| !l.t.state.is_finished() && l.t.state != TransferState::Paused)
            .map(|l| l.t.id)
            .collect();
        for id in ids {
            if let Err(e) = self.inner.suspend(&mut st, id, TransferState::Paused) {
                log::debug!("not paused: {e}");
            }
        }
    }

    pub fn resume_all(&self) {
        let mut st = self.inner.lock();
        let ids: Vec<TransferId> = st
            .live
            .values()
            .filter(|l| l.t.state == TransferState::Paused)
            .map(|l| l.t.id)
            .collect();
        for id in ids {
            if let Err(e) = self.inner.resume_locked(&mut st, id) {
                log::debug!("not resumed: {e}");
            }
        }
    }

    pub fn cancel(&self, id: TransferId) -> Result<()> {
        let mut st = self.inner.lock();
        self.inner.cancel_locked(&mut st, id)
    }

    /// *Retry failed* (XFR-9): only the failed items run again.
    pub fn retry_failed(&self, id: TransferId) -> Result<()> {
        let mut st = self.inner.lock();
        self.inner.retry_locked(&mut st, id)
    }

    pub fn move_up(&self, id: TransferId) -> bool {
        self.reorder(|s| s.move_up(id))
    }

    pub fn move_down(&self, id: TransferId) -> bool {
        self.reorder(|s| s.move_down(id))
    }

    pub fn move_to_top(&self, id: TransferId) -> bool {
        self.reorder(|s| s.move_to_top(id))
    }

    fn reorder(&self, f: impl FnOnce(&mut Scheduler) -> bool) -> bool {
        let mut st = self.inner.lock();
        if !f(&mut st.sched) {
            return false;
        }
        self.inner.renumber(&mut st);
        self.inner.pump(&mut st);
        true
    }

    /// Answers a conflict (OPS-2). `apply_to_all` also settles the other open
    /// questions that allow the choice and answers future ones the same way.
    pub fn answer(
        &self,
        id: TransferId,
        item: u32,
        choice: ConflictChoice,
        apply_to_all: bool,
    ) -> Result<()> {
        let mut st = self.inner.lock();
        self.inner.answer_locked(&mut st, id, item, choice, apply_to_all)
    }

    /// The bridge came or went (NVB-12). Transfers on bridge locations wait
    /// and resume by themselves.
    pub fn bridge_available(&self, up: bool) {
        let mut st = self.inner.lock();
        st.bridge_up = up;
        self.inner.apply_gates(&mut st);
        self.inner.pump(&mut st);
    }

    /// Network reachability as reported by the bridge (XFR-7).
    pub fn network_available(&self, up: bool) {
        let mut st = self.inner.lock();
        st.network_up = up;
        self.inner.apply_gates(&mut st);
        self.inner.pump(&mut st);
    }

    /// A removable volume vanished or came back; affected transfers show
    /// "Insert the card to continue".
    pub fn volume_present(&self, location: &str, present: bool) {
        let mut st = self.inner.lock();
        if present {
            st.volumes_gone.remove(location);
        } else {
            st.volumes_gone.insert(location.to_owned());
        }
        self.inner.apply_gates(&mut st);
        self.inner.pump(&mut st);
    }

    /// Concurrency of a remote location, 1 to 6 (XFR-2).
    pub fn set_remote_limit(&self, location: &str, limit: usize) {
        let mut st = self.inner.lock();
        st.sched.set_remote_limit(location, limit);
        self.inner.pump(&mut st);
    }

    /// True while any transfer is queued or running: hold `KeepAlive` (XFR-5).
    pub fn is_busy(&self) -> bool {
        self.inner.lock().busy
    }

    /// Transfers still unfinished, for the "will resume when you open Lautta
    /// again" notification (XFR-6).
    pub fn pending_summary(&self) -> usize {
        self.inner
            .lock()
            .live
            .values()
            .filter(|l| !l.t.state.is_finished())
            .count()
    }

    /// Removes a finished transfer from the history.
    pub fn forget(&self, id: TransferId) -> Result<()> {
        let mut st = self.inner.lock();
        let state = st.live.get(&id).ok_or_else(|| not_found(id))?.t.state;
        if !state.is_finished() {
            return Err(bad_state("forget", state));
        }
        st.live.remove(&id);
        self.inner.send(Write::Delete(id));
        self.inner.emit(TransferEvent::Changed(id));
        Ok(())
    }

    /// Removes finished transfers older than the retention (XFR-10).
    pub async fn purge_history(&self) -> Result<usize> {
        let store = self.inner.store.clone();
        let (now, days) = (self.inner.deps.clock.now_ms(), self.inner.cfg.retention_days);
        self.flush().await;
        let n = blocking(move || store.purge_history(now, days)).await?;
        let cutoff = now - i64::from(days) * 86_400_000;
        let mut st = self.inner.lock();
        st.live
            .retain(|_, l| !(l.t.state.is_finished() && l.t.finished_ms.unwrap_or(l.t.created_ms) < cutoff));
        Ok(n)
    }

    /// Waits until everything queued so far is written to the database.
    pub async fn flush(&self) {
        let (tx, rx) = oneshot::channel();
        if self.inner.writer.send(Cmd::Flush(tx)).is_ok() {
            let _ = rx.await;
        }
    }
}

// ------------------------------------------------------------ internals

impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    fn emit(&self, e: TransferEvent) {
        let _ = self.events.send(e);
    }

    fn send(&self, w: Write) {
        let _ = self.writer.send(Cmd::Write(Box::new(w)));
    }

    fn write_summary(&self, live: &Live) {
        self.send(Write::Summary(live.summary()));
    }

    fn write_item(&self, live: &Live, seq: u32) {
        if let Some(item) = live.t.items.get(seq as usize) {
            self.send(Write::Item(live.t.id, item.clone()));
        }
    }

    fn refresh_busy(&self, st: &mut State) {
        let busy = st.live.values().any(|l| {
            matches!(
                l.t.state,
                TransferState::Queued | TransferState::Scanning | TransferState::Running
            )
        });
        if busy != st.busy {
            st.busy = busy;
            self.emit(TransferEvent::Busy(busy));
        }
    }

    fn live_mut<'a>(&self, st: &'a mut State, id: TransferId) -> Result<&'a mut Live> {
        st.live.get_mut(&id).ok_or_else(|| not_found(id))
    }

    /// Registers a freshly inserted transfer.
    fn register(&self, st: &mut State, t: Transfer) {
        let live = Live::new(t);
        let op = live.t.kind;
        st.sched
            .add(live.t.id, &live.t.dest.location, live.pending_items(op));
        st.live.insert(live.t.id, live);
    }

    /// Takes a transfer loaded from the database into the queue (XFR-11).
    fn adopt(&self, st: &mut State, mut t: Transfer) {
        let id = t.id;
        if t.state.is_finished() {
            st.live.insert(id, Live::new(t));
            return;
        }
        let stale: Vec<u32> = t
            .items
            .iter()
            .filter(|i| i.state == ItemState::Running)
            .map(|i| i.seq)
            .collect();
        for seq in stale {
            t.set_item_state(seq, ItemState::Pending);
        }
        t.recount();
        t.state = TransferState::Paused;
        self.register(st, t);
        if let Some(live) = st.live.get(&id) {
            for item in &live.t.items {
                if item.state == ItemState::Pending {
                    self.write_item(live, item.seq);
                }
            }
            self.write_summary(live);
        }
        st.sched.set_runnable(id, false);
        self.emit(TransferEvent::Added(id));
    }

    /// Changes state through the transition table and tells everyone.
    fn set_state(&self, st: &mut State, id: TransferId, to: TransferState) -> Result<()> {
        let now = self.deps.clock.now_ms();
        let live = self.live_mut(st, id)?;
        live.t.transition(to)?;
        if to.is_finished() {
            live.t.finished_ms = Some(now);
            live.rate.reset();
        }
        self.write_summary(live);
        st.sched.set_runnable(id, to.is_runnable());
        if to.is_finished() {
            st.throttle.forget(id);
            st.sched.remove(id);
        }
        self.emit(TransferEvent::Changed(id));
        self.refresh_busy(st);
        Ok(())
    }

    /// Stops the running items of a transfer (their partial files stay for
    /// resume, XFR-12), puts them back in line and moves to `to`.
    fn suspend(&self, st: &mut State, id: TransferId, to: TransferState) -> Result<()> {
        let live = st.live.get_mut(&id).ok_or_else(|| not_found(id))?;
        let op = live.t.kind;
        if let Some(h) = live.finalizer.take() {
            h.abort();
        }
        let running: Vec<(u32, AbortHandle)> = live.running.drain().collect();
        let mut requeue = Vec::new();
        for (seq, handle) in running {
            handle.abort();
            live.item_bytes.remove(&seq);
            live.t.set_item_state(seq, ItemState::Pending);
            self.write_item(live, seq);
            let kind = live.t.items[seq as usize].plan.kind;
            requeue.push((seq, is_barrier(op, kind)));
            st.sched.item_finished(ItemRef { transfer: id, seq });
        }
        live.rate.reset();
        st.sched.requeue(id, requeue);
        self.set_state(st, id, to)
    }

    fn resume_locked(self: &Arc<Self>, st: &mut State, id: TransferId) -> Result<()> {
        let live = st.live.get_mut(&id).ok_or_else(|| not_found(id))?;
        let state = live.t.state;
        if !matches!(state, TransferState::Paused | TransferState::Waiting(_)) {
            return Err(bad_state("resume", state));
        }
        let op = live.t.kind;
        let reopened: Vec<u32> = live
            .t
            .items
            .iter()
            .filter(|i| i.state == ItemState::NeedsAnswer)
            .map(|i| i.seq)
            .collect();
        for seq in &reopened {
            live.t.set_item_state(*seq, ItemState::Pending);
            self.write_item(live, *seq);
        }
        let again: Vec<(u32, bool)> = reopened
            .iter()
            .map(|s| (*s, is_barrier(op, live.t.items[*s as usize].plan.kind)))
            .collect();
        st.sched.requeue(id, again);
        self.set_state(st, id, TransferState::Queued)?;
        self.apply_gates(st);
        self.pump(st);
        self.maybe_finish(st, id);
        Ok(())
    }

    fn cancel_locked(self: &Arc<Self>, st: &mut State, id: TransferId) -> Result<()> {
        let live = st.live.get_mut(&id).ok_or_else(|| not_found(id))?;
        if live.t.state.is_finished() {
            return Err(bad_state("cancel", live.t.state));
        }
        if let Some(h) = live.finalizer.take() {
            h.abort();
        }
        let running: Vec<(u32, AbortHandle)> = live.running.drain().collect();
        for (seq, handle) in running {
            handle.abort();
            live.item_bytes.remove(&seq);
            st.sched.item_finished(ItemRef { transfer: id, seq });
        }
        let leftovers: Vec<TransferItem> = live
            .t
            .items
            .iter()
            .filter(|i| i.temp_name.is_some() && i.state != ItemState::Done)
            .cloned()
            .collect();
        let op = live.t.kind;
        self.set_state(st, id, TransferState::Canceled)?;
        self.emit(TransferEvent::Finished(id));
        self.pump(st);
        let inner = self.clone();
        self.rt
            .spawn(async move { inner.remove_leftovers(op, leftovers).await });
        Ok(())
    }

    async fn remove_leftovers(&self, op: OperationKind, items: Vec<TransferItem>) {
        if op == OperationKind::Delete {
            return;
        }
        for item in items {
            let Some(temp_name) = item.temp_name.clone() else {
                continue;
            };
            if let Ok(dst) = self.deps.resolver.provider(&item.plan.dst.location) {
                exec::cleanup_temp(
                    &*dst,
                    &ItemJob {
                        plan: item.plan,
                        temp_name,
                    },
                )
                .await;
            }
        }
    }

    fn retry_locked(self: &Arc<Self>, st: &mut State, id: TransferId) -> Result<()> {
        let live = self.live_mut(st, id)?;
        if live.t.state != TransferState::Failed {
            return Err(bad_state("retry", live.t.state));
        }
        let failed: Vec<u32> = live
            .t
            .items
            .iter()
            .filter(|i| i.state == ItemState::Failed)
            .map(|i| i.seq)
            .collect();
        for seq in failed {
            live.t.set_item_state(seq, ItemState::Pending);
            let item = &mut live.t.items[seq as usize];
            item.attempts = 0;
            item.error = None;
            self.write_item(live, seq);
        }
        live.t.error = None;
        live.t.finished_ms = None;
        let (op, loc) = (live.t.kind, live.t.dest.location.clone());
        let pending = live.pending_items(op);
        self.set_state(st, id, TransferState::Queued)?;
        st.sched.add(id, &loc, pending);
        self.apply_gates(st);
        self.pump(st);
        self.maybe_finish(st, id);
        Ok(())
    }

    /// The reason this transfer cannot run right now, if any.
    fn blocked_reason(st: &State, live: &Live) -> Option<WaitReason> {
        let remote = live.locations.iter().any(|l| is_bridge_location(l));
        if remote && !st.bridge_up {
            return Some(WaitReason::Bridge);
        }
        if remote && !st.network_up {
            return Some(WaitReason::Network);
        }
        live.locations
            .iter()
            .any(|l| st.volumes_gone.contains(l))
            .then_some(WaitReason::Volume)
    }

    /// Puts transfers into or out of the bridge, network and volume waits.
    fn apply_gates(self: &Arc<Self>, st: &mut State) {
        let decisions: Vec<(TransferId, TransferState, Option<WaitReason>)> = st
            .live
            .values()
            .map(|l| (l.t.id, l.t.state, Self::blocked_reason(st, l)))
            .collect();
        for (id, state, blocked) in decisions {
            let outcome = match (state, blocked) {
                (s, Some(r)) if s.is_runnable() => self.suspend(st, id, TransferState::Waiting(r)),
                (TransferState::Waiting(cur), Some(r)) if cur != r && cur != WaitReason::Question => {
                    self.set_state(st, id, TransferState::Waiting(r))
                }
                (TransferState::Waiting(cur), None) if cur != WaitReason::Question => {
                    self.set_state(st, id, TransferState::Queued)
                }
                _ => Ok(()),
            };
            if let Err(e) = outcome {
                log::debug!("gate for transfer {id}: {e}");
            }
        }
        self.pump(st);
    }

    fn renumber(&self, st: &mut State) {
        let order = st.sched.order();
        let mut slots: Vec<i64> = order
            .iter()
            .filter_map(|id| st.live.get(id).map(|l| l.t.position))
            .collect();
        slots.sort_unstable();
        for (id, pos) in order.iter().zip(slots) {
            if let Some(live) = st.live.get_mut(id) {
                live.t.position = pos;
                self.write_summary(live);
                self.emit(TransferEvent::Changed(*id));
            }
        }
    }

    /// Starts whatever the scheduler allows.
    fn pump(self: &Arc<Self>, st: &mut State) {
        for pick in st.sched.start_runnable() {
            if st.live.get(&pick.transfer).map(|l| l.t.state) == Some(TransferState::Queued) {
                if let Err(e) = self.set_state(st, pick.transfer, TransferState::Running) {
                    log::debug!("cannot start: {e}");
                }
            }
            let Some(live) = st.live.get_mut(&pick.transfer) else {
                st.sched.item_finished(pick);
                continue;
            };
            live.t.set_item_state(pick.seq, ItemState::Running);
            self.write_item(live, pick.seq);
            let handle = self.rt.spawn(run_item_task(self.clone(), pick));
            live.running.insert(pick.seq, handle.abort_handle());
        }
    }

    /// Completes the transfer once every item is settled (XFR-9).
    fn maybe_finish(self: &Arc<Self>, st: &mut State, id: TransferId) {
        let Some(live) = st.live.get_mut(&id) else {
            return;
        };
        let open = matches!(
            live.t.state,
            TransferState::Queued
                | TransferState::Scanning
                | TransferState::Running
                | TransferState::Waiting(WaitReason::Question)
        );
        let settled = live.t.items_done + live.t.items_failed == live.t.items_total;
        if !open || !settled || !live.running.is_empty() || live.finalizer.is_some() {
            return;
        }
        if live.t.kind == OperationKind::Move {
            let dirs: Vec<Uri> = live
                .t
                .items
                .iter()
                .filter(|i| i.plan.kind == Kind::Dir && i.state == ItemState::Done)
                .map(|i| i.plan.src.clone())
                .collect();
            if !dirs.is_empty() {
                let inner = self.clone();
                let handle = self.rt.spawn(async move {
                    inner.remove_moved_dirs(dirs).await;
                    let mut st = inner.lock();
                    inner.complete(&mut st, id);
                });
                live.finalizer = Some(handle.abort_handle());
                return;
            }
        }
        self.complete(st, id);
    }

    /// OPS-4: after the files are verified and gone, empty source folders go
    /// too, deepest first. A folder that still holds something stays.
    async fn remove_moved_dirs(&self, mut dirs: Vec<Uri>) {
        dirs.sort_by_key(|d| std::cmp::Reverse(d.path.depth()));
        for dir in dirs {
            let Ok(p) = self.deps.resolver.provider(&dir.location) else {
                continue;
            };
            if let Err(e) = p.remove_dir(&dir.path).await {
                log::debug!("source folder stays: {e}");
            }
        }
    }

    fn complete(&self, st: &mut State, id: TransferId) {
        let Some(live) = st.live.get_mut(&id) else {
            return;
        };
        live.finalizer = None;
        let failed = live.t.items_failed;
        live.t.error = (failed > 0).then(|| format!("{failed} items failed"));
        let to = if failed > 0 {
            TransferState::Failed
        } else {
            TransferState::Completed
        };
        // Queued (nothing to do) and Waiting(Question) pass through Running.
        if live.t.state != TransferState::Running {
            if let Err(e) = self.set_state(st, id, TransferState::Running) {
                log::debug!("complete: {e}");
                return;
            }
        }
        if let Err(e) = self.set_state(st, id, to) {
            log::debug!("complete: {e}");
            return;
        }
        self.emit(TransferEvent::Finished(id));
    }

    // --------------------------------------------------- item results

    fn on_progress(&self, id: TransferId, seq: u32, done: u64) {
        let now = self.deps.clock.now_ms();
        let mut st = self.lock();
        let due = st.throttle.due(id, now);
        let Some(live) = st.live.get_mut(&id) else {
            return;
        };
        live.item_bytes.insert(seq, done);
        let summary = live.summary();
        live.rate.sample(now, summary.bytes_done);
        if now.saturating_sub(live.last_event_ms) >= self.cfg.progress_event_interval_ms {
            live.last_event_ms = now;
            self.emit(TransferEvent::Progress {
                id,
                bytes_done: summary.bytes_done,
                rate: live.rate.rate(),
                eta_secs: live
                    .rate
                    .eta_secs(summary.bytes_total.saturating_sub(summary.bytes_done)),
            });
        }
        if due {
            self.send(Write::Summary(summary));
        }
    }

    /// Detaches a finished item from its transfer's running set.
    fn release(&self, st: &mut State, pick: ItemRef) {
        if let Some(live) = st.live.get_mut(&pick.transfer) {
            live.running.remove(&pick.seq);
            live.item_bytes.remove(&pick.seq);
        }
        st.sched.item_finished(pick);
    }

    fn settle(self: &Arc<Self>, pick: ItemRef, state: ItemState, error: Option<String>) {
        let mut st = self.lock();
        self.release(&mut st, pick);
        let Some(live) = st.live.get_mut(&pick.transfer) else {
            return;
        };
        live.t.set_item_state(pick.seq, state);
        if let Some(item) = live.t.items.get_mut(pick.seq as usize) {
            item.error = error;
            item.attempts = 0;
            if state == ItemState::Done {
                item.committed = item.size();
            }
        }
        self.write_item(live, pick.seq);
        if state == ItemState::Skipped && live.t.items[pick.seq as usize].plan.kind == Kind::Dir {
            self.settle_subtree(&mut st, pick, ItemState::Skipped);
        }
        self.after_settle(&mut st, pick.transfer);
    }

    fn after_settle(self: &Arc<Self>, st: &mut State, id: TransferId) {
        if let Some(live) = st.live.get(&id) {
            self.emit(TransferEvent::Changed(id));
            let summary = live.summary();
            if st.throttle.due(id, self.deps.clock.now_ms()) {
                self.send(Write::Summary(summary));
            }
        }
        self.maybe_finish(st, id);
        self.pump(st);
    }

    /// Everything not started below the settled item gets the same state
    /// (a folder renamed as a whole, trashed, or skipped).
    fn settle_subtree(&self, st: &mut State, pick: ItemRef, to: ItemState) {
        let Some(live) = st.live.get_mut(&pick.transfer) else {
            return;
        };
        let root = live.t.items[pick.seq as usize].plan.src.clone();
        let inside: Vec<u32> = live
            .t
            .items
            .iter()
            .filter(|i| i.seq != pick.seq && i.plan.src.is_inside(&root))
            .filter(|i| matches!(i.state, ItemState::Pending | ItemState::NeedsAnswer))
            .map(|i| i.seq)
            .collect();
        for seq in &inside {
            live.t.set_item_state(*seq, to);
            self.write_item(live, *seq);
        }
        let gone: HashSet<u32> = inside.into_iter().collect();
        st.sched.drop_pending(pick.transfer, |s| !gone.contains(&s));
    }

    fn subtree_done(self: &Arc<Self>, pick: ItemRef) {
        let mut st = self.lock();
        self.release(&mut st, pick);
        let Some(live) = st.live.get_mut(&pick.transfer) else {
            return;
        };
        live.t.set_item_state(pick.seq, ItemState::Done);
        self.write_item(live, pick.seq);
        self.settle_subtree(&mut st, pick, ItemState::Done);
        self.after_settle(&mut st, pick.transfer);
    }

    fn ask(self: &Arc<Self>, pick: ItemRef, conflict: Conflict) {
        let mut st = self.lock();
        self.release(&mut st, pick);
        let id = pick.transfer;
        let Some(live) = st.live.get_mut(&id) else {
            return;
        };
        live.t.set_item_state(pick.seq, ItemState::NeedsAnswer);
        live.t.items[pick.seq as usize].plan.conflict = Some(conflict);
        self.write_item(live, pick.seq);
        if live.t.state != TransferState::Waiting(WaitReason::Question) {
            let to = TransferState::Waiting(WaitReason::Question);
            if let Err(e) = self.set_state(&mut st, id, to) {
                log::debug!("question: {e}");
            }
        }
        self.emit(TransferEvent::NeedsAnswer { id, item: pick.seq });
        self.pump(&mut st);
    }

    fn rebase(&self, pick: ItemRef, job: &mut ItemJob, new_dst: Uri) {
        let mut st = self.lock();
        let Some(live) = st.live.get_mut(&pick.transfer) else {
            return;
        };
        let old = live.t.items[pick.seq as usize].plan.dst.clone();
        let mut touched = vec![pick.seq];
        for item in &mut live.t.items {
            if item.seq == pick.seq {
                item.plan.dst = new_dst.clone();
                item.plan.resolution = None;
                item.plan.conflict = None;
                item.temp_name = Some(exec::temp_name_for(
                    new_dst.name().unwrap_or(b"item"),
                    pick.transfer,
                    pick.seq,
                ));
                job.plan = item.plan.clone();
                job.temp_name = item.temp_name.clone().unwrap_or_default();
            } else if item.state == ItemState::Pending && item.plan.dst.is_inside(&old) {
                let rel = item.plan.dst.path.strip_prefix(&old.path).unwrap_or_default();
                item.plan.dst = Uri::new(new_dst.location.clone(), new_dst.path.join_path(&rel));
                touched.push(item.seq);
            }
        }
        for seq in touched {
            self.write_item(live, seq);
        }
    }

    /// Decides what an item task does after `run_item` returned.
    fn on_result(
        self: &Arc<Self>,
        pick: ItemRef,
        job: &mut ItemJob,
        res: Result<Outcome>,
        attempt: u32,
    ) -> Next {
        match res {
            Ok(Outcome::Done) => self.settle(pick, ItemState::Done, None),
            Ok(Outcome::Skipped) => self.settle(pick, ItemState::Skipped, None),
            Ok(Outcome::Subtree) => self.subtree_done(pick),
            Ok(Outcome::Ask(c)) => self.ask(pick, c),
            Ok(Outcome::KeepBoth(uri)) => {
                self.rebase(pick, job, uri);
                return Next::Rerun;
            }
            Err(e) => return self.on_error(pick, e, attempt),
        }
        Next::Stop
    }

    fn on_error(self: &Arc<Self>, pick: ItemRef, e: Error, attempt: u32) -> Next {
        if !e.kind.is_transient() {
            return Next::Fail(e);
        }
        if attempt < self.cfg.max_retries {
            let backoff = self.cfg.backoff_base * 2u32.pow(attempt);
            let hint = Duration::from_millis(e.retry_after_ms.unwrap_or(0));
            self.note_attempt(pick, attempt + 1);
            return Next::Retry(backoff.max(hint));
        }
        match wait_reason_for(e.kind) {
            Some(reason) => {
                let mut st = self.lock();
                if let Err(err) = self.suspend(&mut st, pick.transfer, TransferState::Waiting(reason)) {
                    log::debug!("cannot wait: {err}");
                }
                self.pump(&mut st);
                Next::Stop
            }
            None => Next::Fail(e),
        }
    }

    fn note_attempt(&self, pick: ItemRef, attempts: u32) {
        let mut st = self.lock();
        if let Some(live) = st.live.get_mut(&pick.transfer) {
            if let Some(item) = live.t.items.get_mut(pick.seq as usize) {
                item.attempts = attempts;
            }
            self.write_item(live, pick.seq);
        }
    }

    fn answer_locked(
        self: &Arc<Self>,
        st: &mut State,
        id: TransferId,
        seq: u32,
        choice: ConflictChoice,
        apply_to_all: bool,
    ) -> Result<()> {
        let live = self.live_mut(st, id)?;
        let item = live
            .t
            .items
            .get(seq as usize)
            .filter(|i| i.state == ItemState::NeedsAnswer)
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "no open question"))?;
        let allowed = |i: &TransferItem| {
            i.plan
                .conflict
                .as_ref()
                .is_some_and(|c| c.choices.contains(&choice))
        };
        if !allowed(item) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "that answer does not apply to this conflict",
            ));
        }
        let mut targets = vec![seq];
        if apply_to_all {
            live.t.options.resolve_all = Some(choice);
            targets = live
                .t
                .items
                .iter()
                .filter(|i| i.state == ItemState::NeedsAnswer && allowed(i))
                .map(|i| i.seq)
                .collect();
        }
        for s in targets {
            self.apply_answer(st, id, s, choice);
        }
        let live = self.live_mut(st, id)?;
        let waiting = live.t.state == TransferState::Waiting(WaitReason::Question);
        if waiting && live.t.questions().next().is_none() {
            self.set_state(st, id, TransferState::Queued)?;
        }
        if let Some(live) = st.live.get(&id) {
            self.write_summary(live);
        }
        self.apply_gates(st);
        self.maybe_finish(st, id);
        self.pump(st);
        Ok(())
    }

    fn apply_answer(&self, st: &mut State, id: TransferId, seq: u32, choice: ConflictChoice) {
        let Some(live) = st.live.get_mut(&id) else {
            return;
        };
        let op = live.t.kind;
        live.t.items[seq as usize].plan.resolution = Some(choice);
        if choice == ConflictChoice::Skip {
            live.t.set_item_state(seq, ItemState::Skipped);
            self.write_item(live, seq);
            if live.t.items[seq as usize].plan.kind == Kind::Dir {
                self.settle_subtree(st, ItemRef { transfer: id, seq }, ItemState::Skipped);
            }
            return;
        }
        live.t.set_item_state(seq, ItemState::Pending);
        self.write_item(live, seq);
        let barrier = is_barrier(op, live.t.items[seq as usize].plan.kind);
        st.sched.requeue(id, [(seq, barrier)]);
    }

    /// Reads what an item task needs and resolves its providers.
    fn prepare(self: &Arc<Self>, pick: ItemRef) -> Option<Prepared> {
        let (plan, temp_name, op, options) = {
            let mut st = self.lock();
            let live = st.live.get_mut(&pick.transfer)?;
            let item = live.t.items.get_mut(pick.seq as usize)?;
            if item.temp_name.is_none() {
                let name = item.plan.dst.name().unwrap_or(b"item").to_vec();
                item.temp_name = Some(exec::temp_name_for(&name, pick.transfer, pick.seq));
                self.write_item(live, pick.seq);
            }
            let item = &live.t.items[pick.seq as usize];
            (
                item.plan.clone(),
                item.temp_name.clone().unwrap_or_default(),
                live.t.kind,
                live.t.options.clone(),
            )
        };
        let providers = (
            self.deps.resolver.provider(&plan.src.location),
            self.deps.resolver.provider(&plan.dst.location),
        );
        let (src, dst) = match providers {
            (Ok(s), Ok(d)) => (s, d),
            (Err(e), _) | (_, Err(e)) => {
                self.settle(pick, ItemState::Failed, Some(e.to_string()));
                return None;
            }
        };
        let inner = self.clone();
        let progress: ProgressSink =
            Arc::new(move |done, _| inner.on_progress(pick.transfer, pick.seq, done));
        let env = ItemEnv {
            same_location: plan.src.location == plan.dst.location,
            src,
            dst,
            op,
            options,
            scratch_dir: self.cfg.scratch_dir.clone(),
            trasher: self.deps.trasher.clone(),
            progress,
        };
        Some(Prepared {
            env,
            job: ItemJob { plan, temp_name },
        })
    }
}

/// One item from start to settled: retries transient errors with backoff
/// (XFR-9), keeps the partial file for resume (XFR-12).
async fn run_item_task(inner: Arc<Inner>, pick: ItemRef) {
    let Some(Prepared { env, mut job }) = inner.prepare(pick) else {
        return;
    };
    let mut attempt = 0;
    loop {
        let res = exec::run_item(&env, &job).await;
        match inner.on_result(pick, &mut job, res, attempt) {
            Next::Stop => return,
            Next::Rerun => {}
            Next::Retry(delay) => {
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Next::Fail(e) => {
                if e.kind != ErrorKind::AlreadyExists {
                    exec::cleanup_temp(&*env.dst, &job).await;
                }
                inner.settle(pick, ItemState::Failed, Some(e.to_string()));
                return;
            }
        }
    }
}
