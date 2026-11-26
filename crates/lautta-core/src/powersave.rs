// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::app::Core;
use futures::StreamExt;
use std::future::Future;
use std::sync::{Arc, Weak};
use zbus::message::Type;
use zbus::{Connection, MatchRule, MessageStream};

const MCE: &str = "com.nokia.mce";

pub fn spawn<F>(core: &Arc<Core>, connect: F)
where
    F: Future<Output = zbus::Result<Connection>> + Send + 'static,
{
    let core = Arc::downgrade(core);
    tokio::spawn(async move {
        let result = match connect.await {
            Ok(conn) => follow_owned(conn, core).await,
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            log::info!("battery saving not followed: {e}");
        }
    });
}

pub async fn follow_owned(conn: Connection, core: Weak<Core>) -> zbus::Result<()> {
    let conn = &conn;
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface("com.nokia.mce.signal")?
        .member("psm_state_ind")?
        .build();
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

pub fn spawn_charger<F>(core: &Arc<Core>, connect: F)
where
    F: Future<Output = zbus::Result<Connection>> + Send + 'static,
{
    let core = Arc::downgrade(core);
    tokio::spawn(async move {
        let result = match connect.await {
            Ok(conn) => follow_charger(conn, core).await,
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            log::info!("charger not followed: {e}");
        }
    });
}

async fn ask<T: serde::de::DeserializeOwned + zbus::zvariant::Type>(
    conn: &Connection,
    member: &str,
) -> zbus::Result<T> {
    let reply = conn
        .call_method(
            Some(MCE),
            "/com/nokia/mce/request",
            Some("com.nokia.mce.request"),
            member,
            &(),
        )
        .await?;
    reply.body().deserialize()
}

pub fn battery_percent(level: i32) -> Option<u8> {
    u8::try_from(level).ok().filter(|l| *l <= 100)
}

pub async fn follow_charger(conn: Connection, core: Weak<Core>) -> zbus::Result<()> {
    let conn = &conn;
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface("com.nokia.mce.signal")?
        .build();
    let mut signals = MessageStream::for_match_rule(rule, conn, None).await?;
    let charging = ask::<String>(conn, "get_charger_state").await? == "on";
    let level = battery_percent(ask::<i32>(conn, "get_battery_level").await?);
    let Some(c) = core.upgrade() else { return Ok(()) };
    c.index_set_conditions(|k| {
        k.charging = charging;
        k.battery = level;
    });
    drop(c);
    while let Some(msg) = signals.next().await {
        let msg = msg?;
        let member = msg.header().member().map(|m| m.to_string()).unwrap_or_default();
        let Some(c) = core.upgrade() else { return Ok(()) };
        match member.as_str() {
            "charger_state_ind" => {
                if let Ok(state) = msg.body().deserialize::<String>() {
                    c.index_set_conditions(|k| k.charging = state == "on");
                }
            }
            "battery_level_ind" => {
                if let Ok(level) = msg.body().deserialize::<i32>() {
                    c.index_set_conditions(|k| k.battery = battery_percent(level));
                }
            }
            _ => {}
        }
    }
    Ok(())
}
