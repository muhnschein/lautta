// SPDX-License-Identifier: LGPL-2.1-or-later
//! The `Bridge` singleton: netvfs bridge status, consent, hand-offs,
//! ad-hoc servers and the questions the bridge asks (SPEC §7).
//!
//! `status` is one of `absent`, `connecting`, `tooOld`, `consentUnknown`,
//! `consentDenied`, `ready`, `reconnecting`.
//!
//! Secrets (SEC-1): the secret of *Connect to server* and the answers of a
//! sign-in question are moved into buffers that core wipes. The `QString`
//! the QML text field handed over cannot be wiped from Qt's memory; QML
//! clears its field right after the call and keeps no other copy.

use super::{error_parts, Guard};
use crate::json::{from_json, to_json};
use crate::runtime::{core, handle, spawn_then};
use lautta_core::app::Core;
use lautta_core::bridge::{AdHocOptions, BridgeStatus};
use lautta_core::questions::{Answer, Question, QuestionEvent, QuestionKind, Responses};
use lautta_core::Uri;
use qmetaobject::prelude::*;
use qmetaobject::{queued_callback, QPointer, QSingletonInit};
use serde::Deserialize;
use std::sync::{Arc, Once};
use zeroize::Zeroizing;

static PURGE: Once = Once::new();

/// The status as QML sees it.
pub fn status_name(s: BridgeStatus) -> &'static str {
    match s {
        BridgeStatus::Absent => "absent",
        BridgeStatus::Connecting => "connecting",
        BridgeStatus::TooOld => "tooOld",
        BridgeStatus::ConsentUnknown => "consentUnknown",
        BridgeStatus::ConsentDenied => "consentDenied",
        BridgeStatus::Ready => "ready",
        BridgeStatus::Reconnecting => "reconnecting",
    }
}

/// Provider preselected by *Add server* (the account is created in
/// netvfs' Settings, where the user can change it, NVB-5).
const DEFAULT_PROVIDER: &str = "sftp";

#[derive(Deserialize, Default)]
#[serde(default)]
struct OptionsJson {
    user: Option<String>,
    #[serde(rename = "securityProfile")]
    security_profile: Option<String>,
}

/// An answer to a question, as QML sends it: `{"accept":true}`,
/// `{"decline":true}` or `{"responses":["…"]}`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct AnswerJson {
    accept: bool,
    responses: Option<Vec<String>>,
}

/// Turns the answer text into an [`Answer`]; secret strings are moved, not
/// copied, into the wiping buffer.
fn parse_answer(json: Zeroizing<String>) -> Answer {
    let parsed: AnswerJson = from_json(&json).unwrap_or_default();
    match parsed.responses {
        Some(responses) => Answer::Responses(Responses::new(
            responses.into_iter().map(String::into_bytes).collect(),
        )),
        None if parsed.accept => Answer::Accept,
        None => Answer::Decline,
    }
}

fn details_json(q: &Question) -> String {
    to_json(&q.details)
}

/// Questions of the bridge go to the UI; conflicts belong to transfers.
fn is_ours(q: &Question) -> bool {
    q.kind != QuestionKind::Conflict
}

#[derive(QObject, Default)]
pub struct Bridge {
    base: qt_base_class!(trait QObject),
    status: qt_property!(QString; NOTIFY statusChanged),
    statusChanged: qt_signal!(),
    /// Bridge version text for About; empty while unknown.
    version: qt_property!(QString; NOTIFY statusChanged),
    /// True when the Servers section and its hand-offs are usable.
    ready: qt_property!(bool; NOTIFY statusChanged),

    requestConsent: qt_method!(fn(&mut self)),
    addServer: qt_method!(fn(&mut self, provider: QString)),
    editAccount: qt_method!(fn(&mut self, location_uri: QString)),
    connectAdHoc: qt_method!(fn(&mut self, url: QString, secret: QString, options_json: QString)),
    forgetAdHoc: qt_method!(fn(&mut self, id: QString)),
    removeRecent: qt_method!(fn(&mut self, id: QString)),
    disconnect: qt_method!(fn(&mut self, id: QString)),
    setDiscover: qt_method!(fn(&mut self, on: bool)),
    setForeground: qt_method!(fn(&mut self, foreground: bool)),
    poke: qt_method!(fn(&mut self)),
    answer: qt_method!(fn(&mut self, id: QString, answer_json: QString)),
    pending: qt_method!(fn(&self) -> QString),

    question: qt_signal!(questionId: QString, kind: QString, detailsJson: QString),
    questionResolved: qt_signal!(questionId: QString),
    adhocConnected: qt_signal!(locationUri: QString),
    /// A call failed; `subject` names what it was about (server, address).
    failed: qt_signal!(kind: QString, message: QString, subject: QString),

    guard: Guard,
}

impl QSingletonInit for Bridge {
    fn init(&mut self) {
        let Some(core) = core() else { return };
        self.publish_status(&core);
        PURGE.call_once(|| {
            let core = core.clone();
            handle().spawn(async move { core.spawn_account_purge(super::store_changed) });
        });
        self.follow_status(core.clone());
        self.follow_questions(core);
    }
}

impl Bridge {
    fn publish_status(&mut self, core: &Core) {
        let (status, version) = match &core.bridge {
            Some(b) => (b.status(), b.bridge_version().unwrap_or_default()),
            None => (BridgeStatus::Absent, String::new()),
        };
        self.status = QString::from(status_name(status));
        self.ready = status == BridgeStatus::Ready;
        self.version = QString::from(version.as_str());
        self.statusChanged();
    }

    fn follow_status(&mut self, core: Arc<Core>) {
        let Some(client) = core.bridge.clone() else {
            return;
        };
        let me = QPointer::from(&*self);
        let notify = queued_callback(move |core: Arc<Core>| {
            if let Some(b) = me.as_pinned() {
                b.borrow_mut().publish_status(&core);
            }
        });
        let mut stop = self.guard.subscribe();
        handle().spawn(async move {
            let mut status = client.watch_status();
            loop {
                tokio::select! {
                    r = status.changed() => if r.is_err() { return },
                    _ = stop.changed() => return,
                }
                notify(core.clone());
            }
        });
    }

    fn follow_questions(&mut self, core: Arc<Core>) {
        let me = QPointer::from(&*self);
        let notify = queued_callback(move |event: QuestionEvent| {
            let Some(b) = me.as_pinned() else { return };
            let b = b.borrow();
            match event {
                QuestionEvent::Asked(q) if is_ours(&q) => b.announce(&q),
                QuestionEvent::Asked(_) => {}
                QuestionEvent::Resolved(id) => b.questionResolved(QString::from(id.as_str())),
            }
        });
        let mut stop = self.guard.subscribe();
        let mut events = core.questions.subscribe();
        // Questions asked before the page looked are announced first.
        for q in core.questions.pending() {
            notify(QuestionEvent::Asked(q));
        }
        handle().spawn(async move {
            loop {
                tokio::select! {
                    event = events.recv() => match event {
                        Ok(e) => notify(e),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => return,
                    },
                    _ = stop.changed() => return,
                }
            }
        });
    }

    fn announce(&self, q: &Question) {
        self.question(
            QString::from(q.id.as_str()),
            QString::from(q.kind.name()),
            QString::from(details_json(q).as_str()),
        );
    }

    fn report(&self, e: &lautta_core::Error, subject: &str) {
        let (kind, message) = error_parts(e);
        self.failed(kind, message, QString::from(subject));
    }

    /// Runs a bridge call and reports its failure through `failed`.
    fn call<F>(&self, subject: String, f: impl FnOnce(Arc<Core>) -> F)
    where
        F: std::future::Future<Output = lautta_core::Result<()>> + Send + 'static,
    {
        let Some(core) = core() else { return };
        let me = QPointer::from(self);
        spawn_then(f(core), move |res| {
            if let (Err(e), Some(b)) = (res, me.as_pinned()) {
                b.borrow().report(&e, &subject);
            }
        });
    }

    fn requestConsent(&mut self) {
        self.call(String::new(), |core| async move {
            match &core.bridge {
                Some(b) => b.request_consent().await,
                None => Ok(()),
            }
        });
    }

    fn addServer(&mut self, provider: QString) {
        let provider = match provider.to_string() {
            p if p.is_empty() => DEFAULT_PROVIDER.to_owned(),
            p => p,
        };
        self.call(String::new(), |core| async move {
            match &core.bridge {
                Some(b) => b.add_account(&provider).await,
                None => Ok(()),
            }
        });
    }

    fn editAccount(&mut self, location_uri: QString) {
        let id = location_id(&location_uri.to_string());
        self.call(name_of(&id), |core| async move {
            match &core.bridge {
                Some(b) => b.open_account_settings(&id).await,
                None => Ok(()),
            }
        });
    }

    fn connectAdHoc(&mut self, url: QString, secret: QString, options_json: QString) {
        // Moved, not copied: the Vec is wiped by core when the call ends.
        let secret = secret.to_string().into_bytes();
        let options: OptionsJson = from_json(&options_json.to_string()).unwrap_or_default();
        let opts = AdHocOptions {
            user: options.user.filter(|u| !u.is_empty()),
            security_profile: options.security_profile.filter(|p| !p.is_empty()),
        };
        let url = url.to_string();
        let subject = lautta_core::app_browse::url_host(&url);
        let Some(core) = core() else { return };
        let me = QPointer::from(&*self);
        spawn_then(
            async move { core.connect_adhoc(&url, secret, opts).await },
            move |res| {
                let Some(b) = me.as_pinned() else { return };
                let b = b.borrow();
                match res {
                    Ok(id) => b.adhocConnected(QString::from(Uri::root(id).to_string().as_str())),
                    Err(e) => b.report(&e, &subject),
                }
            },
        );
    }

    fn forgetAdHoc(&mut self, id: QString) {
        let id = location_id(&id.to_string());
        self.call(name_of(&id), |core| async move { core.forget_adhoc(&id).await });
    }

    fn removeRecent(&mut self, id: QString) {
        let id = location_id(&id.to_string());
        self.call(name_of(&id), |core| async move {
            tokio::task::spawn_blocking(move || core.remove_adhoc_recent(&id))
                .await
                .map_err(|e| lautta_core::Error::new(lautta_core::ErrorKind::Internal, e.to_string()))?
        });
    }

    fn disconnect(&mut self, id: QString) {
        let id = location_id(&id.to_string());
        self.call(name_of(&id), |core| async move {
            match &core.bridge {
                Some(b) => b.disconnect(&id).await,
                None => Ok(()),
            }
        });
    }

    fn setDiscover(&mut self, on: bool) {
        self.call(String::new(), |core| async move {
            match &core.bridge {
                Some(b) => b.discover(on).await,
                None => Ok(()),
            }
        });
    }

    fn setForeground(&mut self, foreground: bool) {
        if let Some(b) = core().and_then(|c| c.bridge.clone()) {
            b.set_foreground(foreground);
        }
    }

    /// Looks for the bridge now: the app came back, *Retry* was tapped.
    fn poke(&mut self) {
        if let Some(b) = core().and_then(|c| c.bridge.clone()) {
            b.poke();
        }
    }

    fn answer(&mut self, id: QString, answer_json: QString) {
        let Some(core) = core() else { return };
        let answer = parse_answer(Zeroizing::new(answer_json.to_string()));
        if let Err(e) = core.questions.answer(&id.to_string(), answer) {
            // A question that is gone (answered elsewhere, connection lost).
            log::debug!("answer not delivered: {e}");
        }
    }

    /// The questions waiting for an answer, as `[{id, kind, details}]`.
    fn pending(&self) -> QString {
        let list: Vec<serde_json::Value> = core()
            .map(|c| c.questions.pending())
            .unwrap_or_default()
            .iter()
            .filter(|q| is_ours(q))
            .map(|q| serde_json::json!({"id": q.id, "kind": q.kind.name(), "details": q.details}))
            .collect();
        QString::from(to_json(&list).as_str())
    }
}

/// The name of a location for messages (empty when unknown).
fn name_of(id: &str) -> String {
    core()
        .and_then(|c| c.location(id))
        .map(|l| l.name)
        .unwrap_or_default()
}

/// The location id inside a URI, or the text itself when it is an id.
fn location_id(uri_or_id: &str) -> String {
    Uri::parse(uri_or_id)
        .map(|u| u.location)
        .unwrap_or_else(|_| uri_or_id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_names_are_the_documented_ones() {
        assert_eq!(status_name(BridgeStatus::Absent), "absent");
        assert_eq!(status_name(BridgeStatus::TooOld), "tooOld");
        assert_eq!(status_name(BridgeStatus::ConsentUnknown), "consentUnknown");
        assert_eq!(status_name(BridgeStatus::ConsentDenied), "consentDenied");
        assert_eq!(status_name(BridgeStatus::Ready), "ready");
        assert_eq!(status_name(BridgeStatus::Reconnecting), "reconnecting");
        assert_eq!(status_name(BridgeStatus::Connecting), "connecting");
    }

    #[test]
    fn answers_are_read_from_json() {
        let a = |s: &str| parse_answer(Zeroizing::new(s.to_owned()));
        assert!(matches!(a(r#"{"accept":true}"#), Answer::Accept));
        assert!(matches!(a(r#"{"decline":true}"#), Answer::Decline));
        assert!(matches!(a("nonsense"), Answer::Decline));
        match a(r#"{"responses":["123456","0000"]}"#) {
            Answer::Responses(r) => assert_eq!(r.as_slice(), [b"123456".to_vec(), b"0000".to_vec()]),
            other => panic!("not responses: {other:?}"),
        }
    }

    #[test]
    fn locations_come_from_uris_or_ids() {
        assert_eq!(location_id("lautta://nv-account:1/photos"), "nv-account:1");
        assert_eq!(location_id("nv-adhoc:3"), "nv-adhoc:3");
    }

    #[test]
    fn conflicts_are_not_ours() {
        let q = |kind| Question {
            id: "q1".into(),
            kind,
            details: Default::default(),
        };
        assert!(!is_ours(&q(QuestionKind::Conflict)));
        assert!(is_ours(&q(QuestionKind::IdentityUnknown)));
        assert_eq!(details_json(&q(QuestionKind::Conflict)), "{}");
    }
}
