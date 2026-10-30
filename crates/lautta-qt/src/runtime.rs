// SPDX-License-Identifier: LGPL-2.1-or-later
//! Process-wide core and tokio runtime. [`spawn_then`] is the one way Qt layer runs async work:
//! result returns to the GUI thread.

use lautta_core::app::Core;
use lautta_core::paths::AppPaths;
use lautta_core::Result;
use std::future::Future;
use std::sync::{Arc, OnceLock};
use tokio::runtime::{Handle, Runtime};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static CORE: OnceLock<Arc<Core>> = OnceLock::new();
static START_ERROR: OnceLock<String> = OnceLock::new();

/// Two workers for orchestration, bridge client and scheduler; blocking file system work goes to
/// `spawn_blocking`.
fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("lautta-rt")
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("cannot start the runtime: {e}");
                std::process::abort()
            }
        }
    })
}

pub fn handle() -> Handle {
    runtime().handle().clone()
}

pub fn init(paths: AppPaths) -> Result<Arc<Core>> {
    if let Some(core) = CORE.get() {
        return Ok(core.clone());
    }
    match runtime().block_on(Core::open(paths)) {
        Ok(core) => Ok(CORE.get_or_init(|| core).clone()),
        Err(e) => {
            let _ = START_ERROR.set(format!("{e}"));
            Err(e)
        }
    }
}

pub fn start_error() -> String {
    START_ERROR.get().cloned().unwrap_or_default()
}

/// Facades are created only after [`init`] succeeded.
pub fn core() -> Option<Arc<Core>> {
    CORE.get().cloned()
}

/// `done` runs on the calling (GUI) thread. Result dropped if that thread's event loop is gone.
pub fn spawn_then<T, F, D>(fut: F, done: D)
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
    D: FnOnce(T) + 'static,
{
    let mut done = Some(done);
    let callback = qmetaobject::queued_callback(move |value: T| {
        if let Some(d) = done.take() {
            d(value);
        }
    });
    runtime().spawn(async move {
        callback(fut.await);
    });
}

pub fn blocking_then<T, F, D>(work: F, done: D)
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
    D: FnOnce(T) + 'static,
{
    spawn_then(
        async move {
            match tokio::task::spawn_blocking(work).await {
                Ok(v) => Some(v),
                Err(e) => {
                    log::error!("blocking task failed: {e}");
                    None
                }
            }
        },
        move |v| {
            if let Some(v) = v {
                done(v);
            }
        },
    );
}
