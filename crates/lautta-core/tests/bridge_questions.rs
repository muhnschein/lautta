// SPDX-License-Identifier: LGPL-2.1-or-later
//! Questions of the bridge (identity, keyboard-interactive, insecure
//! connection) travel to the app's hub and the answers travel back (SPEC NVB-6,
//! §5.3), and secrets are wiped (SEC-1).

mod bridge_support;

use bridge_support::{eventually, Rig};
use lautta_bridge_proto::fake::{InfoValue, ScriptedQuestion};
use lautta_core::bridge::AdHocOptions;
use lautta_core::error::ErrorKind;
use lautta_core::questions::{Answer, QuestionEvent, QuestionKind, Responses};
use std::time::Duration;
use zeroize::Zeroizing;

fn question(kind: &str, details: &[(&str, InfoValue)]) -> ScriptedQuestion {
    ScriptedQuestion {
        kind: kind.to_owned(),
        details: details
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    }
}

fn s(v: &str) -> InfoValue {
    InfoValue::Str(v.to_owned())
}

fn identity() -> ScriptedQuestion {
    question(
        "identity-unknown",
        &[
            ("host", s("nas.example")),
            ("fingerprint", s("SHA256:abc")),
            ("algorithm", s("ssh-ed25519")),
            ("port", InfoValue::Int(22)),
            ("systemTrusted", InfoValue::Bool(false)),
            (
                "problems",
                InfoValue::Strings(vec!["self-signed".into(), "expired".into()]),
            ),
        ],
    )
}

/// Waits for the next question of the hub.
async fn next_question(
    rig: &Rig,
    events: &mut tokio::sync::broadcast::Receiver<QuestionEvent>,
) -> lautta_core::questions::Question {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("no question arrived")
            .unwrap();
        if let QuestionEvent::Asked(q) = event {
            assert!(rig.questions.pending().iter().any(|p| p.id == q.id));
            return q;
        }
    }
}

#[tokio::test]
async fn an_unknown_identity_is_asked_and_accepted() {
    let rig = Rig::ready().await;
    rig.fake.script_adhoc_questions(vec![identity()]);
    let mut events = rig.questions.subscribe();
    let client = rig.client.clone();
    let connect = tokio::spawn(async move {
        client
            .connect_adhoc(
                "sftp://nas.example/",
                Zeroizing::new(b"pw".to_vec()),
                AdHocOptions::default(),
            )
            .await
    });

    let q = next_question(&rig, &mut events).await;
    assert_eq!(q.kind, QuestionKind::IdentityUnknown);
    assert_eq!(q.details["host"], "nas.example");
    assert_eq!(q.details["fingerprint"], "SHA256:abc");
    assert_eq!(q.details["port"], "22");
    assert_eq!(q.details["systemTrusted"], "false");
    assert_eq!(q.details["problems"], "self-signed\nexpired");
    assert!(!connect.is_finished(), "the connection waits for the answer");

    rig.questions.answer(&q.id, Answer::Accept).unwrap();
    let id = connect.await.unwrap().unwrap();
    assert_eq!(id, "nv-adhoc:1");
    assert!(rig.fake.answers()[0].accept);
    assert!(rig.questions.pending().is_empty());
}

#[tokio::test]
async fn declining_an_identity_fails_the_connection_with_its_name() {
    let rig = Rig::ready().await;
    rig.fake.script_adhoc_questions(vec![identity()]);
    let mut events = rig.questions.subscribe();
    let client = rig.client.clone();
    let connect = tokio::spawn(async move {
        client
            .connect_adhoc(
                "sftp://nas.example/",
                Zeroizing::new(Vec::new()),
                AdHocOptions::default(),
            )
            .await
    });
    let q = next_question(&rig, &mut events).await;
    rig.questions.answer(&q.id, Answer::Decline).unwrap();
    let e = connect.await.unwrap().unwrap_err();
    assert_eq!(e.kind, ErrorKind::ServerIdentityUnknown);
    assert!(!rig.fake.answers()[0].accept);
    assert_eq!(rig.client.locations().len(), 1, "nothing was added");
}

#[tokio::test]
async fn keyboard_interactive_answers_reach_the_bridge_unchanged() {
    let rig = Rig::ready().await;
    rig.fake.script_adhoc_questions(vec![question(
        "keyboard-interactive",
        &[
            ("name", s("Two-factor")),
            ("instruction", s("Enter the code")),
            (
                "prompts",
                InfoValue::Strings(vec!["Code: ".into(), "PIN: ".into()]),
            ),
        ],
    )]);
    let mut events = rig.questions.subscribe();
    let client = rig.client.clone();
    let connect = tokio::spawn(async move {
        client
            .connect_adhoc(
                "sftp://nas.example/",
                Zeroizing::new(b"pw".to_vec()),
                AdHocOptions::default(),
            )
            .await
    });
    let q = next_question(&rig, &mut events).await;
    assert_eq!(q.kind, QuestionKind::KeyboardInteractive);
    assert_eq!(q.details["prompts"], "Code: \nPIN: ");
    assert_eq!(q.details["instruction"], "Enter the code");

    let answers = Responses::new(vec![b"123456".to_vec(), b"0000".to_vec()]);
    rig.questions.answer(&q.id, Answer::Responses(answers)).unwrap();
    connect.await.unwrap().unwrap();
    let recorded = rig.fake.answers();
    assert_eq!(recorded.len(), 1);
    assert!(recorded[0].accept);
    assert_eq!(recorded[0].answers, vec![b"123456".to_vec(), b"0000".to_vec()]);
}

#[tokio::test]
async fn an_insecure_connection_needs_consent() {
    let rig = Rig::ready().await;
    rig.fake.script_adhoc_questions(vec![question(
        "insecure-consent",
        &[
            ("url", s("ftp://old.example/")),
            ("provider", s("ftp")),
            ("host", s("old.example")),
        ],
    )]);
    let mut events = rig.questions.subscribe();
    let client = rig.client.clone();
    let connect = tokio::spawn(async move {
        client
            .connect_adhoc(
                "ftp://old.example/",
                Zeroizing::new(Vec::new()),
                AdHocOptions::default(),
            )
            .await
    });
    let q = next_question(&rig, &mut events).await;
    assert_eq!(q.kind, QuestionKind::InsecureConsent);
    assert_eq!(q.details["host"], "old.example");
    rig.questions.answer(&q.id, Answer::Decline).unwrap();
    assert_eq!(
        connect.await.unwrap().unwrap_err().kind,
        ErrorKind::SecurityPolicy
    );
}

#[tokio::test]
async fn several_questions_come_one_after_the_other() {
    let rig = Rig::ready().await;
    rig.fake.script_adhoc_questions(vec![
        question("insecure-consent", &[("host", s("h"))]),
        identity(),
    ]);
    let mut events = rig.questions.subscribe();
    let client = rig.client.clone();
    let connect = tokio::spawn(async move {
        client
            .connect_adhoc("ftp://h/", Zeroizing::new(Vec::new()), AdHocOptions::default())
            .await
    });
    let first = next_question(&rig, &mut events).await;
    assert_eq!(first.kind, QuestionKind::InsecureConsent);
    rig.questions.answer(&first.id, Answer::Accept).unwrap();
    let second = next_question(&rig, &mut events).await;
    assert_eq!(second.kind, QuestionKind::IdentityUnknown);
    assert_ne!(first.id, second.id);
    rig.questions.answer(&second.id, Answer::Accept).unwrap();
    connect.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_question_the_app_withdraws_is_answered_no() {
    let rig = Rig::ready().await;
    let mut events = rig.questions.subscribe();
    let fake = rig.fake.clone();
    let asked = tokio::spawn(async move {
        fake.ask("keyboard-interactive", vec![("name".into(), s("x"))])
            .await
    });
    let q = next_question(&rig, &mut events).await;
    assert!(rig.questions.cancel(&q.id));
    let answer = asked.await.unwrap().expect("the bridge got an answer");
    assert!(!answer.accept);
    assert!(answer.answers.is_empty());
}

#[tokio::test]
async fn questions_vanish_with_the_connection() {
    let rig = Rig::ready().await;
    let mut events = rig.questions.subscribe();
    let fake = rig.fake.clone();
    let asked =
        tokio::spawn(async move { fake.ask("identity-unknown", vec![("host".into(), s("h"))]).await });
    let q = next_question(&rig, &mut events).await;

    rig.fake.set_accepting(false);
    rig.fake.disconnect_clients().await;
    eventually("the question to be withdrawn", || {
        rig.questions.pending().is_empty()
    })
    .await;
    let mut resolved = false;
    while let Ok(event) = events.try_recv() {
        resolved |= event == QuestionEvent::Resolved(q.id.clone());
    }
    assert!(resolved, "the UI is told to close the dialog");
    // The answer cannot be delivered any more; the asker on the bridge side is gone too.
    assert!(asked.await.unwrap().is_none());
}

#[tokio::test]
async fn an_unknown_kind_still_reaches_the_ui() {
    let rig = Rig::ready().await;
    let mut events = rig.questions.subscribe();
    let fake = rig.fake.clone();
    let asked = tokio::spawn(async move { fake.ask("future-kind", vec![("a".into(), s("b"))]).await });
    let q = next_question(&rig, &mut events).await;
    assert_eq!(q.kind, QuestionKind::Other("future-kind".into()));
    rig.questions.answer(&q.id, Answer::Accept).unwrap();
    assert!(asked.await.unwrap().unwrap().accept);
}
