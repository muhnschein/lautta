// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::json::to_json;
use crate::runtime::{core, handle};
use qmetaobject::prelude::*;
use qmetaobject::{queued_callback, QPointer};
use serde::Serialize;

#[derive(Serialize)]
struct Line {
    location: String,
    name: String,
    local: bool,
    kind: &'static str,
    state: &'static str,
    names: i64,
    #[serde(rename = "checkedMs")]
    checked_ms: i64,
    wanted: bool,
}

#[derive(QObject, Default)]
pub struct IndexInfo {
    base: qt_base_class!(trait QObject),
    statusJson: qt_property!(QString; NOTIFY changed),
    running: qt_property!(bool; NOTIFY changed),
    changed: qt_signal!(),

    refresh: qt_method!(fn(&mut self)),
    setServerIndexed: qt_method!(fn(&mut self, location: QString, on: bool)),
    following: bool,
}

fn kind_of(core: &lautta_core::app::Core, location: &str) -> &'static str {
    use lautta_core::locations::LocationKind;
    match core.locations.get(location).map(|l| l.kind) {
        Some(LocationKind::Volume) => "volume",
        Some(LocationKind::Server { .. }) => "server",
        _ => "device",
    }
}

fn lines(core: &lautta_core::app::Core) -> Vec<Line> {
    let mut out: Vec<Line> = core
        .index_status()
        .into_iter()
        .map(|s| Line {
            kind: kind_of(core, &s.location),
            location: s.location,
            name: s.name,
            local: s.local,
            state: s.state.name(),
            names: s.names,
            checked_ms: s.checked_ms.unwrap_or(-1),
            wanted: true,
        })
        .collect();
    for l in core.locations.locations() {
        let server = matches!(l.kind, lautta_core::locations::LocationKind::Server { .. });
        if server && !out.iter().any(|x| x.location == l.id) {
            out.push(Line {
                kind: "server",
                location: l.id,
                name: l.name,
                local: false,
                state: "off",
                names: 0,
                checked_ms: -1,
                wanted: false,
            });
        }
    }
    out
}

impl IndexInfo {
    fn refresh(&mut self) {
        let Some(core) = core() else { return };
        self.statusJson = QString::from(to_json(&lines(&core)).as_str());
        self.running = core
            .index
            .as_ref()
            .is_some_and(|i| i.running().is_some() || i.servers_running());
        self.changed();
        if !self.following {
            self.following = true;
            self.follow();
        }
    }

    fn follow(&mut self) {
        let Some(core) = core() else { return };
        let Some(index) = &core.index else { return };
        let mut changes = index.subscribe();
        let me = QPointer::from(&*self);
        let post = queued_callback(move |()| {
            if let Some(info) = me.as_pinned() {
                info.borrow_mut().refresh();
            }
        });
        handle().spawn(async move {
            while changes.changed().await.is_ok() {
                post(());
            }
        });
    }

    fn setServerIndexed(&mut self, location: QString, on: bool) {
        let Some(core) = core() else { return };
        let id = location.to_string();
        let mut prefs = core
            .location_prefs
            .get(&id)
            .unwrap_or_else(|_| lautta_core::settings::LocationPrefs::new(&id));
        prefs.index_names = on;
        if core.location_prefs.set(&prefs).is_ok() {
            core.index_forget_unwanted();
            core.index_restart();
        }
        self.refresh();
    }
}
