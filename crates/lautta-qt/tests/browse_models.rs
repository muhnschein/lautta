// SPDX-License-Identifier: LGPL-2.1-or-later
//! The browse facades with the real Qt event loop, a temporary HOME and a
//! fake netvfs bridge: models load and follow changes, the `Bridge`
//! singleton follows consent, questions, ad-hoc servers and hand-offs, and
//! a removed account is forgotten (SPEC §7, §13, §15.2, SEC-5).
//!
//! The scenario is a QML script that runs one step after the other, each
//! with an action and a condition to wait for; the fake bridge is driven
//! through the `Fake` object. One engine per process, hence one test.

#![allow(non_snake_case)]

use lautta_bridge_proto::fake::{Consent, FakeBridge, InfoValue, ScriptedQuestion};
use lautta_core::org::RecentKind;
use lautta_core::paths::AppPaths;
use lautta_core::Uri;
use qmetaobject::*;
use std::sync::Mutex;

static FAKE: Mutex<Option<FakeBridge>> = Mutex::new(None);

fn fake() -> FakeBridge {
    FAKE.lock().unwrap().clone().expect("the fake bridge is set up")
}

fn run<T>(f: impl std::future::Future<Output = T>) -> T {
    lautta_qt::runtime::handle().block_on(f)
}

/// What the QML script can do to the world around the app.
#[derive(QObject, Default)]
struct FakeWorld {
    base: qt_base_class!(trait QObject),
    pump: qt_method!(fn(&self, ms: i32)),
    grant: qt_method!(fn(&self)),
    consentRequests: qt_method!(fn(&self) -> i32),
    setAttention: qt_method!(fn(&self, id: QString, attention: QString)),
    removeAccount: qt_method!(fn(&self, id: QString)),
    scriptQuestion: qt_method!(fn(&self, kind: QString)),
    secrets: qt_method!(fn(&self) -> QString),
    answers: qt_method!(fn(&self) -> QString),
    handoffs: qt_method!(fn(&self) -> QString),
    record: qt_method!(fn(&self, uri: QString, name: QString, kind: QString)),
}

impl FakeWorld {
    fn pump(&self, ms: i32) {
        lautta_qt::browse::pump_events(ms);
    }

    fn grant(&self) {
        run(fake().set_consent(Consent::Granted));
    }

    fn consentRequests(&self) -> i32 {
        fake().consent_requests() as i32
    }

    fn setAttention(&self, id: QString, attention: QString) {
        let attention = attention.to_string();
        run(fake().set_attention(
            &id.to_string(),
            Some(attention.as_str()).filter(|a| !a.is_empty()),
        ));
    }

    fn removeAccount(&self, id: QString) {
        run(fake().remove_location(&id.to_string()));
    }

    fn scriptQuestion(&self, kind: QString) {
        let kind = kind.to_string();
        let details = match kind.as_str() {
            "identity-unknown" => vec![
                ("host".to_owned(), InfoValue::Str("host.example".into())),
                ("fingerprint".to_owned(), InfoValue::Str("SHA256:abc".into())),
            ],
            _ => vec![
                ("name".to_owned(), InfoValue::Str("Two-factor".into())),
                ("prompts".to_owned(), InfoValue::Strings(vec!["Code: ".into()])),
            ],
        };
        fake().script_adhoc_questions(vec![ScriptedQuestion { kind, details }]);
    }

    fn secrets(&self) -> QString {
        let list: Vec<String> = fake()
            .received_secrets()
            .iter()
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();
        QString::from(serde_json::to_string(&list).unwrap().as_str())
    }

    fn answers(&self) -> QString {
        let list: Vec<serde_json::Value> = fake()
            .answers()
            .iter()
            .map(|a| {
                let responses: Vec<String> = a
                    .answers
                    .iter()
                    .map(|s| String::from_utf8_lossy(s).into_owned())
                    .collect();
                serde_json::json!({"accept": a.accept, "answers": responses})
            })
            .collect();
        QString::from(serde_json::to_string(&list).unwrap().as_str())
    }

    fn handoffs(&self) -> QString {
        QString::from(serde_json::to_string(&fake().handoffs()).unwrap().as_str())
    }

    fn record(&self, uri: QString, name: QString, kind: QString) {
        let core = lautta_qt::runtime::core().unwrap();
        let uri = Uri::parse(&uri.to_string()).unwrap();
        let kind = RecentKind::parse(&kind.to_string()).unwrap();
        core.recents.record(&uri, &name.to_string(), kind).unwrap();
    }
}

impl QSingletonInit for FakeWorld {
    fn init(&mut self) {}
}

const SCRIPT: &str = include_str!("browse_models.qml");

#[test]
fn browse_models_follow_the_core_and_the_bridge() {
    let home = tempfile::tempdir().unwrap();
    for d in ["Documents", "Music"] {
        std::fs::create_dir_all(home.path().join(d)).unwrap();
    }
    std::fs::write(home.path().join("Documents/a.txt"), "a").unwrap();
    std::fs::write(home.path().join("Documents/.hidden"), "h").unwrap();
    std::env::set_var("HOME", home.path());
    std::env::set_var("QT_QPA_PLATFORM", "offscreen");

    // The bridge listens before the app starts; the user has not decided yet.
    let bridge = FakeBridge::new();
    let paths = AppPaths::from_env();
    let _listener = run(async {
        bridge.set_consent(Consent::Unknown).await;
        bridge.add_account(1, "sftp", "NAS", "nas.home").await;
        bridge.listen(&paths.bridge_socket()).unwrap()
    });
    *FAKE.lock().unwrap() = Some(bridge);

    lautta_qt::runtime::init(paths).unwrap();
    qml_register_singleton_type::<FakeWorld>(
        &lautta_qt::cstr("FakeWorld"),
        1,
        0,
        &lautta_qt::cstr("FakeWorld"),
    );

    let file = home.path().join("scenario.qml");
    std::fs::write(
        &file,
        format!("import QtQuick 2.6\nimport Lautta 1.0\nimport FakeWorld 1.0\n{SCRIPT}"),
    )
    .unwrap();
    // `qml_check` fails on any console.error, which is how a step fails.
    assert_eq!(
        lautta_qt::qml_check(&[file.display().to_string()]),
        0,
        "a step of the scenario failed (see output)"
    );
}
