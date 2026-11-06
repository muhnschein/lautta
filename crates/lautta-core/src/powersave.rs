// SPDX-License-Identifier: LGPL-2.1-or-later
//! mce's battery saving into [`Core::set_power_save`]: `get_psm_state`, then each
//! `psm_state_ind`. Sailjail's Base permission lets apps call one and hear the other.

use crate::app::Core;
use futures::StreamExt;
use std::sync::{Arc, Weak};
use zbus::message::Type;
use zbus::{Connection, MatchRule, MessageStream};

const MCE: &str = "com.nokia.mce";

/// Off when the system bus or mce is missing (host, SDK).
pub(crate) fn spawn(core: &Arc<Core>) {
    let core = Arc::downgrade(core);
    tokio::spawn(async move {
        let result = match Connection::system().await {
            Ok(conn) => follow_owned(conn, core).await,
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            log::info!("battery saving not followed: {e}");
        }
    });
}

/// Returns when `core` is gone or the connection ends.
pub async fn follow_owned(conn: Connection, core: Weak<Core>) -> zbus::Result<()> {
    let conn = &conn;
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface("com.nokia.mce.signal")?
        .member("psm_state_ind")?
        .build();
    // Subscribed before asking: no change slips between the two.
    let mut changes = MessageStream::for_match_rule(rule, conn, None).await?;
    let reply = conn
        .call_method(
            Some(MCE),
            "/com/nokia/mce/request",
            Some("com.nokia.mce.request"),
            "get_psm_state",
            &(),
        )
        .await?;
    let Some(c) = core.upgrade() else { return Ok(()) };
    c.set_power_save(reply.body().deserialize()?);
    drop(c);
    while let Some(msg) = changes.next().await {
        let Some(c) = core.upgrade() else { return Ok(()) };
        if let Ok(on) = msg?.body().deserialize::<bool>() {
            c.set_power_save(on);
        }
    }
    Ok(())
}
