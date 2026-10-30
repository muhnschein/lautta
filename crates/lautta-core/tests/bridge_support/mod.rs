// SPDX-License-Identifier: LGPL-2.1-or-later
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

/// App defaults without polling and handshake timeout (a paused clock would fire it at the first
/// idle moment).
pub fn config(paths: &AppPaths) -> BridgeConfig {
    BridgeConfig {
        poll_interval: None,
        handshake_timeout: None,
        nearby_settle: Duration::from_millis(100),
        ..BridgeConfig::for_app(paths)
    }
}

pub fn quick_backoff(mut config: BridgeConfig) -> BridgeConfig {
    config.backoff = [50, 100, 200, 400, 750].map(Duration::from_millis).to_vec();
    config
}

impl Rig {
    pub async fn ready() -> Rig {
        let rig = Rig::listening().await;
        rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
        rig
    }

    pub async fn ready_with(tune: impl FnOnce(BridgeConfig) -> BridgeConfig) -> Rig {
        let fake = FakeBridge::new();
        fake.add_account(1, "fake", "Fake 1", "fake.example").await;
        let rig = Rig::with_fake(fake, true, tune).await;
        rig.client.wait_status(|s| s == BridgeStatus::Ready).await;
        rig
    }

    pub async fn listening() -> Rig {
        let fake = FakeBridge::new();
        fake.add_account(1, "fake", "Fake 1", "fake.example").await;
        Rig::with_fake(fake, true, |c| c).await
    }

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

/// Polls on the (possibly virtual) clock.
pub async fn eventually(what: &str, mut cond: impl FnMut() -> bool) {
    for _ in 0..400 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}
