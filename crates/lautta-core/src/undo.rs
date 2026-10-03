// SPDX-License-Identifier: LGPL-2.1-or-later
//! Undo of the last rename, same-location move and trash (OPS-9).
//!
//! Only the last undoable action is kept, and only for [`UNDO_WINDOW_MS`] (the
//! banner). The recorder never touches the file system: [`undo_steps`] turns
//! an action into inverse steps that the engine and the trash execute.

use crate::ops::names::is_case_only_rename;
use crate::uri::Uri;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// How long the undo banner is offered (OPS-9).
pub const UNDO_WINDOW_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoAction {
    Rename {
        from: Uri,
        to: Uri,
    },
    /// A move inside one location: `(original, new)` pairs.
    Move {
        pairs: Vec<(Uri, Uri)>,
    },
    /// Items sent to Recently deleted, by `trash_items.id`.
    Trash {
        ids: Vec<i64>,
    },
}

/// What the engine or the trash does to take an action back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoStep {
    /// Rename without replacing; fail if `to` exists. `via_temp` for
    /// case-only renames on case-insensitive locations (OPS-3).
    Rename { from: Uri, to: Uri, via_temp: bool },
    /// Restore a trash item to its original place.
    Restore { id: i64 },
}

/// Time source in milliseconds, injectable for tests.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// Monotonic time since the clock was created.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    start: Instant,
}

impl SystemClock {
    pub fn new() -> SystemClock {
        SystemClock {
            start: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// A clock that only moves when told to.
#[derive(Debug, Clone, Default)]
pub struct ManualClock {
    now: Arc<AtomicU64>,
}

impl ManualClock {
    pub fn advance(&self, ms: u64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
}

#[derive(Clone)]
pub struct UndoRecorder {
    clock: Arc<dyn Clock>,
    last: Option<(UndoAction, u64)>,
}

impl std::fmt::Debug for UndoRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UndoRecorder").field("last", &self.last).finish()
    }
}

impl Default for UndoRecorder {
    fn default() -> Self {
        UndoRecorder::with_clock(Arc::new(SystemClock::new()))
    }
}

impl UndoRecorder {
    pub fn with_clock(clock: Arc<dyn Clock>) -> UndoRecorder {
        UndoRecorder { clock, last: None }
    }

    /// Remembers `action`, replacing any earlier one.
    pub fn record(&mut self, action: UndoAction) {
        let now = self.clock.now_ms();
        self.record_at(action, now);
    }

    pub fn record_at(&mut self, action: UndoAction, now_ms: u64) {
        self.last = Some((action, now_ms));
    }

    pub fn clear(&mut self) {
        self.last = None;
    }

    /// The action, if recorded less than [`UNDO_WINDOW_MS`] before `now_ms`;
    /// it is consumed either way.
    pub fn take_if_fresh(&mut self, now_ms: u64) -> Option<UndoAction> {
        let (action, at) = self.last.take()?;
        (now_ms.saturating_sub(at) < UNDO_WINDOW_MS).then_some(action)
    }

    /// [`take_if_fresh`](Self::take_if_fresh) with the recorder's clock.
    pub fn take(&mut self) -> Option<UndoAction> {
        let now = self.clock.now_ms();
        self.take_if_fresh(now)
    }

    /// Whether the banner should still be shown at `now_ms`.
    pub fn is_fresh(&self, now_ms: u64) -> bool {
        matches!(&self.last, Some((_, at)) if now_ms.saturating_sub(*at) < UNDO_WINDOW_MS)
    }

    pub fn peek(&self) -> Option<&UndoAction> {
        self.last.as_ref().map(|(a, _)| a)
    }
}

fn reverse_rename(from: &Uri, to: &Uri) -> UndoStep {
    let case_only = match (from.name(), to.name()) {
        (Some(a), Some(b)) => from.parent() == to.parent() && is_case_only_rename(a, b),
        _ => false,
    };
    UndoStep::Rename {
        from: to.clone(),
        to: from.clone(),
        via_temp: case_only,
    }
}

/// The inverse of `action`, in the order to execute it (a batch move is
/// undone last item first).
pub fn undo_steps(action: &UndoAction) -> Vec<UndoStep> {
    match action {
        UndoAction::Rename { from, to } => vec![reverse_rename(from, to)],
        UndoAction::Move { pairs } => pairs
            .iter()
            .rev()
            .map(|(from, to)| reverse_rename(from, to))
            .collect(),
        UndoAction::Trash { ids } => ids.iter().map(|id| UndoStep::Restore { id: *id }).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vpath::VPath;

    fn uri(path: &str) -> Uri {
        Uri::new("l", VPath::parse(path.as_bytes()).unwrap())
    }

    fn rename() -> UndoAction {
        UndoAction::Rename {
            from: uri("a/old"),
            to: uri("a/new"),
        }
    }

    fn recorder() -> (UndoRecorder, ManualClock) {
        let clock = ManualClock::default();
        (UndoRecorder::with_clock(Arc::new(clock.clone())), clock)
    }

    fn step(from: &str, to: &str, via_temp: bool) -> UndoStep {
        UndoStep::Rename {
            from: uri(from),
            to: uri(to),
            via_temp,
        }
    }

    #[test]
    fn fresh_for_ten_seconds_exclusive() {
        let (mut r, clock) = recorder();
        clock.advance(500);
        r.record(rename());
        assert!(r.is_fresh(500 + 9_999));
        assert!(!r.is_fresh(500 + 10_000));
        assert_eq!(r.take_if_fresh(500 + 9_999), Some(rename()));
        r.record(rename());
        assert_eq!(r.take_if_fresh(500 + 10_000), None);
    }

    #[test]
    fn take_consumes_and_clock_is_used() {
        let (mut r, clock) = recorder();
        r.record(rename());
        clock.advance(9_000);
        assert_eq!(r.peek(), Some(&rename()));
        assert_eq!(r.take(), Some(rename()));
        assert_eq!(r.take(), None);
        assert_eq!(r.peek(), None);
        r.record(rename());
        clock.advance(10_000);
        assert_eq!(r.take(), None);
    }

    #[test]
    fn only_the_last_action_is_kept() {
        let (mut r, _) = recorder();
        r.record(rename());
        r.record(UndoAction::Trash { ids: vec![1] });
        assert_eq!(r.take(), Some(UndoAction::Trash { ids: vec![1] }));
        assert_eq!(r.take(), None);
    }

    #[test]
    fn clear_forgets() {
        let (mut r, _) = recorder();
        r.record(rename());
        r.clear();
        assert!(!r.is_fresh(0));
        assert_eq!(r.take(), None);
    }

    #[test]
    fn clock_going_backwards_counts_as_fresh() {
        let mut r = UndoRecorder::default();
        r.record_at(rename(), 5_000);
        assert_eq!(r.take_if_fresh(1_000), Some(rename()));
    }

    #[test]
    fn system_clock_advances() {
        let c = SystemClock::default();
        let a = c.now_ms();
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(c.now_ms() > a);
        assert!(format!("{:?}", UndoRecorder::default()).contains("UndoRecorder"));
    }

    #[test]
    fn rename_inverse() {
        assert_eq!(undo_steps(&rename()), vec![step("a/new", "a/old", false)]);
    }

    #[test]
    fn case_only_rename_goes_through_a_temp_name() {
        let a = UndoAction::Rename {
            from: uri("a/photo.jpg"),
            to: uri("a/Photo.jpg"),
        };
        assert_eq!(undo_steps(&a), vec![step("a/Photo.jpg", "a/photo.jpg", true)]);
        let elsewhere = UndoAction::Rename {
            from: uri("a/photo.jpg"),
            to: uri("b/Photo.jpg"),
        };
        assert_eq!(
            undo_steps(&elsewhere),
            vec![step("b/Photo.jpg", "a/photo.jpg", false)]
        );
    }

    #[test]
    fn move_inverse_runs_in_reverse_order() {
        let a = UndoAction::Move {
            pairs: vec![(uri("a/x"), uri("b/x")), (uri("a/y"), uri("b/y"))],
        };
        assert_eq!(
            undo_steps(&a),
            vec![step("b/y", "a/y", false), step("b/x", "a/x", false)]
        );
    }

    #[test]
    fn trash_inverse_restores_each_id() {
        let steps = undo_steps(&UndoAction::Trash { ids: vec![3, 4] });
        assert_eq!(
            steps,
            vec![UndoStep::Restore { id: 3 }, UndoStep::Restore { id: 4 }]
        );
        assert!(undo_steps(&UndoAction::Trash { ids: vec![] }).is_empty());
    }
}
