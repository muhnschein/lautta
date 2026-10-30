// SPDX-License-Identifier: LGPL-2.1-or-later
//! Routes signals to the waiting request. A signal can arrive before the method reply naming its id
//! is processed (`ListBatch` right behind `List`): slots are created by whichever side comes first,
//! events wait until the request subscribes.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};
use tokio::sync::mpsc;

/// Slots for ids nobody has subscribed to yet; older ones are dropped.
const MAX_BUFFERED_IDS: usize = 128;
/// Requests that gave up: late signals are ignored.
const MAX_IGNORED_IDS: usize = 256;

enum Slot<T> {
    Waiting(Vec<T>),
    Open(mpsc::UnboundedSender<T>),
}

struct Inner<T> {
    slots: HashMap<u32, Slot<T>>,
    ignored: VecDeque<u32>,
    closed: bool,
}

pub struct Registry<T> {
    inner: Mutex<Inner<T>>,
}

impl<T> Default for Registry<T> {
    fn default() -> Registry<T> {
        Registry {
            inner: Mutex::new(Inner {
                slots: HashMap::new(),
                ignored: VecDeque::new(),
                closed: false,
            }),
        }
    }
}

impl<T> Registry<T> {
    fn lock(&self) -> MutexGuard<'_, Inner<T>> {
        // Every update is one step, so data stays consistent if a holder panicked.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn push(&self, id: u32, event: T) {
        let mut g = self.lock();
        if g.closed || g.ignored.contains(&id) {
            return;
        }
        match g.slots.get_mut(&id) {
            Some(Slot::Open(tx)) => {
                if tx.send(event).is_err() {
                    g.slots.remove(&id);
                }
            }
            Some(Slot::Waiting(buffer)) => buffer.push(event),
            None => {
                g.slots.insert(id, Slot::Waiting(vec![event]));
                if g.slots.len() > MAX_BUFFERED_IDS {
                    let oldest = g
                        .slots
                        .iter()
                        .filter(|(_, s)| matches!(s, Slot::Waiting(_)))
                        .map(|(k, _)| *k)
                        .min();
                    if let Some(k) = oldest {
                        g.slots.remove(&k);
                    }
                }
            }
        }
    }

    pub fn subscribe(&self, id: u32) -> mpsc::UnboundedReceiver<T> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut g = self.lock();
        if g.closed {
            return rx;
        }
        if let Some(Slot::Waiting(buffer)) = g.slots.remove(&id) {
            for event in buffer {
                let _ = tx.send(event);
            }
        }
        g.slots.insert(id, Slot::Open(tx));
        rx
    }

    pub fn forget(&self, id: u32) {
        let mut g = self.lock();
        g.slots.remove(&id);
        g.ignored.push_back(id);
        if g.ignored.len() > MAX_IGNORED_IDS {
            g.ignored.pop_front();
        }
    }

    pub fn close(&self) {
        let mut g = self.lock();
        g.closed = true;
        g.slots.clear();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.lock().slots.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_before_subscription_are_kept_in_order() {
        let r: Registry<u32> = Registry::default();
        r.push(7, 1);
        r.push(7, 2);
        let mut rx = r.subscribe(7);
        r.push(7, 3);
        assert_eq!(rx.try_recv().unwrap(), 1);
        assert_eq!(rx.try_recv().unwrap(), 2);
        assert_eq!(rx.try_recv().unwrap(), 3);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn ids_are_independent() {
        let r: Registry<&str> = Registry::default();
        let mut a = r.subscribe(1);
        let mut b = r.subscribe(2);
        r.push(2, "b");
        r.push(1, "a");
        assert_eq!(a.try_recv().unwrap(), "a");
        assert_eq!(b.try_recv().unwrap(), "b");
    }

    #[test]
    fn forgotten_ids_drop_late_events() {
        let r: Registry<u32> = Registry::default();
        let _rx = r.subscribe(1);
        r.forget(1);
        r.push(1, 9);
        assert_eq!(r.len(), 0);
        let mut again = r.subscribe(1);
        assert!(again.try_recv().is_err());
    }

    #[test]
    fn closing_ends_receivers_and_refuses_new_ones() {
        let r: Registry<u32> = Registry::default();
        let mut rx = r.subscribe(1);
        r.close();
        assert!(matches!(
            rx.try_recv(),
            Err(mpsc::error::TryRecvError::Disconnected)
        ));
        r.push(1, 1);
        let mut late = r.subscribe(2);
        assert!(matches!(
            late.try_recv(),
            Err(mpsc::error::TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn unclaimed_ids_are_bounded() {
        let r: Registry<u32> = Registry::default();
        for id in 0..(MAX_BUFFERED_IDS as u32 + 10) {
            r.push(id, id);
        }
        assert_eq!(r.len(), MAX_BUFFERED_IDS);
        let mut newest = r.subscribe(MAX_BUFFERED_IDS as u32 + 9);
        assert_eq!(newest.try_recv().unwrap(), MAX_BUFFERED_IDS as u32 + 9);
        let mut oldest = r.subscribe(0);
        assert!(oldest.try_recv().is_err());
    }

    #[test]
    fn a_dropped_receiver_does_not_stick() {
        let r: Registry<u32> = Registry::default();
        drop(r.subscribe(1));
        r.push(1, 1);
        assert_eq!(r.len(), 0);
    }
}
