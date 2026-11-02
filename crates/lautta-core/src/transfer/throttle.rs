// SPDX-License-Identifier: LGPL-2.1-or-later
//! Engine events on their way to the GUI: `Changed` per transfer at most once per window, since
//! each makes the pages re-read every item (a 5,000-file copy sends one per file).

use super::{TransferEvent, TransferId};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::time::Instant;

/// What `forward_throttled` hands on: an event, or `Lagged` when the receiver missed some.
#[derive(Debug, Clone, PartialEq)]
pub enum Forwarded {
    Event(TransferEvent),
    Lagged,
}

/// First `Changed(id)` at once, later ones of that id held until `every` has passed; a held one
/// goes out then. Other events pass at once, in order.
pub async fn forward_throttled(
    mut rx: broadcast::Receiver<TransferEvent>,
    every: Duration,
    mut emit: impl FnMut(Forwarded),
) {
    let mut last: HashMap<TransferId, Instant> = HashMap::new();
    let mut held: Vec<TransferId> = Vec::new();
    loop {
        let due = held
            .iter()
            .filter_map(|id| last.get(id))
            .min()
            .map(|t| *t + every);
        let next = match due {
            Some(at) => tokio::time::timeout_at(at, rx.recv()).await.ok(),
            None => Some(rx.recv().await),
        };
        let now = Instant::now();
        match next {
            None => {
                held.retain(|id| {
                    let ready = last.get(id).is_none_or(|t| *t + every <= now);
                    if ready {
                        last.insert(*id, now);
                        emit(Forwarded::Event(TransferEvent::Changed(*id)));
                    }
                    !ready
                });
            }
            Some(Ok(TransferEvent::Changed(id))) => match last.get(&id) {
                Some(t) if *t + every > now => {
                    if !held.contains(&id) {
                        held.push(id);
                    }
                }
                _ => {
                    last.insert(id, now);
                    emit(Forwarded::Event(TransferEvent::Changed(id)));
                }
            },
            Some(Ok(ev)) => emit(Forwarded::Event(ev)),
            Some(Err(RecvError::Lagged(_))) => emit(Forwarded::Lagged),
            Some(Err(RecvError::Closed)) => {
                for id in held.drain(..) {
                    emit(Forwarded::Event(TransferEvent::Changed(id)));
                }
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn changes_of_one_transfer_come_once_per_window() {
        let (tx, rx) = broadcast::channel(64);
        let (out_tx, mut out) = tokio::sync::mpsc::unbounded_channel();
        let start = Instant::now();
        let every = Duration::from_millis(200);
        let fwd = tokio::spawn(forward_throttled(rx, every, move |f| {
            let _ = out_tx.send(((Instant::now() - start).as_millis(), f));
        }));
        let changed = |id| Forwarded::Event(TransferEvent::Changed(id));
        for _ in 0..50 {
            tx.send(TransferEvent::Changed(1)).unwrap();
        }
        tx.send(TransferEvent::Changed(2)).unwrap();
        tx.send(TransferEvent::Finished(1)).unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        tx.send(TransferEvent::Changed(1)).unwrap();
        tx.send(TransferEvent::Changed(1)).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(tx);
        fwd.await.unwrap();
        let mut got = Vec::new();
        while let Some(x) = out.recv().await {
            got.push(x);
        }
        assert_eq!(
            got,
            [
                (0, changed(1)),
                (0, changed(2)),
                (0, Forwarded::Event(TransferEvent::Finished(1))),
                (200, changed(1)),
                (500, changed(1)),
                (550, changed(1)),
            ],
            "held change goes out when its window ends; closing flushes the rest"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_lagging_receiver_says_so() {
        let (tx, rx) = broadcast::channel(2);
        for i in 0..5 {
            tx.send(TransferEvent::Added(i)).unwrap();
        }
        drop(tx);
        let mut got = Vec::new();
        forward_throttled(rx, Duration::from_millis(200), |f| got.push(f)).await;
        assert_eq!(
            got,
            [
                Forwarded::Lagged,
                Forwarded::Event(TransferEvent::Added(3)),
                Forwarded::Event(TransferEvent::Added(4)),
            ]
        );
    }
}
