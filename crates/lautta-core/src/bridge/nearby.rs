// SPDX-License-Identifier: LGPL-2.1-or-later
//! Nearby servers across a restart of the discovery (LOC-6).
//!
//! netvfs forgets what it found when the last client stops looking and, after
//! a new `Discover(true)`, reports its fresh, empty cache right away; the
//! servers come back a moment later, one answer at a time. Browse turns the
//! discovery off whenever it is not on top, so without care every return to
//! it would empty the Nearby section and fill it again. While the discovery
//! settles after a start, the servers shown so far stay and new ones join
//! them; when it has settled, the newest list is taken as it is.

use super::types::NearbyServer;

#[derive(Debug, Default)]
pub(super) struct Settle {
    /// Number of the current settling window; a window that ended is stale.
    generation: u64,
    settling: bool,
    /// The newest list the bridge reported in the window.
    latest: Option<Vec<NearbyServer>>,
}

impl Settle {
    /// The discovery was (re)started: a window opens. Returns its number,
    /// for [`Settle::finish`].
    pub fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.settling = true;
        self.latest = None;
        self.generation
    }

    /// A `NearbyChanged` with `list`: what to show, given that `shown` is
    /// shown now.
    pub fn receive(&mut self, shown: &[NearbyServer], list: Vec<NearbyServer>) -> Vec<NearbyServer> {
        if !self.settling {
            return list;
        }
        let merged = merge(shown, &list);
        self.latest = Some(list);
        merged
    }

    /// The connection or the consent is gone: no window is open any more.
    pub fn cancel(&mut self) {
        self.settling = false;
        self.latest = None;
    }

    /// Window `generation` is over: the newest list of the window, if the
    /// bridge reported any and the window is still the current one.
    pub fn finish(&mut self, generation: u64) -> Option<Vec<NearbyServer>> {
        if generation != self.generation || !self.settling {
            return None;
        }
        self.settling = false;
        self.latest.take()
    }
}

/// `shown` in its order with entries updated from `list`, then the servers of
/// `list` that are new: rows keep their places while answers come in.
fn merge(shown: &[NearbyServer], list: &[NearbyServer]) -> Vec<NearbyServer> {
    let mut merged: Vec<NearbyServer> = shown
        .iter()
        .map(|old| {
            list.iter()
                .find(|new| same_server(old, new))
                .unwrap_or(old)
                .clone()
        })
        .collect();
    for new in list {
        if !shown.iter().any(|old| same_server(old, new)) {
            merged.push(new.clone());
        }
    }
    merged
}

/// The same service: a renamed one stays one row.
fn same_server(a: &NearbyServer, b: &NearbyServer) -> bool {
    a.provider == b.provider && a.host == b.host && a.port == b.port && a.path == b.path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vpath::VPath;

    fn server(name: &str, host: &str) -> NearbyServer {
        NearbyServer {
            name: name.into(),
            provider: "smb".into(),
            host: host.into(),
            port: 445,
            path: VPath::default(),
        }
    }

    #[test]
    fn outside_a_window_lists_are_taken_as_they_are() {
        let mut s = Settle::default();
        let a = server("A", "a.local");
        assert_eq!(s.receive(&[a.clone()], vec![]), vec![]);
        assert_eq!(s.receive(&[], vec![a.clone()]), vec![a]);
    }

    #[test]
    fn a_restart_keeps_what_is_shown_until_it_settled() {
        let (a, b, c) = (
            server("A", "a.local"),
            server("B", "b.local"),
            server("C", "c.local"),
        );
        let mut s = Settle::default();
        let window = s.begin();
        let shown = vec![a.clone(), b.clone()];
        // The fresh cache of netvfs, then the answers one by one.
        let shown = s.receive(&shown, vec![]);
        assert_eq!(shown, vec![a.clone(), b.clone()]);
        let shown = s.receive(&shown, vec![c.clone(), b.clone()]);
        assert_eq!(shown, vec![a.clone(), b.clone(), c.clone()]);
        // Settled: A did not answer, it is gone.
        assert_eq!(s.finish(window), Some(vec![c.clone(), b.clone()]));
        assert_eq!(s.receive(&[b.clone(), c.clone()], vec![c.clone()]), vec![c]);
    }

    #[test]
    fn a_renamed_server_is_updated_in_place() {
        let mut s = Settle::default();
        s.begin();
        let renamed = server("New name", "a.local");
        let shown = s.receive(&[server("A", "a.local")], vec![renamed.clone()]);
        assert_eq!(shown, vec![renamed]);
    }

    #[test]
    fn a_window_without_news_keeps_the_list() {
        let mut s = Settle::default();
        let window = s.begin();
        assert_eq!(s.finish(window), None);
        // Finished once only.
        s.receive(&[], vec![]);
        assert_eq!(s.finish(window), None);
    }

    #[test]
    fn a_cancelled_window_changes_nothing() {
        let mut s = Settle::default();
        let window = s.begin();
        s.receive(&[], vec![server("A", "a.local")]);
        s.cancel();
        assert_eq!(s.finish(window), None);
        assert_eq!(s.receive(&[server("A", "a.local")], vec![]), vec![]);
    }

    #[test]
    fn only_the_newest_window_finishes() {
        let mut s = Settle::default();
        let first = s.begin();
        s.receive(&[], vec![server("A", "a.local")]);
        let second = s.begin();
        assert_eq!(s.finish(first), None, "a stale window changes nothing");
        let shown = s.receive(&[server("A", "a.local")], vec![]);
        assert_eq!(shown.len(), 1, "the second window still settles");
        assert_eq!(s.finish(second), Some(vec![]));
    }
}
