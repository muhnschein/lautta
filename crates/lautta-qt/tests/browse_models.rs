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
use std::cell::RefCell;
use std::sync::Mutex;

static FAKE: Mutex<Option<FakeBridge>> = Mutex::new(None);
static LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn fake() -> FakeBridge {
    FAKE.lock().unwrap().clone().expect("the fake bridge is set up")
}

fn run<T>(f: impl std::future::Future<Output = T>) -> T {
    lautta_qt::runtime::handle().block_on(f)
}

extern "C" fn capture(_: QtMsgType, _: &QMessageLogContext, message: &QString) {
    let text = message.to_string();
    println!("{text}");
    LOGS.lock().unwrap().push(text);
}

/// What the QML script can do to the world around the app.
#[derive(QObject, Default)]
struct Fake {
    base: qt_base_class!(trait QObject),
    grant: qt_method!(fn(&self)),
    consentRequests: qt_method!(fn(&self) -> i32),
    setAttention: qt_method!(fn(&self, id: QString, attention: QString)),
    removeAccount: qt_method!(fn(&self, id: QString)),
    scriptQuestion: qt_method!(fn(&self, kind: QString)),
    secrets: qt_method!(fn(&self) -> QString),
    answers: qt_method!(fn(&self) -> QString),
    handoffs: qt_method!(fn(&self) -> QString),
    record: qt_method!(fn(&self, uri: QString, name: QString, kind: QString)),
    removeFile: qt_method!(fn(&self, relative: QString)),
}

impl Fake {
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

    fn removeFile(&self, relative: QString) {
        let home = std::env::var("HOME").unwrap();
        std::fs::remove_file(std::path::Path::new(&home).join(relative.to_string())).unwrap();
    }
}

const SCRIPT: &str = include_str!("browse_models.qml");

#[test]
fn browse_models_follow_the_core_and_the_bridge() {
    let home = tempfile::tempdir().unwrap();
    for d in ["Documents", "Music"] {
        std::fs::create_dir_all(home.path().join(d)).unwrap();
    }
    std::fs::write(home.path().join("Documents/a.txt"), "a").unwrap();
    std::fs::write(home.path().join("Documents/b.txt"), "b").unwrap();
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
    lautta_qt::register_types();
    install_message_handler(Some(capture));

    let helper = RefCell::new(Fake::default());
    let mut engine = QmlEngine::new();
    // SAFETY: `helper` outlives the engine, which is dropped first.
    engine.set_object_property("Fake".into(), unsafe { QObjectPinned::new(&helper) });
    let script = format!("import QtQuick 2.6\nimport Lautta 1.0\n{SCRIPT}");
    engine.load_data(script.into());
    engine.exec();

    let logs = LOGS.lock().unwrap();
    let failed: Vec<&String> = logs.iter().filter(|l| l.contains("FAILED")).collect();
    assert!(failed.is_empty(), "failed steps: {failed:?}");
    assert!(logs.iter().any(|l| l == "DONE"), "the script did not finish");
    let passed = logs.iter().filter(|l| l.starts_with("PASS ")).count();
    assert!(passed >= 20, "only {passed} steps passed");
}
