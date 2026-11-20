// SPDX-License-Identifier: LGPL-2.1-or-later
//! Shared state of the fake bridge and of one client session.

use super::tree::Tree;
use crate::access::{self, After, Level, Own};
use crate::wire::{
    drive_info, feature, location_state, WireCapabilities, WireDrive, WireLocation, WireNearby,
};
use std::collections::{BTreeMap, HashMap};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tokio::time::Instant;
use zbus::zvariant::{OwnedValue, Value};

/// What the user decided for this app, for every account location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consent {
    /// Not decided yet.
    Unknown,
    /// No account.
    Denied,
    /// Every account at this level.
    Allowed(Level),
}

impl Consent {
    /// Word of a contract sequence (as in netvfs `bridge.conf`).
    pub fn as_str(self) -> &'static str {
        match self {
            Consent::Unknown => "unknown",
            Consent::Denied => "denied",
            Consent::Allowed(level) => level.as_str(),
        }
    }

    pub fn parse(word: &str) -> Option<Consent> {
        match word {
            "unknown" => Some(Consent::Unknown),
            "denied" => Some(Consent::Denied),
            _ => Level::parse(word).map(Consent::Allowed),
        }
    }

    /// What `GetConsent` and `ConsentChanged` say: `unknown` or `decided`.
    pub fn wire(self) -> &'static str {
        match self {
            Consent::Unknown => "unknown",
            Consent::Denied | Consent::Allowed(_) => "decided",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Failure {
    pub method: String,
    pub name: String,
    pub message: String,
    pub detail: Option<String>,
    pub retry_after_ms: Option<u64>,
    pub skip: u32,
    pub remaining: Option<u32>,
}

impl Failure {
    pub fn new(method: &str, name: &str, message: &str) -> Failure {
        Failure {
            method: method.to_owned(),
            name: name.to_owned(),
            message: message.to_owned(),
            detail: None,
            retry_after_ms: None,
            skip: 0,
            remaining: None,
        }
    }

    pub fn times(mut self, n: u32) -> Failure {
        self.remaining = Some(n);
        self
    }

    pub fn after(mut self, calls: u32) -> Failure {
        self.skip = calls;
        self
    }

    pub fn retry_after(mut self, ms: u64) -> Failure {
        self.retry_after_ms = Some(ms);
        self
    }

    pub fn detail(mut self, detail: &str) -> Failure {
        self.detail = Some(detail.to_owned());
        self
    }
}

#[derive(Debug, Clone)]
pub struct JobFailure {
    pub method: String,
    pub after_bytes: u64,
    pub name: String,
    pub message: String,
    pub detail: Option<String>,
    pub retry_after_ms: Option<u64>,
    pub remaining: Option<u32>,
}

impl JobFailure {
    pub fn new(method: &str, after_bytes: u64, name: &str, message: &str) -> JobFailure {
        JobFailure {
            method: method.to_owned(),
            after_bytes,
            name: name.to_owned(),
            message: message.to_owned(),
            detail: None,
            retry_after_ms: None,
            remaining: Some(1),
        }
    }

    pub fn detail(mut self, detail: &str, retry_after_ms: Option<u64>) -> JobFailure {
        self.detail = Some(detail.to_owned());
        self.retry_after_ms = retry_after_ms;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InfoValue {
    Str(String),
    Int(i32),
    Bool(bool),
    Bytes(Vec<u8>),
    Strings(Vec<String>),
}

impl InfoValue {
    pub fn to_owned_value(&self) -> OwnedValue {
        let v = match self {
            InfoValue::Str(s) => Value::from(s.clone()),
            InfoValue::Int(n) => Value::from(*n),
            InfoValue::Bool(b) => Value::from(*b),
            InfoValue::Bytes(b) => Value::from(b.clone()),
            InfoValue::Strings(s) => Value::from(s.clone()),
        };
        // only fd values fail; plain values hold none
        OwnedValue::try_from(v).unwrap_or_else(|_| OwnedValue::from(0u8))
    }
}

/// Memory card or USB drive of the fake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeDrive {
    pub id: String,
    pub label: String,
    /// Empty when not mounted.
    pub mount_point: Vec<u8>,
    /// `sd` or `usb`.
    pub kind: String,
    pub size: i64,
    pub uuid: String,
    /// LUKS device not unlocked: listed as `locked`, never mounted.
    pub locked: bool,
}

impl FakeDrive {
    pub fn new(id: &str, label: &str, kind: &str, size: i64) -> FakeDrive {
        FakeDrive {
            id: id.to_owned(),
            label: label.to_owned(),
            mount_point: Vec::new(),
            kind: kind.to_owned(),
            size,
            uuid: String::new(),
            locked: false,
        }
    }

    pub fn locked(mut self) -> FakeDrive {
        self.locked = true;
        self
    }

    pub fn with_uuid(mut self, uuid: &str) -> FakeDrive {
        uuid.clone_into(&mut self.uuid);
        self
    }

    pub fn mounted(mut self, mount_point: &[u8]) -> FakeDrive {
        self.mount_point = mount_point.to_vec();
        self
    }

    pub fn wire(&self) -> WireDrive {
        let fs_type = if self.locked { "crypto_LUKS" } else { "vfat" };
        let mut info = vec![
            (drive_info::KIND, InfoValue::Str(self.kind.clone())),
            (drive_info::FS_TYPE, InfoValue::Str(fs_type.to_owned())),
            (drive_info::READ_ONLY, InfoValue::Bool(false)),
        ];
        if !self.uuid.is_empty() {
            info.push((drive_info::UUID, InfoValue::Str(self.uuid.clone())));
        }
        if self.locked {
            info.push((drive_info::LOCKED, InfoValue::Bool(true)));
        }
        let mut info: HashMap<String, OwnedValue> = info
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.to_owned_value()))
            .collect();
        info.insert(drive_info::SIZE.to_owned(), OwnedValue::from(self.size));
        WireDrive {
            id: self.id.clone(),
            label: self.label.clone(),
            mount_point: self.mount_point.clone(),
            info,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallRecord {
    pub method: String,
    pub lane: String,
    pub target: String,
}

#[derive(Debug, Clone)]
pub struct LocationRec {
    pub provider: String,
    pub name: String,
    pub info: Vec<(String, InfoValue)>,
}

pub struct State {
    pub consent: Consent,
    /// Per location, overriding [`State::consent`]: `None` not allowed.
    pub access: HashMap<String, Option<Level>>,
    /// What the app added at an add level.
    pub own: HashMap<(String, Vec<u8>), Own>,
    pub consent_requests: u32,
    pub auto_consent: Option<Consent>,
    pub bridge_version: String,
    /// `None`: bridge without `GetFeatures`; answers `UnknownMethod`.
    pub features: Option<Vec<String>>,
    pub drives: Vec<FakeDrive>,
    /// Where `Mount` makes the drive's folder; `None`: below `/run/media/user`, not made.
    pub mount_root: Option<PathBuf>,
    pub locations: BTreeMap<String, LocationRec>,
    pub trees: HashMap<String, Tree>,
    pub caps: HashMap<String, WireCapabilities>,
    pub space_total: HashMap<String, i64>,
    pub latency: Duration,
    pub chunk: usize,
    pub failures: Vec<Failure>,
    pub job_failures: Vec<JobFailure>,
    pub calls: Vec<CallRecord>,
    pub location_replies: usize,
    pub jobs_canceled: usize,
    pub handoffs: Vec<String>,
    pub nearby: Vec<WireNearby>,
    pub sessions: Vec<Arc<Session>>,
    pub accepting: bool,
    pub accepts: Vec<Instant>,
}

impl Default for State {
    fn default() -> State {
        State {
            consent: Consent::Allowed(Level::Full),
            access: HashMap::new(),
            own: HashMap::new(),
            consent_requests: 0,
            auto_consent: None,
            bridge_version: "0.2.0-fake".to_owned(),
            features: Some(vec![feature::STORAGE_SETTINGS.to_owned()]),
            drives: Vec::new(),
            mount_root: None,
            locations: BTreeMap::new(),
            trees: HashMap::new(),
            caps: HashMap::new(),
            space_total: HashMap::new(),
            latency: Duration::ZERO,
            chunk: 64 * 1024,
            failures: Vec::new(),
            job_failures: Vec::new(),
            calls: Vec::new(),
            location_replies: 0,
            jobs_canceled: 0,
            handoffs: Vec::new(),
            nearby: Vec::new(),
            sessions: Vec::new(),
            accepting: true,
            accepts: Vec::new(),
        }
    }
}

impl State {
    /// Bridges not naming the drive methods answer `UnknownMethod`.
    pub fn offers_drives(&self) -> bool {
        self.features
            .as_ref()
            .is_some_and(|f| f.iter().any(|n| n == feature::DRIVES))
    }

    /// Returns (mount point before, after); `None` for an unknown drive. Mounted below the
    /// mount root by label, else UUID, else id.
    pub fn mount(&mut self, drive: &str, mount: bool) -> Option<(Vec<u8>, Vec<u8>)> {
        let root = self
            .mount_root
            .clone()
            .unwrap_or_else(|| PathBuf::from("/run/media/user"));
        let d = self.drives.iter_mut().find(|d| d.id == drive)?;
        let wanted = if mount && d.mount_point.is_empty() {
            let name = [&d.label, &d.uuid, &d.id]
                .into_iter()
                .find(|n| !n.is_empty())
                .unwrap_or(&d.id);
            root.join(name).as_os_str().as_bytes().to_vec()
        } else if mount {
            d.mount_point.clone()
        } else {
            Vec::new()
        };
        Some((std::mem::replace(&mut d.mount_point, wanted.clone()), wanted))
    }

    /// Without a `state` in `info` the location is disconnected, as the bridge lists one it has not
    /// connected to.
    pub fn add_location(&mut self, id: &str, provider: &str, name: &str, mut info: Vec<(String, InfoValue)>) {
        if !info.iter().any(|(k, _)| k == location_state::KEY) {
            info.push((
                location_state::KEY.to_owned(),
                InfoValue::Str(location_state::DISCONNECTED.into()),
            ));
        }
        self.locations.insert(
            id.to_owned(),
            LocationRec {
                provider: provider.to_owned(),
                name: name.to_owned(),
                info,
            },
        );
        self.trees.entry(id.to_owned()).or_default();
    }

    /// Whether that changed anything.
    pub fn set_state(&mut self, id: &str, state: &str, error: Option<&str>) -> bool {
        let Some(rec) = self.locations.get_mut(id) else {
            return false;
        };
        let value = |key: &str| rec.info.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        let error = error.map(|e| InfoValue::Str(e.to_owned()));
        if value(location_state::KEY) == Some(InfoValue::Str(state.to_owned()))
            && value(location_state::ERROR_KEY) == error
        {
            return false;
        }
        rec.info
            .retain(|(k, _)| k != location_state::KEY && k != location_state::ERROR_KEY);
        rec.info
            .push((location_state::KEY.to_owned(), InfoValue::Str(state.to_owned())));
        if let Some(error) = error {
            rec.info.push((location_state::ERROR_KEY.to_owned(), error));
        }
        true
    }

    pub fn remove_location(&mut self, id: &str) {
        self.locations.remove(id);
        self.trees.remove(id);
    }

    pub fn tree_mut(&mut self, loc: &str) -> &mut Tree {
        self.trees.entry(loc.to_owned()).or_default()
    }

    /// Locations the app may use, each with its level.
    pub fn wire_locations(&self) -> Vec<WireLocation> {
        self.locations
            .iter()
            .filter_map(|(id, rec)| {
                let level = self.level(id)?;
                let mut info: HashMap<String, OwnedValue> = rec
                    .info
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_owned_value()))
                    .collect();
                info.insert(
                    access::KEY.to_owned(),
                    InfoValue::Str(level.as_str().to_owned()).to_owned_value(),
                );
                Some(WireLocation {
                    id: id.clone(),
                    provider: rec.provider.clone(),
                    name: rec.name.clone(),
                    info,
                })
            })
            .collect()
    }

    /// What the app may do in location `id`; `None` while undecided or not allowed.
    pub fn level(&self, id: &str) -> Option<Level> {
        let default = match self.consent {
            Consent::Unknown => return None,
            Consent::Denied => None,
            Consent::Allowed(level) => Some(level),
        };
        self.access.get(id).copied().unwrap_or(default)
    }

    pub fn own(&self, loc: &str, path: &[u8]) -> Own {
        self.own
            .get(&(loc.to_owned(), path.to_vec()))
            .copied()
            .unwrap_or(Own::No)
    }

    /// Records what a finished `call` (`error`: bare name of its failure) made of `path` in `loc`;
    /// nothing at a level that changes anything.
    pub fn after_call(&mut self, loc: &str, path: &[u8], call: After, error: Option<&str>) {
        if self.level(loc).is_none_or(Level::changes) {
            return;
        }
        let own = access::after(self.own(loc, path), call, error);
        let key = (loc.to_owned(), path.to_vec());
        if own == Own::No {
            self.own.remove(&key);
        } else {
            self.own.insert(key, own);
        }
    }

    pub fn due_failure(&mut self, method: &str) -> Option<Failure> {
        let idx = self
            .failures
            .iter()
            .position(|f| (f.method == method || f.method == "*") && f.remaining != Some(0))?;
        let f = &mut self.failures[idx];
        if f.skip > 0 {
            f.skip -= 1;
            return None;
        }
        if let Some(n) = f.remaining.as_mut() {
            *n -= 1;
        }
        Some(f.clone())
    }

    pub fn due_job_failure(&mut self, method: &str) -> Option<JobFailure> {
        let f = self
            .job_failures
            .iter_mut()
            .find(|f| f.method == method && f.remaining != Some(0))?;
        if let Some(n) = f.remaining.as_mut() {
            *n -= 1;
        }
        Some(f.clone())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CancelOutcome {
    Canceled,
    Finished,
    Unknown,
}

pub struct SessionState {
    pub hello: bool,
    next_id: u32,
    next_handle: u32,
    pub handles: HashMap<u32, (String, Vec<u8>)>,
    jobs: HashMap<u32, Arc<AtomicBool>>,
    pub discovering: bool,
}

pub struct Session {
    conn: Mutex<Option<zbus::Connection>>,
    st: Mutex<SessionState>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // a panic while holding the lock cannot leave the data inconsistent here
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Session {
    pub fn new() -> Session {
        Session {
            conn: Mutex::new(None),
            st: Mutex::new(SessionState {
                hello: false,
                next_id: 1,
                next_handle: 1,
                handles: HashMap::new(),
                jobs: HashMap::new(),
                discovering: false,
            }),
        }
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut SessionState) -> R) -> R {
        f(&mut lock(&self.st))
    }

    pub fn set_conn(&self, conn: zbus::Connection) {
        *lock(&self.conn) = Some(conn);
    }

    pub fn conn(&self) -> Option<zbus::Connection> {
        lock(&self.conn).clone()
    }

    pub fn take_conn(&self) -> Option<zbus::Connection> {
        lock(&self.conn).take()
    }

    pub fn open_handles(&self) -> usize {
        self.with(|s| s.handles.len())
    }

    pub fn next_id(&self) -> u32 {
        self.with(|s| {
            let id = s.next_id;
            s.next_id += 1;
            id
        })
    }

    pub fn add_handle(&self, loc: &str, path: &[u8]) -> u32 {
        self.with(|s| {
            let id = s.next_handle;
            s.next_handle += 1;
            s.handles.insert(id, (loc.to_owned(), path.to_vec()));
            id
        })
    }

    pub fn register_job(&self, id: u32) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.with(|s| s.jobs.insert(id, flag.clone()));
        flag
    }

    pub fn finish_job(&self, id: u32) {
        self.with(|s| s.jobs.remove(&id));
    }

    pub fn cancel(&self, id: u32) -> CancelOutcome {
        self.with(|s| match s.jobs.get(&id) {
            Some(flag) => {
                flag.store(true, Ordering::SeqCst);
                CancelOutcome::Canceled
            }
            None if id != 0 && id < s.next_id => CancelOutcome::Finished,
            None => CancelOutcome::Unknown,
        })
    }

    /// Client gone: jobs canceled, handles closed, as the real bridge does.
    pub fn shut(&self) {
        self.with(|s| {
            for flag in s.jobs.values() {
                flag.store(true, Ordering::SeqCst);
            }
            s.handles.clear();
        });
    }
}

pub struct Shared {
    pub(super) state: Mutex<State>,
}

impl Shared {
    pub fn new(state: State) -> Shared {
        Shared {
            state: Mutex::new(state),
        }
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        f(&mut lock(&self.state))
    }

    pub fn new_session(&self) -> Arc<Session> {
        let session = Arc::new(Session::new());
        self.with(|s| s.sessions.push(session.clone()));
        session
    }

    pub fn take_sessions(&self) -> Vec<Arc<Session>> {
        self.with(|s| std::mem::take(&mut s.sessions))
    }

    pub fn sessions_snapshot(&self) -> Vec<Arc<Session>> {
        self.with(|s| s.sessions.clone())
    }

    pub fn any_discovering(&self) -> bool {
        self.sessions_snapshot().iter().any(|s| s.with(|s| s.discovering))
    }

    pub fn record_accept(&self) {
        self.with(|s| s.accepts.push(Instant::now()));
    }

    pub fn accepting(&self) -> bool {
        self.with(|s| s.accepting)
    }

    pub fn record_call(&self, method: &str, lane: &str, target: &str) {
        self.with(|s| {
            s.calls.push(CallRecord {
                method: method.to_owned(),
                lane: lane.to_owned(),
                target: target.to_owned(),
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_skip_then_count_down() {
        let mut st = State::default();
        st.failures
            .push(Failure::new("Stat", "NotFound", "x").after(1).times(2));
        assert!(st.due_failure("Stat").is_none());
        assert!(st.due_failure("List").is_none());
        assert_eq!(st.due_failure("Stat").unwrap().name, "NotFound");
        assert!(st.due_failure("Stat").is_some());
        assert!(st.due_failure("Stat").is_none());
        st.failures
            .push(Failure::new("*", "Locked", "").retry_after(5).detail("d"));
        let f = st.due_failure("Anything").unwrap();
        assert_eq!((f.retry_after_ms, f.detail.as_deref()), (Some(5), Some("d")));
    }

    #[test]
    fn job_failures_are_consumed() {
        let mut st = State::default();
        st.job_failures
            .push(JobFailure::new("Upload", 3, "ConnectionLost", "x").detail("d", Some(9)));
        assert!(st.due_job_failure("Download").is_none());
        let f = st.due_job_failure("Upload").unwrap();
        assert_eq!((f.after_bytes, f.retry_after_ms), (3, Some(9)));
        assert!(st.due_job_failure("Upload").is_none());
    }

    #[test]
    fn session_ids_handles_and_cancel() {
        let s = Session::new();
        assert_eq!(s.next_id(), 1);
        let flag = s.register_job(2);
        assert_eq!(s.cancel(2), CancelOutcome::Canceled);
        assert!(flag.load(Ordering::SeqCst));
        s.finish_job(2);
        assert_eq!(s.cancel(1), CancelOutcome::Finished);
        assert_eq!(s.cancel(0), CancelOutcome::Unknown);
        assert_eq!(s.cancel(77), CancelOutcome::Unknown);
        let h = s.add_handle("account:1", b"f");
        assert_eq!(s.open_handles(), 1);
        assert!(s.with(|st| st.handles.contains_key(&h)));
    }

    #[test]
    fn locations_carry_info() {
        let mut st = State::default();
        st.add_location(
            "account:1",
            "fake",
            "Fake 1",
            vec![("host".into(), InfoValue::Str("h".into()))],
        );
        let w = st.wire_locations();
        assert_eq!(w.len(), 1);
        assert_eq!(crate::wire::value_str(&w[0].info["host"]).as_deref(), Some("h"));
        let state = |st: &State, key: &str| {
            st.wire_locations()[0]
                .info
                .get(key)
                .and_then(|v| crate::wire::value_str(v))
        };
        assert_eq!(state(&st, "state").as_deref(), Some("disconnected"));
        assert!(st.set_state("account:1", "failed", Some("Timeout")));
        assert!(!st.set_state("account:1", "failed", Some("Timeout")));
        assert_eq!(state(&st, "stateError").as_deref(), Some("Timeout"));
        assert!(st.set_state("account:1", "failed", Some("AuthFailed")));
        assert!(st.set_state("account:1", "connected", None));
        assert_eq!(
            (state(&st, "state").as_deref(), state(&st, "stateError")),
            (Some("connected"), None)
        );
        assert!(!st.set_state("account:1", "connected", None));
        assert_eq!(state(&st, "host").as_deref(), Some("h"));
        assert!(!st.set_state("account:9", "connected", None));
        st.remove_location("account:1");
        assert!(st.wire_locations().is_empty());
        assert_eq!(Consent::Denied.as_str(), "denied");
        assert_eq!(Consent::Denied.wire(), "decided");
    }

    #[test]
    fn shutting_a_session_cancels_everything_of_it() {
        let s = Session::new();
        let flag = s.register_job(1);
        s.add_handle("a", b"f");
        s.shut();
        assert!(flag.load(Ordering::SeqCst));
        assert_eq!(s.open_handles(), 0);
    }
}
