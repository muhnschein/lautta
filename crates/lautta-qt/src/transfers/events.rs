// SPDX-License-Identifier: LGPL-2.1-or-later
//! One forwarding task carries engine `TransferEvent`s to the GUI thread (queued callback), then to
//! the `Transfers` singleton and live models via a listener registry.

use crate::runtime::{core, handle};
use lautta_core::transfer::TransferEvent;
use std::cell::RefCell;
use std::sync::Once;
use tokio::sync::broadcast::error::RecvError;

type Listener = Box<dyn FnMut(&TransferEvent) -> bool>;

thread_local! {
    static LISTENERS: RefCell<Vec<Listener>> = const { RefCell::new(Vec::new()) };
}

static FORWARDER: Once = Once::new();

/// Synthetic "something else changed, reload" event (history cleared, start-up restore, missed
/// events).
pub const RELOAD: i64 = 0;

pub fn is_reload(ev: &TransferEvent) -> bool {
    matches!(ev, TransferEvent::Changed(RELOAD))
}

pub fn listen(f: impl FnMut(&TransferEvent) -> bool + 'static) {
    LISTENERS.with(|l| l.borrow_mut().push(Box::new(f)));
}

/// Listeners added meanwhile are kept.
pub fn dispatch(ev: &TransferEvent) {
    let mut current = LISTENERS.with(|l| std::mem::take(&mut *l.borrow_mut()));
    current.retain_mut(|f| f(ev));
    LISTENERS.with(|l| {
        let mut added = l.borrow_mut();
        current.append(&mut added);
        *added = current;
    });
}

/// Next event-loop turn: callers are often inside a borrow of a listener.
pub fn reload() {
    qmetaobject::single_shot(std::time::Duration::from_millis(0), || {
        dispatch(&TransferEvent::Changed(RELOAD));
    });
}

/// Once. Call on the GUI thread.
pub fn ensure_forwarder() {
    FORWARDER.call_once(|| {
        let Some(core) = core() else { return };
        let mut rx = core.engine.subscribe();
        let to_gui = qmetaobject::queued_callback(|ev: TransferEvent| dispatch(&ev));
        handle().spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ev) => to_gui(ev),
                    Err(RecvError::Lagged(_)) => to_gui(TransferEvent::Changed(RELOAD)),
                    Err(RecvError::Closed) => break,
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn listeners_see_events_and_are_dropped_when_gone() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s = seen.clone();
        listen(move |ev| {
            s.borrow_mut().push(ev.clone());
            s.borrow().len() < 2
        });
        let s2 = seen.clone();
        listen(move |_| {
            s2.borrow_mut().push(TransferEvent::Busy(true));
            true
        });
        dispatch(&TransferEvent::Added(1));
        dispatch(&TransferEvent::Added(2));
        dispatch(&TransferEvent::Added(3));
        let got = seen.borrow().clone();
        let added = got
            .iter()
            .filter(|e| matches!(e, TransferEvent::Added(_)))
            .count();
        let busy = got
            .iter()
            .filter(|e| matches!(e, TransferEvent::Busy(true)))
            .count();
        assert_eq!(added, 2, "the first listener left after two events");
        assert_eq!(busy, 3, "the second stayed");
        assert!(is_reload(&TransferEvent::Changed(RELOAD)));
        assert!(!is_reload(&TransferEvent::Changed(5)));
        LISTENERS.with(|l| l.borrow_mut().clear());
    }
}
