// SPDX-License-Identifier: LGPL-2.1-or-later
//! Scheduling (XFR-2): FIFO per destination location, round-robin across
//! locations, concurrency limits per location. Pure logic: no I/O, no clock,
//! no randomness, so every decision is reproducible in tests.

use super::model::TransferId;
use std::collections::{HashMap, VecDeque};

/// Concurrent items for a local destination (XFR-2).
pub const LOCAL_LIMIT: usize = 2;
/// Default for a remote location; the user may choose 1 to 6.
pub const DEFAULT_REMOTE_LIMIT: usize = 2;
pub const MIN_REMOTE_LIMIT: usize = 1;
pub const MAX_REMOTE_LIMIT: usize = 6;

/// Bridge-served locations: `nv-<bridge id>` (LOC-7).
pub fn is_bridge_location(location: &str) -> bool {
    location.starts_with("nv-")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemRef {
    pub transfer: TransferId,
    pub seq: u32,
}

#[derive(Debug, Clone)]
struct Entry {
    id: TransferId,
    location: String,
    runnable: bool,
    priority: bool,
    /// Items not yet started, in plan order, each with its barrier flag.
    pending: VecDeque<(u32, bool)>,
    running: u32,
    barrier_running: bool,
}

/// Working state while computing one batch.
#[derive(Debug, Clone, Copy, Default)]
struct Sim {
    taken: usize,
    running: u32,
    barrier_running: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Scheduler {
    entries: Vec<Entry>,
    remote_limits: HashMap<String, usize>,
    /// Optional cap on running items over all locations; without it the
    /// round-robin only interleaves the start order.
    global_limit: Option<usize>,
    /// Index into the location rotation where the next batch starts.
    cursor: usize,
}

impl Scheduler {
    pub fn new() -> Scheduler {
        Scheduler::default()
    }

    /// Sets the concurrency of a remote location, clamped to 1..=6.
    pub fn set_remote_limit(&mut self, location: &str, limit: usize) {
        self.remote_limits.insert(
            location.to_owned(),
            limit.clamp(MIN_REMOTE_LIMIT, MAX_REMOTE_LIMIT),
        );
    }

    pub fn set_global_limit(&mut self, limit: Option<usize>) {
        self.global_limit = limit.map(|l| l.max(1));
    }

    pub fn limit_for(&self, location: &str) -> usize {
        if !is_bridge_location(location) {
            return LOCAL_LIMIT;
        }
        self.remote_limits
            .get(location)
            .copied()
            .unwrap_or(DEFAULT_REMOTE_LIMIT)
    }

    /// Queues a transfer. A `high_priority` one (write-back, EDT-2) goes
    /// ahead of ordinary transfers but behind earlier high-priority ones.
    /// `items` are `(seq, barrier)`: a barrier (folder, link, delete) runs
    /// alone, after everything before it, and blocks what follows until done.
    pub fn add(
        &mut self,
        id: TransferId,
        location: &str,
        items: impl IntoIterator<Item = (u32, bool)>,
        high_priority: bool,
    ) {
        self.remove(id);
        let entry = Entry {
            id,
            location: location.to_owned(),
            runnable: true,
            priority: high_priority,
            pending: items.into_iter().collect(),
            running: 0,
            barrier_running: false,
        };
        let at = if high_priority {
            self.entries.iter().take_while(|e| e.priority).count()
        } else {
            self.entries.len()
        };
        self.entries.insert(at, entry);
    }

    pub fn remove(&mut self, id: TransferId) {
        self.entries.retain(|e| e.id != id);
    }

    pub fn contains(&self, id: TransferId) -> bool {
        self.entries.iter().any(|e| e.id == id)
    }

    pub fn set_runnable(&mut self, id: TransferId, runnable: bool) {
        if let Some(e) = self.entry_mut(id) {
            e.runnable = runnable;
        }
    }

    /// Re-queues items (retry, XFR-9), keeping plan order.
    pub fn requeue(&mut self, id: TransferId, items: impl IntoIterator<Item = (u32, bool)>) {
        if let Some(e) = self.entry_mut(id) {
            for item in items {
                if e.pending.iter().any(|p| p.0 == item.0) {
                    continue;
                }
                let at = e.pending.iter().take_while(|p| p.0 < item.0).count();
                e.pending.insert(at, item);
            }
        }
    }

    /// Drops not-yet-started items (skipped subtrees, answered "skip").
    pub fn drop_pending(&mut self, id: TransferId, keep: impl Fn(u32) -> bool) {
        if let Some(e) = self.entry_mut(id) {
            e.pending.retain(|p| keep(p.0));
        }
    }

    pub fn pending_count(&self, id: TransferId) -> usize {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .map_or(0, |e| e.pending.len())
    }

    pub fn running_count(&self, id: TransferId) -> usize {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .map_or(0, |e| e.running as usize)
    }

    pub fn running_in(&self, location: &str) -> usize {
        self.entries
            .iter()
            .filter(|e| e.location == location)
            .map(|e| e.running as usize)
            .sum()
    }

    /// Queue order, front first.
    pub fn order(&self) -> Vec<TransferId> {
        self.entries.iter().map(|e| e.id).collect()
    }

    pub fn move_up(&mut self, id: TransferId) -> bool {
        match self.entries.iter().position(|e| e.id == id) {
            Some(i) if i > 0 => {
                self.entries.swap(i, i - 1);
                true
            }
            _ => false,
        }
    }

    pub fn move_down(&mut self, id: TransferId) -> bool {
        match self.entries.iter().position(|e| e.id == id) {
            Some(i) if i + 1 < self.entries.len() => {
                self.entries.swap(i, i + 1);
                true
            }
            _ => false,
        }
    }

    pub fn move_to_top(&mut self, id: TransferId) -> bool {
        match self.entries.iter().position(|e| e.id == id) {
            Some(i) if i > 0 => {
                let e = self.entries.remove(i);
                self.entries.insert(0, e);
                true
            }
            _ => false,
        }
    }

    pub fn item_finished(&mut self, item: ItemRef) {
        if let Some(e) = self.entry_mut(item.transfer) {
            e.running = e.running.saturating_sub(1);
            if e.running == 0 {
                e.barrier_running = false;
            }
        }
    }

    /// What `start_runnable` would start now, without changing anything.
    pub fn next_runnable(&self) -> Vec<ItemRef> {
        self.compute().0.into_iter().map(|(item, _)| item).collect()
    }

    /// Picks the items to start, marks them running and advances the
    /// round-robin cursor.
    pub fn start_runnable(&mut self) -> Vec<ItemRef> {
        let (picked, cursor) = self.compute();
        self.cursor = cursor;
        for (item, barrier) in &picked {
            if let Some(e) = self.entry_mut(item.transfer) {
                e.pending.retain(|p| p.0 != item.seq);
                e.running += 1;
                e.barrier_running = *barrier;
            }
        }
        picked.into_iter().map(|(item, _)| item).collect()
    }

    fn entry_mut(&mut self, id: TransferId) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    fn locations(&self) -> Vec<&str> {
        let mut seen: Vec<&str> = Vec::new();
        for e in &self.entries {
            if !seen.contains(&e.location.as_str()) {
                seen.push(&e.location);
            }
        }
        seen
    }

    fn compute(&self) -> (Vec<(ItemRef, bool)>, usize) {
        let locations = self.locations();
        let n = locations.len();
        if n == 0 {
            return (Vec::new(), 0);
        }
        let start = self.cursor % n;
        let mut sims: Vec<Sim> = self
            .entries
            .iter()
            .map(|e| Sim {
                taken: 0,
                running: e.running,
                barrier_running: e.barrier_running,
            })
            .collect();
        let mut load: HashMap<&str, usize> = locations.iter().map(|l| (*l, self.running_in(l))).collect();
        let mut picked = Vec::new();
        let mut last = None;
        let mut total: usize = self.entries.iter().map(|e| e.running as usize).sum();
        // Each round that is not the last picks at least one pending item, so
        // the rounds are bounded (a mistake here then fails tests, not hangs).
        let rounds = self.entries.iter().map(|e| e.pending.len()).sum::<usize>() + 1;
        for _ in 0..rounds {
            let before = picked.len();
            for k in 0..n {
                let li = (start + k) % n;
                let loc = locations[li];
                let in_use = load.get(loc).copied().unwrap_or(0);
                if in_use >= self.limit_for(loc) || self.global_limit.is_some_and(|g| total >= g) {
                    continue;
                }
                if let Some(item) = self.pick_from(loc, &mut sims) {
                    load.insert(loc, in_use + 1);
                    total += 1;
                    picked.push(item);
                    last = Some(li);
                }
            }
            if picked.len() == before {
                break;
            }
        }
        (picked, last.map_or(self.cursor, |l| (l + 1) % n))
    }

    /// The first startable item of the first transfer of `location` that has
    /// one (FIFO within the location).
    fn pick_from(&self, location: &str, sims: &mut [Sim]) -> Option<(ItemRef, bool)> {
        for (i, e) in self.entries.iter().enumerate() {
            if e.location != location || !e.runnable {
                continue;
            }
            let sim = &mut sims[i];
            let Some(&(seq, barrier)) = e.pending.get(sim.taken) else {
                continue;
            };
            if sim.barrier_running || (barrier && sim.running > 0) {
                continue;
            }
            sim.taken += 1;
            sim.running += 1;
            sim.barrier_running = barrier;
            return Some((ItemRef { transfer: e.id, seq }, barrier));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(n: u32) -> Vec<(u32, bool)> {
        (0..n).map(|i| (i, false)).collect()
    }

    fn r(transfer: TransferId, seq: u32) -> ItemRef {
        ItemRef { transfer, seq }
    }

    #[test]
    fn local_limit_is_two() {
        let mut s = Scheduler::new();
        s.add(1, "user-documents", files(5), false);
        assert_eq!(s.start_runnable(), vec![r(1, 0), r(1, 1)]);
        assert!(s.start_runnable().is_empty());
        s.item_finished(r(1, 0));
        assert_eq!(s.start_runnable(), vec![r(1, 2)]);
        assert_eq!(s.running_in("user-documents"), 2);
        assert_eq!(s.pending_count(1), 2);
        assert_eq!(s.running_count(1), 2);
    }

    #[test]
    fn remote_limit_default_override_and_clamp() {
        let mut s = Scheduler::new();
        s.add(1, "nv-a", files(10), false);
        assert_eq!(s.limit_for("nv-a"), 2);
        assert_eq!(s.next_runnable().len(), 2);
        s.set_remote_limit("nv-a", 4);
        assert_eq!(s.next_runnable().len(), 4);
        s.set_remote_limit("nv-a", 99);
        assert_eq!(s.limit_for("nv-a"), 6);
        s.set_remote_limit("nv-a", 0);
        assert_eq!(s.limit_for("nv-a"), 1);
        assert_eq!(s.limit_for("vol-1"), 2);
        s.set_remote_limit("vol-1", 6);
        assert_eq!(s.limit_for("vol-1"), 2);
    }

    #[test]
    fn next_runnable_does_not_mutate() {
        let mut s = Scheduler::new();
        s.add(1, "a", files(3), false);
        let first = s.next_runnable();
        assert_eq!(first, s.next_runnable());
        assert_eq!(s.running_count(1), 0);
        assert_eq!(s.start_runnable(), first);
    }

    #[test]
    fn fifo_within_a_location() {
        let mut s = Scheduler::new();
        s.add(1, "a", files(1), false);
        s.add(2, "a", files(3), false);
        assert_eq!(s.start_runnable(), vec![r(1, 0), r(2, 0)]);
        s.item_finished(r(1, 0));
        assert_eq!(s.start_runnable(), vec![r(2, 1)]);
    }

    #[test]
    fn round_robin_across_locations() {
        let mut s = Scheduler::new();
        s.add(1, "a", files(4), false);
        s.add(2, "nv-b", files(4), false);
        s.add(3, "c", files(4), false);
        let order: Vec<TransferId> = s.start_runnable().iter().map(|i| i.transfer).collect();
        assert_eq!(order, vec![1, 2, 3, 1, 2, 3]);
    }

    #[test]
    fn busy_location_does_not_starve_others() {
        let mut s = Scheduler::new();
        s.add(1, "a", files(10), false);
        s.add(2, "nv-b", files(10), false);
        assert_eq!(s.start_runnable().len(), 4);
        s.item_finished(r(1, 0));
        assert_eq!(s.start_runnable(), vec![r(1, 2)]);
        s.item_finished(r(2, 0));
        assert_eq!(s.start_runnable(), vec![r(2, 2)]);
    }

    #[test]
    fn global_cap_rotates_the_first_served_location() {
        let mut s = Scheduler::new();
        s.set_global_limit(Some(3));
        s.add(1, "a", files(9), false);
        s.add(2, "c", files(9), false);
        let first: Vec<TransferId> = s.start_runnable().iter().map(|i| i.transfer).collect();
        assert_eq!(first, vec![1, 2, 1]);
        s.item_finished(r(1, 0));
        s.item_finished(r(2, 0));
        let second: Vec<TransferId> = s.start_runnable().iter().map(|i| i.transfer).collect();
        assert_eq!(second, vec![2, 1]);
        assert!(s.start_runnable().is_empty());
        s.set_global_limit(None);
        assert_eq!(s.start_runnable().len(), 1);
    }

    #[test]
    fn barriers_run_alone_and_in_order() {
        let mut s = Scheduler::new();
        s.add(
            1,
            "a",
            vec![(0, true), (1, false), (2, false), (3, true), (4, false)],
            false,
        );
        assert_eq!(s.start_runnable(), vec![r(1, 0)]);
        assert!(s.start_runnable().is_empty());
        s.item_finished(r(1, 0));
        assert_eq!(s.start_runnable(), vec![r(1, 1), r(1, 2)]);
        assert!(s.start_runnable().is_empty());
        s.item_finished(r(1, 1));
        assert!(s.start_runnable().is_empty());
        s.item_finished(r(1, 2));
        assert_eq!(s.start_runnable(), vec![r(1, 3)]);
        s.item_finished(r(1, 3));
        assert_eq!(s.start_runnable(), vec![r(1, 4)]);
    }

    #[test]
    fn barrier_waits_for_earlier_items_started_in_the_same_batch() {
        let mut s = Scheduler::new();
        s.add(1, "a", vec![(0, false), (1, true), (2, false)], false);
        assert_eq!(s.start_runnable(), vec![r(1, 0)]);
        s.item_finished(r(1, 0));
        assert_eq!(s.start_runnable(), vec![r(1, 1)]);
    }

    #[test]
    fn barrier_in_one_transfer_does_not_block_another() {
        let mut s = Scheduler::new();
        s.add(1, "a", vec![(0, true), (1, false)], false);
        s.add(2, "a", files(2), false);
        assert_eq!(s.start_runnable(), vec![r(1, 0), r(2, 0)]);
    }

    #[test]
    fn not_runnable_transfers_are_skipped() {
        let mut s = Scheduler::new();
        s.add(1, "a", files(2), false);
        s.add(2, "a", files(2), false);
        s.set_runnable(1, false);
        assert_eq!(s.start_runnable(), vec![r(2, 0), r(2, 1)]);
        s.set_runnable(1, true);
        s.item_finished(r(2, 0));
        assert_eq!(s.start_runnable(), vec![r(1, 0)]);
    }

    #[test]
    fn reorder_changes_service_order() {
        let mut s = Scheduler::new();
        for id in 1..=3 {
            s.add(id, "a", files(1), false);
        }
        assert!(s.move_to_top(3));
        assert_eq!(s.order(), vec![3, 1, 2]);
        assert!(s.move_down(3));
        assert_eq!(s.order(), vec![1, 3, 2]);
        assert!(s.move_up(2));
        assert_eq!(s.order(), vec![1, 2, 3]);
        assert!(!s.move_up(1));
        assert!(!s.move_down(3));
        assert!(!s.move_to_top(1));
        assert!(!s.move_up(99) && !s.move_down(99) && !s.move_to_top(99));
        s.move_to_top(2);
        assert_eq!(s.start_runnable(), vec![r(2, 0), r(1, 0)]);
    }

    #[test]
    fn high_priority_goes_first_but_keeps_its_own_order() {
        let mut s = Scheduler::new();
        s.add(5, "a", files(1), false);
        s.add(6, "a", files(1), true);
        s.add(7, "a", files(1), true);
        assert_eq!(s.order(), vec![6, 7, 5]);
    }

    #[test]
    fn requeue_keeps_plan_order_and_ignores_duplicates() {
        let mut s = Scheduler::new();
        s.add(1, "a", vec![(5, false)], false);
        s.requeue(1, vec![(2, false), (9, false), (5, false), (7, false)]);
        s.set_remote_limit("a", 1);
        assert_eq!(s.pending_count(1), 4);
        let started: Vec<u32> = s.start_runnable().iter().map(|i| i.seq).collect();
        assert_eq!(started, vec![2, 5]);
        s.requeue(99, files(1));
    }

    #[test]
    fn drop_pending_and_remove() {
        let mut s = Scheduler::new();
        s.add(1, "a", files(4), false);
        s.drop_pending(1, |seq| seq >= 2);
        assert_eq!(s.pending_count(1), 2);
        assert!(s.contains(1));
        s.remove(1);
        assert!(!s.contains(1));
        assert!(s.start_runnable().is_empty());
        assert_eq!(s.pending_count(1), 0);
        assert_eq!(s.running_count(1), 0);
    }

    #[test]
    fn readding_replaces_the_entry() {
        let mut s = Scheduler::new();
        s.add(1, "a", files(1), false);
        s.add(1, "a", files(3), false);
        assert_eq!(s.order(), vec![1]);
        assert_eq!(s.pending_count(1), 3);
    }

    #[test]
    fn finishing_unknown_item_is_harmless() {
        let mut s = Scheduler::new();
        s.item_finished(r(1, 1));
        s.add(1, "a", files(1), false);
        s.item_finished(r(1, 0));
        assert_eq!(s.running_count(1), 0);
        assert_eq!(s.start_runnable(), vec![r(1, 0)]);
    }
}
