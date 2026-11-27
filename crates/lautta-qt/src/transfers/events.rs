// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::runtime::{core, handle};
use lautta_core::transfer::throttle::{forward_throttled, Forwarded};
use lautta_core::transfer::TransferEvent;
use std::cell::RefCell;
use std::sync::Once;
use std::time::Duration;

type Listener = Box<dyn FnMut(&TransferEvent) -> bool>;

thread_local! {
    static LISTENERS: RefCell<Vec<Listener>> = const { RefCell::new(Vec::new()) };
}

static FORWARDER: Once = Once::new();

const CHANGED_EVERY: Duration = Duration::from_millis(250);

pub const RELOAD: i64 = 0;

pub fn is_reload(ev: &TransferEvent) -> bool {
    matches!(ev, TransferEvent::Changed(RELOAD))
}

pub fn listen(f: impl FnMut(&TransferEvent) -> bool + 'static) {
    LISTENERS.with(|l| l.borrow_mut().push(Box::new(f)));
}

pub fn dispatch(ev: &TransferEvent) {
    let mut current = LISTENERS.with(|l| std::mem::take(&mut *l.borrow_mut()));
    current.retain_mut(|f| f(ev));
    LISTENERS.with(|l| {
        let mut added = l.borrow_mut();
        current.append(&mut added);
        *added = current;
    });
}

pub fn reload() {
    qmetaobject::single_shot(std::time::Duration::from_millis(0), || {
        dispatch(&TransferEvent::Changed(RELOAD));
    });
}

pub fn ensure_forwarder() {
    FORWARDER.call_once(|| {
        let Some(core) = core() else { return };
        let rx = core.engine.subscribe();
        let to_gui = qmetaobject::queued_callback(|ev: TransferEvent| dispatch(&ev));
        handle().spawn(forward_throttled(rx, CHANGED_EVERY, move |f| match f {
            Forwarded::Event(ev) => to_gui(ev),
            Forwarded::Lagged => to_gui(TransferEvent::Changed(RELOAD)),
        }));
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
