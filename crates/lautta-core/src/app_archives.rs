// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::app::Core;
use crate::error::Result;
use crate::locations::{Location, LocationKind, LocationRegistry};
use crate::password::Password;
use crate::provider::archive::{archive_location_id, ArchiveProvider};
use crate::room::Room;
use crate::transfer::{Engine, TransferId};
use crate::uri::Uri;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::runtime::Handle;

#[derive(Clone, Default)]
pub(crate) struct LockedArchives(Arc<Mutex<HashMap<String, usize>>>);

impl LockedArchives {
    fn with<R>(&self, f: impl FnOnce(&mut HashMap<String, usize>) -> R) -> R {
        f(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

#[derive(Clone, Default)]
pub struct ArchiveHold(Option<Arc<Held>>);

impl ArchiveHold {
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}

impl fmt::Debug for ArchiveHold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_some() {
            "ArchiveHold(held)"
        } else {
            "ArchiveHold(none)"
        })
    }
}

impl PartialEq for ArchiveHold {
    fn eq(&self, _: &ArchiveHold) -> bool {
        true
    }
}

impl Eq for ArchiveHold {}

struct Held {
    id: String,
    locked: LockedArchives,
    engine: Engine,
    locations: LocationRegistry,
    runtime: Handle,
}

impl Drop for Held {
    fn drop(&mut self) {
        self.locked.with(|m| {
            if let Some(holds) = m.get_mut(&self.id) {
                *holds = holds.saturating_sub(1);
            }
        });
        let _runtime = self.runtime.enter();
        close_idle(&self.locked, &self.engine, &self.locations);
    }
}

fn readers(engine: &Engine, id: &str) -> Vec<TransferId> {
    engine
        .list()
        .into_iter()
        .filter(|s| !s.state.is_finished())
        .filter(|s| engine.first_item(s.id).is_some_and(|p| p.src.location == id))
        .map(|s| s.id)
        .collect()
}

fn close_idle(locked: &LockedArchives, engine: &Engine, locations: &LocationRegistry) {
    let idle: Vec<String> = locked.with(|m| {
        m.iter()
            .filter(|(_, holds)| **holds == 0)
            .map(|(id, _)| id.clone())
            .collect()
    });
    for id in idle {
        let busy = readers(engine, &id);
        if busy.is_empty() {
            locked.with(|m| {
                if m.get(&id) == Some(&0) {
                    m.remove(&id);
                    locations.unregister(&id);
                }
            });
            continue;
        }
        let (locked, engine, locations) = (locked.clone(), engine.clone(), locations.clone());
        tokio::spawn(async move {
            for t in busy {
                let _ = engine.wait_for(t, |s| s.state.is_finished()).await;
            }
            close_idle(&locked, &engine, &locations);
        });
    }
}

impl Core {
    pub async fn open_archive(&self, uri: &Uri, password: Option<Password>) -> Result<(String, ArchiveHold)> {
        let id = archive_location_id(uri);
        if let Some(hold) = self.hold_if_open(&id) {
            return Ok((id, hold));
        }
        let source = self.provider(&uri.location)?;
        let cache = self.paths.cache_dir().join("archives");
        tokio::fs::create_dir_all(&cache).await?;
        let provider =
            ArchiveProvider::open(source, uri.path.clone(), cache, Room::default(), password).await?;
        let name = uri
            .path
            .name()
            .map(crate::vpath::display_name)
            .unwrap_or_default();
        let location = Location::remote(id.clone(), LocationKind::Archive, name, Some(uri.to_string()));
        let hold = if provider.locked() {
            self.locked_archives
                .with(|m| *m.entry(id.clone()).or_insert(0) += 1);
            self.held(&id)
        } else {
            ArchiveHold::default()
        };
        self.locations.register(location, Arc::new(provider));
        Ok((id, hold))
    }

    pub fn hold_archive(&self, id: &str) -> ArchiveHold {
        self.hold_if_open(id).unwrap_or_default()
    }

    fn hold_if_open(&self, id: &str) -> Option<ArchiveHold> {
        let locked = self
            .locked_archives
            .with(|m| m.get_mut(id).map(|holds| *holds += 1).is_some());
        if locked {
            return Some(self.held(id));
        }
        self.locations.get(id).map(|_| ArchiveHold::default())
    }

    fn held(&self, id: &str) -> ArchiveHold {
        ArchiveHold(Some(Arc::new(Held {
            id: id.to_owned(),
            locked: self.locked_archives.clone(),
            engine: self.engine.clone(),
            locations: self.locations.clone(),
            runtime: Handle::current(),
        })))
    }
}
