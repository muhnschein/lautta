// SPDX-License-Identifier: LGPL-2.1-or-later
//! Shared setup of the bridge tests: a fake bridge listening where the app
//! looks for the socket (NVB-1), and a client started against it.
#![allow(dead_code)]

use lautta_bridge_proto::fake::{FakeBridge, Listener};
use lautta_core::bridge::{BridgeClient, BridgeConfig, BridgeStatus};
use lautta_core::paths::AppPaths;
use lautta_core::provider::netvfs::NetvfsProvider;
use lautta_core::questions::Questions;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

pub const ACCOUNT: &str = "nv-account:1";

pub struct Rig {
    pub fake: FakeBridge,
    pub client: BridgeClient,
    pub questions: Arc<Questions>,
    pub paths: AppPaths,
    pub listener: Option<Listener>,
    pub home: TempDir,
}

/// The configuration the tests start from: the app's defaults without polling,
/// so that only starts, pokes and the foreground trigger a check, and without a
/// handshake timeout, which a paused clock would fire at the first idle moment.
pub fn config(paths: &AppPaths) -> BridgeConfig {
    BridgeConfig {
        poll_interval: None,
        handshake_timeout: None,
        ..BridgeConfig::for_app(paths)
    }
}

/// The reconnect schedule 1, 2, 4, 8, 15 s at a twentieth of the time.
pub fn quick_backoff(mut config: BridgeConfig) -> BridgeConfig {
    config.backoff = [50, 100, 200, 400, 750].map(Duration::from_millis).to_vec();
    config
}

impl Rig {
    /// A fake with `account:1` ("Fake 1") listening, a client started, and the
    /// client waited for until it is `Ready`.
    pub async fn ready() -> Rig {
        let rig = Rig::listening().await;
        rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
        rig
    }

    /// [`Rig::ready`] with the client's configuration tuned.
    pub async fn ready_with(tune: impl FnOnce(BridgeConfig) -> BridgeConfig) -> Rig {
        let fake = FakeBridge::new();
        fake.add_account(1, "fake", "Fake 1", "fake.example").await;
        let rig = Rig::with_fake(fake, true, tune).await;
        rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
        rig
    }

    /// Like [`Rig::ready`] without waiting.
    pub async fn listening() -> Rig {
        let fake = FakeBridge::new();
        fake.add_account(1, "fake", "Fake 1", "fake.example").await;
        Rig::with_fake(fake, true, |c| c).await
    }

    /// A client against `fake`; the socket exists only if `listen`.
    pub async fn with_fake(
        fake: FakeBridge,
        listen: bool,
        tune: impl FnOnce(BridgeConfig) -> BridgeConfig,
    ) -> Rig {
        let home = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(home.path());
        let listener = listen.then(|| fake.listen(&paths.bridge_socket()).unwrap());
        let questions = Questions::new();
        let client = BridgeClient::start(tune(config(&paths)), questions.clone()).unwrap();
        Rig {
            fake,
            client,
            questions,
            paths,
            listener,
            home,
        }
    }

    /// The socket appears (netvfs creates it, the app never does).
    pub fn listen(&mut self) {
        self.listener = Some(self.fake.listen(&self.paths.bridge_socket()).unwrap());
    }

    pub fn provider(&self) -> NetvfsProvider {
        self.client.provider(ACCOUNT)
    }

    pub async fn provider_with_caps(&self) -> NetvfsProvider {
        NetvfsProvider::connect(self.client.clone(), ACCOUNT)
            .await
            .unwrap()
    }
}

/// Lets spawned tasks run until `cond` holds, polling on the (possibly
/// virtual) clock.
pub async fn eventually(what: &str, mut cond: impl FnMut() -> bool) {
    for _ in 0..400 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}
