// SPDX-License-Identifier: LGPL-2.1-or-later

use futures::StreamExt;
use lautta_core::app::Core;
use lautta_core::locations::LocationRegistry;
use lautta_core::paths::AppPaths;
use std::sync::Arc;
use std::time::Duration;
use zbus::connection::Builder;
use zbus::{Connection, Guid, MessageStream};

/// Fake mce on the other end of a socket pair: answers `get_psm_state` with `start`.
async fn fake_mce(start: bool) -> (Connection, Connection) {
    let (a, b) = tokio::net::UnixStream::pair().unwrap();
    let server = Builder::unix_stream(a)
        .server(Guid::generate())
        .unwrap()
        .p2p()
        .build();
    let client = Builder::unix_stream(b).p2p().build();
    let (server, client) = tokio::join!(server, client);
    let (server, client) = (server.unwrap(), client.unwrap());
    let calls = server.clone();
    tokio::spawn(async move {
        let mut stream = MessageStream::from(&calls);
        while let Some(Ok(msg)) = stream.next().await {
            let h = msg.header();
            if h.member().is_some_and(|m| m.as_str() == "get_psm_state")
                && h.interface()
                    .is_some_and(|i| i.as_str() == "com.nokia.mce.request")
            {
                calls.reply(&msg, &start).await.unwrap();
            }
        }
    });
    (server, client)
}

async fn emit(mce: &Connection, member: &str, on: bool) {
    mce.emit_signal(
        None::<()>,
        "/com/nokia/mce/signal",
        "com.nokia.mce.signal",
        member,
        &on,
    )
    .await
    .unwrap();
}

async fn until(core: &Core, on: bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while core.power_save() != on {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("power save never became {on}"));
}

#[tokio::test]
async fn battery_saving_follows_mce() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let core = Core::open_with(paths.clone(), LocationRegistry::new(paths))
        .await
        .unwrap();
    let (mce, conn) = fake_mce(true).await;
    let task = tokio::spawn(lautta_core::powersave::follow_owned(conn, Arc::downgrade(&core)));
    until(&core, true).await;
    emit(&mce, "psm_state_ind", false).await;
    until(&core, false).await;
    emit(&mce, "other_ind", true).await;
    emit(&mce, "psm_state_ind", true).await;
    until(&core, true).await;
    drop(core);
    emit(&mce, "psm_state_ind", false).await;
    let ended = tokio::time::timeout(Duration::from_secs(10), task).await;
    assert!(ended.unwrap().unwrap().is_ok(), "ends once the app is gone");
}

#[tokio::test]
async fn without_mce_battery_saving_stays_off() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let core = Core::open_with(paths.clone(), LocationRegistry::new(paths))
        .await
        .unwrap();
    let (a, b) = tokio::net::UnixStream::pair().unwrap();
    let server = Builder::unix_stream(a)
        .server(Guid::generate())
        .unwrap()
        .p2p()
        .build();
    let client = Builder::unix_stream(b).p2p().build();
    let (server, client) = tokio::join!(server, client);
    let server = server.unwrap();
    drop(server);
    let err = lautta_core::powersave::follow_owned(client.unwrap(), Arc::downgrade(&core)).await;
    assert!(err.is_err());
    assert!(!core.power_save());
}
