// SPDX-License-Identifier: LGPL-2.1-or-later
//! Blocking prompts: conflicts, ad-hoc identity, keyboard-interactive (§5.3).
//! A generic hub: the asker registers a question and waits; the UI lists the
//! pending ones, listens for new ones and answers by id.

use crate::error::{Error, ErrorKind, Result};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::sync::{broadcast, oneshot};
use zeroize::Zeroizing;

pub type QuestionId = String;

/// What is being asked. The first three come from the bridge (netvfs XB-10);
/// `Conflict` is for transfers (OPS-2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QuestionKind {
    /// An ad-hoc server's identity is not known yet (NVB-6).
    IdentityUnknown,
    /// Prompts of a keyboard-interactive sign-in, answered with
    /// [`Answer::Responses`].
    KeyboardInteractive,
    /// An ad-hoc server needs a connection without encryption (NVB-6).
    InsecureConsent,
    /// A name clash at the destination of a transfer (OPS-2).
    Conflict,
    /// A kind this version does not know; the UI shows a generic dialog.
    Other(String),
}

impl QuestionKind {
    pub fn from_wire(kind: &str) -> QuestionKind {
        match kind {
            "identity-unknown" => QuestionKind::IdentityUnknown,
            "keyboard-interactive" => QuestionKind::KeyboardInteractive,
            "insecure-consent" => QuestionKind::InsecureConsent,
            "conflict" => QuestionKind::Conflict,
            other => QuestionKind::Other(other.to_owned()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            QuestionKind::IdentityUnknown => "identity-unknown",
            QuestionKind::KeyboardInteractive => "keyboard-interactive",
            QuestionKind::InsecureConsent => "insecure-consent",
            QuestionKind::Conflict => "conflict",
            QuestionKind::Other(o) => o,
        }
    }
}

/// A question as the UI sees it. `details` are display strings (host,
/// fingerprint, prompts, file names); the kind says how to read them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub id: QuestionId,
    pub kind: QuestionKind,
    pub details: BTreeMap<String, String>,
}

/// Secret answers of a keyboard-interactive prompt; wiped when dropped
/// (SEC-1) and never printed.
pub struct Responses(Zeroizing<Vec<Vec<u8>>>);

impl Responses {
    pub fn new(answers: Vec<Vec<u8>>) -> Responses {
        Responses(Zeroizing::new(answers))
    }

    pub fn as_slice(&self) -> &[Vec<u8>] {
        &self.0
    }

    /// Moves the answers out as a wiping buffer.
    pub fn into_inner(self) -> Zeroizing<Vec<Vec<u8>>> {
        self.0
    }
}

impl fmt::Debug for Responses {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Responses({} hidden)", self.0.len())
    }
}

#[derive(Debug)]
pub enum Answer {
    Accept,
    Decline,
    /// One of a list of choices, by name (a conflict resolution).
    Choice(String),
    Responses(Responses),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionEvent {
    Asked(Question),
    /// Answered, canceled, or given up by the asker.
    Resolved(QuestionId),
}

struct Entry {
    question: Question,
    reply: oneshot::Sender<Answer>,
}

#[derive(Default)]
struct State {
    next: u64,
    pending: Vec<Entry>,
}

/// The hub of pending questions.
pub struct Questions {
    state: Mutex<State>,
    events: broadcast::Sender<QuestionEvent>,
}

impl Questions {
    pub fn new() -> Arc<Questions> {
        Arc::new(Questions {
            state: Mutex::new(State::default()),
            events: broadcast::channel(64).0,
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Registers a question and announces it. The returned [`Pending`] resolves
    /// with the answer; dropping it withdraws the question.
    pub fn ask(self: &Arc<Self>, kind: QuestionKind, details: BTreeMap<String, String>) -> Pending {
        let (tx, rx) = oneshot::channel();
        let question = {
            let mut st = self.lock();
            st.next += 1;
            let question = Question {
                id: format!("q{}", st.next),
                kind,
                details,
            };
            st.pending.push(Entry {
                question: question.clone(),
                reply: tx,
            });
            question
        };
        let id = question.id.clone();
        // No listener yet is fine: the UI reads `pending()` when it starts.
        let _ = self.events.send(QuestionEvent::Asked(question));
        Pending {
            id,
            rx,
            hub: self.clone(),
        }
    }

    fn take(&self, id: &str) -> Option<Entry> {
        let mut st = self.lock();
        let at = st.pending.iter().position(|e| e.question.id == id)?;
        Some(st.pending.remove(at))
    }

    /// Answers a pending question.
    pub fn answer(&self, id: &str, answer: Answer) -> Result<()> {
        let entry = self
            .take(id)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "no such question"))?;
        let _ = self
            .events
            .send(QuestionEvent::Resolved(entry.question.id.clone()));
        // The asker may have stopped waiting in the same instant.
        entry
            .reply
            .send(answer)
            .map_err(|_| Error::new(ErrorKind::Canceled, "the question was withdrawn"))
    }

    /// Withdraws a question; its asker sees `None`. False if it was not pending.
    pub fn cancel(&self, id: &str) -> bool {
        match self.take(id) {
            Some(entry) => {
                let _ = self.events.send(QuestionEvent::Resolved(entry.question.id));
                true
            }
            None => false,
        }
    }

    /// Questions waiting for an answer, oldest first.
    pub fn pending(&self) -> Vec<Question> {
        self.lock().pending.iter().map(|e| e.question.clone()).collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<QuestionEvent> {
        self.events.subscribe()
    }
}

/// The asker's side of a question.
pub struct Pending {
    id: QuestionId,
    rx: oneshot::Receiver<Answer>,
    hub: Arc<Questions>,
}

impl Pending {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Waits for the answer; `None` when the question was canceled.
    pub async fn wait(mut self) -> Option<Answer> {
        (&mut self.rx).await.ok()
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.hub.cancel(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn details(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn kinds_round_trip_names() {
        for k in [
            "identity-unknown",
            "keyboard-interactive",
            "insecure-consent",
            "conflict",
            "odd",
        ] {
            assert_eq!(QuestionKind::from_wire(k).name(), k);
        }
        assert_eq!(QuestionKind::from_wire("odd"), QuestionKind::Other("odd".into()));
    }

    #[tokio::test]
    async fn ask_answer_round_trip() {
        let hub = Questions::new();
        let mut events = hub.subscribe();
        let pending = hub.ask(QuestionKind::Conflict, details(&[("name", "a.txt")]));
        assert_eq!(hub.pending().len(), 1);
        let QuestionEvent::Asked(q) = events.recv().await.unwrap() else {
            panic!("expected Asked");
        };
        assert_eq!(q.id, pending.id());
        assert_eq!(q.details["name"], "a.txt");
        hub.answer(&q.id, Answer::Choice("skip".into())).unwrap();
        assert_eq!(
            events.recv().await.unwrap(),
            QuestionEvent::Resolved(q.id.clone())
        );
        match pending.wait().await {
            Some(Answer::Choice(c)) => assert_eq!(c, "skip"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(hub.pending().is_empty());
    }

    #[tokio::test]
    async fn unknown_and_repeated_answers_are_not_found() {
        let hub = Questions::new();
        assert_eq!(
            hub.answer("q9", Answer::Accept).unwrap_err().kind,
            ErrorKind::NotFound
        );
        let pending = hub.ask(QuestionKind::InsecureConsent, BTreeMap::new());
        let id = pending.id().to_owned();
        hub.answer(&id, Answer::Accept).unwrap();
        assert_eq!(
            hub.answer(&id, Answer::Accept).unwrap_err().kind,
            ErrorKind::NotFound
        );
        assert!(matches!(pending.wait().await, Some(Answer::Accept)));
    }

    #[tokio::test]
    async fn cancel_wakes_the_asker_with_none() {
        let hub = Questions::new();
        let pending = hub.ask(QuestionKind::IdentityUnknown, BTreeMap::new());
        assert!(hub.cancel(pending.id()));
        assert!(!hub.cancel(pending.id()));
        assert!(pending.wait().await.is_none());
    }

    #[tokio::test]
    async fn a_dropped_asker_withdraws_the_question() {
        let hub = Questions::new();
        let mut events = hub.subscribe();
        let pending = hub.ask(QuestionKind::Conflict, BTreeMap::new());
        let id = pending.id().to_owned();
        drop(pending);
        assert!(hub.pending().is_empty());
        assert!(matches!(events.recv().await.unwrap(), QuestionEvent::Asked(_)));
        assert_eq!(events.recv().await.unwrap(), QuestionEvent::Resolved(id.clone()));
        // Answering a withdrawn question reports that it is gone.
        assert_eq!(
            hub.answer(&id, Answer::Accept).unwrap_err().kind,
            ErrorKind::NotFound
        );
    }

    #[test]
    fn ids_are_unique_and_ordered() {
        let hub = Questions::new();
        let a = hub.ask(QuestionKind::Conflict, BTreeMap::new());
        let b = hub.ask(QuestionKind::Conflict, BTreeMap::new());
        assert_ne!(a.id(), b.id());
        let ids: Vec<_> = hub.pending().into_iter().map(|q| q.id).collect();
        assert_eq!(ids, vec![a.id().to_owned(), b.id().to_owned()]);
    }

    #[test]
    fn responses_are_hidden_in_debug() {
        let r = Responses::new(vec![b"hunter2".to_vec()]);
        let shown = format!("{r:?}");
        assert!(!shown.contains("hunter2"));
        assert_eq!(r.as_slice().len(), 1);
        assert_eq!(r.into_inner().len(), 1);
    }
}
