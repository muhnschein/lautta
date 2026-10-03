// SPDX-License-Identifier: LGPL-2.1-or-later
//! The shared contract (SPEC TST-2): the golden sequences replay against the
//! fake bridge, and the fake serves the interface the XML describes.
#![cfg(feature = "fake")]

use lautta_bridge_proto::fake::replay;
use lautta_bridge_proto::fake::FakeBridge;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn contract_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/bridge-contract")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_golden_sequence_replays() {
    let ran = replay::replay_dir(&contract_dir()).await.unwrap();
    let on_disk = std::fs::read_dir(contract_dir())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .is_ok_and(|e| e.path().extension().is_some_and(|x| x == "json"))
        })
        .count();
    assert_eq!(ran.len(), on_disk);
    assert!(ran.len() >= 6, "{ran:?}");
}

/// `(kind, member) -> [(direction, type)]` of the `org.netvfs.Bridge1` interface.
fn members(xml: &str) -> BTreeMap<(String, String), Vec<(String, String)>> {
    fn attr(line: &str, name: &str) -> Option<String> {
        let marker = format!("{name}=\"");
        let start = line.find(&marker)? + marker.len();
        let len = line[start..].find('"')?;
        Some(line[start..start + len].to_owned())
    }
    let mut out = BTreeMap::new();
    let mut in_bridge = false;
    let mut current = None;
    for line in xml.lines().map(str::trim) {
        if line.starts_with("<interface") {
            in_bridge = attr(line, "name").as_deref() == Some("org.netvfs.Bridge1");
        } else if in_bridge && (line.starts_with("<method") || line.starts_with("<signal")) {
            let kind = if line.starts_with("<method") {
                "method"
            } else {
                "signal"
            };
            let key = (kind.to_owned(), attr(line, "name").unwrap());
            out.insert(key.clone(), Vec::new());
            current = (!line.ends_with("/>")).then_some(key);
        } else if line.starts_with("<arg") {
            if let Some(key) = &current {
                let dir = attr(line, "direction").unwrap_or_default();
                out.get_mut(key).unwrap().push((dir, attr(line, "type").unwrap()));
            }
        } else if line.starts_with("</method") || line.starts_with("</signal") {
            current = None;
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_fake_serves_the_contract_interface() {
    let fake = FakeBridge::new();
    let conn = fake.connect().await.unwrap();
    let reply = conn
        .zbus()
        .call_method(
            None::<&str>,
            "/org/netvfs/Bridge",
            Some("org.freedesktop.DBus.Introspectable"),
            "Introspect",
            &(),
        )
        .await
        .unwrap();
    let served: String = reply.body().deserialize().unwrap();
    let want = members(&std::fs::read_to_string(contract_dir().join("org.netvfs.Bridge1.xml")).unwrap());
    let got = members(&served);
    assert!(want.len() > 40, "contract parsed to {} members", want.len());
    assert_eq!(got, want);
}
