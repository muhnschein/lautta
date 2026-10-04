// SPDX-License-Identifier: LGPL-2.1-or-later
//! The model behind a directory page, free of Qt (ARC-8, BRW-5, BRW-8).
//!
//! `ListingState` owns the raw entries, the current sort and filter and the
//! visible permutation. Every mutation returns [`ListChange`]s that, applied
//! in order to a mirror of the previous visible list, reproduce the new one;
//! rows that did not change keep their place, so scroll position and
//! selection survive a revalidation.
//!
//! Change application contract (matches Qt's `begin*Rows` calls): removals
//! come first in descending index order, then inserts and updates in
//! ascending order of their final index. `Insert { index, count }` therefore
//! refers to rows `index..index + count` of the new visible list, and
//! `Update { index }` to row `index` of the new visible list.

use crate::entry::Entry;
use crate::filter::FilterOptions;
use crate::sort::{sort_indices, SortKey, SortOptions, SortRecord};
use crate::viewprefs::ViewPrefs;
use std::collections::{HashMap, HashSet};

/// BRW-8: above this many entries the page falls back to name order, no
/// sections and no thumbnails.
pub const LARGE_FOLDER_THRESHOLD: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListChange {
    Insert {
        index: usize,
        count: usize,
    },
    Remove {
        index: usize,
        count: usize,
    },
    Update {
        index: usize,
    },
    /// Everything changed (for example the large-folder fallback kicked in).
    Reset,
}

pub struct ListingState {
    raw: Vec<Entry>,
    records: Vec<SortRecord>,
    by_name: HashMap<Vec<u8>, u32>,
    visible: Vec<u32>,
    sort: SortOptions,
    filter: FilterOptions,
    selection: HashSet<Vec<u8>>,
    stale: bool,
    loading: bool,
}

impl Default for ListingState {
    fn default() -> ListingState {
        ListingState::new(SortOptions::default(), FilterOptions::default())
    }
}

impl ListingState {
    pub fn new(sort: SortOptions, filter: FilterOptions) -> ListingState {
        ListingState {
            raw: Vec::new(),
            records: Vec::new(),
            by_name: HashMap::new(),
            visible: Vec::new(),
            sort,
            filter,
            selection: HashSet::new(),
            stale: false,
            loading: true,
        }
    }

    /// Starts from a folder's view settings (BRW-4).
    pub fn from_prefs(prefs: &ViewPrefs) -> ListingState {
        ListingState::new(
            prefs.sort_options(),
            FilterOptions::with_hidden(prefs.show_hidden),
        )
    }

    // ---- reading ----

    /// Number of visible rows.
    pub fn len(&self) -> usize {
        self.visible.len()
    }

    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }

    /// Entries held before filtering.
    pub fn total_len(&self) -> usize {
        self.raw.len()
    }

    pub fn entry_at(&self, visible_index: usize) -> Option<&Entry> {
        let raw_index = *self.visible.get(visible_index)?;
        self.raw.get(raw_index as usize)
    }

    /// Visible row of the entry called `name`, if it is shown.
    pub fn index_of(&self, name: &[u8]) -> Option<usize> {
        let raw_index = *self.by_name.get(name)?;
        self.visible.iter().position(|&r| r == raw_index)
    }

    pub fn visible_entries(&self) -> impl Iterator<Item = &Entry> + '_ {
        self.visible.iter().map(|&r| &self.raw[r as usize])
    }

    /// Raw indices of the visible rows in display order (ARC-8).
    pub fn permutation(&self) -> &[u32] {
        &self.visible
    }

    /// The entry with this name regardless of filtering.
    pub fn entry_named(&self, name: &[u8]) -> Option<&Entry> {
        self.by_name.get(name).map(|&i| &self.raw[i as usize])
    }

    pub fn sort(&self) -> &SortOptions {
        &self.sort
    }

    pub fn filter(&self) -> &FilterOptions {
        &self.filter
    }

    /// Order actually used: the chosen one, or name order for large folders.
    pub fn effective_sort(&self) -> SortOptions {
        if self.is_large() {
            SortOptions {
                key: SortKey::Name,
                descending: false,
                folders_first: self.sort.folders_first,
            }
        } else {
            self.sort
        }
    }

    pub fn is_large(&self) -> bool {
        self.raw.len() > LARGE_FOLDER_THRESHOLD
    }

    /// Alphabetical section headers are off for large folders (BRW-8).
    pub fn sections_enabled(&self) -> bool {
        !self.is_large()
    }

    pub fn thumbnails_enabled(&self) -> bool {
        !self.is_large()
    }

    pub fn is_stale(&self) -> bool {
        self.stale
    }

    pub fn set_stale(&mut self, stale: bool) {
        self.stale = stale;
    }

    /// True until [`finish`](Self::finish) or a revalidation completes.
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    // ---- loading ----

    /// Entries arriving incrementally from a provider listing. An entry with
    /// a name already present replaces it.
    pub fn apply_batch(&mut self, batch: Vec<Entry>) -> Vec<ListChange> {
        let was_large = self.is_large();
        let old_visible = self.visible.clone();
        let mut changed = HashSet::new();
        for entry in batch {
            if let Some(i) = self.upsert(entry) {
                changed.insert(i);
            }
        }
        self.reconcile(old_visible, was_large, &changed)
    }

    /// The provider listing is complete.
    pub fn finish(&mut self) {
        self.loading = false;
        self.stale = false;
    }

    /// Shows a cached listing marked stale (BRW-5); follow with
    /// [`replace_with_fresh`](Self::replace_with_fresh) once revalidated.
    pub fn show_cached(&mut self, cached: Vec<Entry>) -> Vec<ListChange> {
        let changes = self.apply_batch(cached);
        self.stale = true;
        changes
    }

    /// Replaces the contents with a freshly fetched full listing and returns
    /// the diff against what was shown. Entries are matched by name, so rows
    /// that survived stay where they are; selection of vanished names is
    /// dropped.
    pub fn replace_with_fresh(&mut self, fresh: Vec<Entry>) -> Vec<ListChange> {
        let was_large = self.is_large();
        let old_raw = std::mem::take(&mut self.raw);
        let old_visible = std::mem::take(&mut self.visible);
        let old_by_name = std::mem::take(&mut self.by_name);
        self.records.clear();
        for entry in fresh {
            self.upsert(entry);
        }
        let by_name = &self.by_name;
        self.selection.retain(|n| by_name.contains_key(n));
        self.stale = false;
        self.loading = false;

        let new_visible = self.compute_visible();
        let changes = if was_large != self.is_large() {
            vec![ListChange::Reset]
        } else {
            let mut old_pos = vec![None; old_raw.len()];
            for (i, &r) in old_visible.iter().enumerate() {
                old_pos[r as usize] = Some(i);
            }
            let mapping: Vec<Option<usize>> = new_visible
                .iter()
                .map(|&r| {
                    let name = &self.raw[r as usize].name;
                    old_by_name.get(name).and_then(|&o| old_pos[o as usize])
                })
                .collect();
            diff_from_mapping(old_visible.len(), &mapping, |j, i| {
                old_raw[old_visible[i] as usize] != self.raw[new_visible[j] as usize]
            })
        };
        self.visible = new_visible;
        changes
    }

    // ---- sort and filter ----

    pub fn set_sort(&mut self, sort: SortOptions) -> Vec<ListChange> {
        if sort == self.sort {
            return Vec::new();
        }
        self.sort = sort;
        let old_visible = self.visible.clone();
        self.reconcile(old_visible, self.is_large(), &HashSet::new())
    }

    pub fn set_filter(&mut self, filter: FilterOptions) -> Vec<ListChange> {
        if filter == self.filter {
            return Vec::new();
        }
        self.filter = filter;
        let old_visible = self.visible.clone();
        self.reconcile(old_visible, self.is_large(), &HashSet::new())
    }

    // ---- selection (by name, so it survives refreshes) ----

    pub fn is_selected(&self, name: &[u8]) -> bool {
        self.selection.contains(name)
    }

    pub fn selection_len(&self) -> usize {
        self.selection.len()
    }

    /// Selects an entry of this folder; unknown names are ignored.
    pub fn select(&mut self, name: &[u8]) -> Vec<ListChange> {
        if !self.by_name.contains_key(name) || !self.selection.insert(name.to_vec()) {
            return Vec::new();
        }
        self.row_update(name)
    }

    pub fn toggle(&mut self, name: &[u8]) -> Vec<ListChange> {
        if self.selection.remove(name) {
            return self.row_update(name);
        }
        self.select(name)
    }

    /// Selects every visible row.
    pub fn select_all(&mut self) -> Vec<ListChange> {
        let mut changes = Vec::new();
        for (i, &r) in self.visible.iter().enumerate() {
            if self.selection.insert(self.raw[r as usize].name.clone()) {
                changes.push(ListChange::Update { index: i });
            }
        }
        changes
    }

    pub fn clear_selection(&mut self) -> Vec<ListChange> {
        let changes = self
            .visible
            .iter()
            .enumerate()
            .filter(|(_, &r)| self.selection.contains(&self.raw[r as usize].name))
            .map(|(index, _)| ListChange::Update { index })
            .collect();
        self.selection.clear();
        changes
    }

    /// Selected names: visible ones in display order, then selected entries
    /// currently hidden by the filter in byte order.
    pub fn selected_names(&self) -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = self
            .visible_entries()
            .filter(|e| self.selection.contains(&e.name))
            .map(|e| e.name.clone())
            .collect();
        let shown: HashSet<&Vec<u8>> = out.iter().collect();
        let mut hidden: Vec<Vec<u8>> = self
            .selection
            .iter()
            .filter(|n| !shown.contains(n))
            .cloned()
            .collect();
        hidden.sort();
        out.extend(hidden);
        out
    }

    // ---- internals ----

    fn row_update(&self, name: &[u8]) -> Vec<ListChange> {
        self.index_of(name)
            .map(|index| vec![ListChange::Update { index }])
            .unwrap_or_default()
    }

    /// Inserts or replaces by name. Returns the raw index when an existing
    /// entry changed content.
    fn upsert(&mut self, entry: Entry) -> Option<u32> {
        if let Some(&i) = self.by_name.get(&entry.name) {
            if self.raw[i as usize] == entry {
                return None;
            }
            self.records[i as usize] = SortRecord::for_entry(&entry);
            self.raw[i as usize] = entry;
            return Some(i);
        }
        let i = self.raw.len() as u32;
        self.by_name.insert(entry.name.clone(), i);
        self.records.push(SortRecord::for_entry(&entry));
        self.raw.push(entry);
        None
    }

    fn compute_visible(&self) -> Vec<u32> {
        let mut idx: Vec<u32> = (0..self.raw.len() as u32)
            .filter(|&i| self.filter.matches(&self.raw[i as usize]))
            .collect();
        sort_indices(&mut idx, &self.raw, &self.records, &self.effective_sort());
        idx
    }

    /// Diff for changes that keep raw indices stable (batches, sort, filter).
    fn reconcile(
        &mut self,
        old_visible: Vec<u32>,
        was_large: bool,
        changed: &HashSet<u32>,
    ) -> Vec<ListChange> {
        let new_visible = self.compute_visible();
        if was_large != self.is_large() {
            self.visible = new_visible;
            return vec![ListChange::Reset];
        }
        let mut old_pos = vec![None; self.raw.len()];
        for (i, &r) in old_visible.iter().enumerate() {
            old_pos[r as usize] = Some(i);
        }
        let mapping: Vec<Option<usize>> = new_visible.iter().map(|&r| old_pos[r as usize]).collect();
        let changes = diff_from_mapping(old_visible.len(), &mapping, |j, _| {
            changed.contains(&new_visible[j])
        });
        self.visible = new_visible;
        changes
    }
}

/// Marks the positions of a longest strictly increasing subsequence of the
/// mapped old positions; those rows stay put.
fn lis_mask(mapping: &[Option<usize>]) -> Vec<bool> {
    let items: Vec<(usize, usize)> = mapping
        .iter()
        .enumerate()
        .filter_map(|(pos, m)| m.map(|v| (pos, v)))
        .collect();
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; items.len()];
    for (k, &(_, value)) in items.iter().enumerate() {
        let at = tails.partition_point(|&t| items[t].1 < value);
        prev[k] = at.checked_sub(1).map(|i| tails[i]);
        if at == tails.len() {
            tails.push(k);
        } else {
            tails[at] = k;
        }
    }
    let mut mask = vec![false; mapping.len()];
    let mut cur = tails.last().copied();
    while let Some(k) = cur {
        mask[items[k].0] = true;
        cur = prev[k];
    }
    mask
}

/// Removal runs of the old list, last run first so earlier indices stay valid.
fn removal_runs(kept_old: &[bool]) -> Vec<ListChange> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &kept) in kept_old.iter().enumerate() {
        match (kept, start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                runs.push(ListChange::Remove {
                    index: s,
                    count: i - s,
                });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        runs.push(ListChange::Remove {
            index: s,
            count: kept_old.len() - s,
        });
    }
    runs.reverse();
    runs
}

/// Inserts for the new rows that are not kept and updates for kept rows whose
/// content changed, in ascending order of final index.
fn inserts_and_updates(
    mapping: &[Option<usize>],
    keep: &[bool],
    changed: impl Fn(usize, usize) -> bool,
) -> Vec<ListChange> {
    let mut out = Vec::new();
    let mut run_start: Option<usize> = None;
    for (j, m) in mapping.iter().enumerate() {
        if keep[j] {
            if let Some(s) = run_start.take() {
                out.push(ListChange::Insert {
                    index: s,
                    count: j - s,
                });
            }
            if m.map_or(false, |i| changed(j, i)) {
                out.push(ListChange::Update { index: j });
            }
        } else if run_start.is_none() {
            run_start = Some(j);
        }
    }
    if let Some(s) = run_start {
        out.push(ListChange::Insert {
            index: s,
            count: mapping.len() - s,
        });
    }
    out
}

/// `mapping[j]` is the old visible position of new row `j` (None when the
/// entry was not shown before). `changed(j, i)` says whether new row `j` and
/// old row `i` differ in content.
fn diff_from_mapping(
    old_len: usize,
    mapping: &[Option<usize>],
    changed: impl Fn(usize, usize) -> bool,
) -> Vec<ListChange> {
    let keep = lis_mask(mapping);
    let mut kept_old = vec![false; old_len];
    for (j, m) in mapping.iter().enumerate() {
        if let (true, Some(i)) = (keep[j], m) {
            kept_old[*i] = true;
        }
    }
    let mut out = removal_runs(&kept_old);
    out.extend(inserts_and_updates(mapping, &keep, changed));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{ms_to_system_time, Kind};
    use std::time::Instant;

    fn file(name: &str, size: u64) -> Entry {
        let mut e = Entry::new(name.as_bytes(), Kind::File);
        e.size = Some(size);
        e
    }

    fn names(s: &ListingState) -> Vec<String> {
        s.visible_entries().map(Entry::display_name).collect()
    }

    fn visible(s: &ListingState) -> Vec<Entry> {
        s.visible_entries().cloned().collect()
    }

    /// Applies `changes` to `mirror` the way a view model would and checks it
    /// ends up equal to the state's visible list.
    fn apply_to_mirror(mirror: &mut Vec<Entry>, changes: &[ListChange], s: &ListingState) {
        for c in changes {
            match *c {
                ListChange::Insert { index, count } => {
                    let items: Vec<Entry> = (index..index + count)
                        .map(|k| s.entry_at(k).cloned().expect("insert beyond the new list"))
                        .collect();
                    assert!(index <= mirror.len(), "insert index out of range: {c:?}");
                    mirror.splice(index..index, items);
                }
                ListChange::Remove { index, count } => {
                    assert!(index + count <= mirror.len(), "remove out of range: {c:?}");
                    mirror.drain(index..index + count);
                }
                ListChange::Update { index } => {
                    mirror[index] = s.entry_at(index).cloned().expect("update beyond the new list");
                }
                ListChange::Reset => *mirror = visible(s),
            }
        }
        assert_eq!(*mirror, visible(s));
    }

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }

        fn chance(&mut self, percent: usize) -> bool {
            self.below(100) < percent
        }
    }

    fn random_entry(rng: &mut Rng) -> Entry {
        const STEMS: [&str; 9] = ["a", "B", "\u{e9}", "file", "File", "10", "9", ".dot", "x.png"];
        let name = format!("{}{}", STEMS[rng.below(STEMS.len())], rng.below(12));
        let kind = if rng.chance(25) { Kind::Dir } else { Kind::File };
        let mut e = Entry::new(name.as_bytes(), kind);
        e.size = Some(rng.below(5) as u64);
        e.modified = Some(ms_to_system_time(rng.below(4) as i64 * 1000));
        e
    }

    fn random_batch(rng: &mut Rng, max: usize) -> Vec<Entry> {
        (0..rng.below(max + 1)).map(|_| random_entry(rng)).collect()
    }

    fn random_sort(rng: &mut Rng) -> SortOptions {
        let key = [SortKey::Name, SortKey::Size, SortKey::Modified, SortKey::Type][rng.below(4)];
        SortOptions {
            key,
            descending: rng.chance(50),
            folders_first: rng.chance(50),
        }
    }

    fn random_filter(rng: &mut Rng) -> FilterOptions {
        let mut f = FilterOptions::with_hidden(rng.chance(50));
        f.set_text(["", "a", "e", "1", "file"][rng.below(5)]);
        if rng.chance(20) {
            f.chips.insert(crate::filter::TypeChip::Images);
        }
        f
    }

    #[test]
    fn diffs_reproduce_the_new_list_in_random_sequences() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for case in 0..400 {
            let mut s = ListingState::new(random_sort(&mut rng), random_filter(&mut rng));
            let mut mirror: Vec<Entry> = Vec::new();
            for step in 0..12 {
                let changes = match rng.below(5) {
                    0 | 1 => s.apply_batch(random_batch(&mut rng, 25)),
                    2 => s.replace_with_fresh(random_batch(&mut rng, 40)),
                    3 => s.set_sort(random_sort(&mut rng)),
                    _ => s.set_filter(random_filter(&mut rng)),
                };
                let ctx = format!("case {case} step {step}");
                apply_to_mirror(&mut mirror, &changes, &s);
                assert_eq!(mirror.len(), s.len(), "{ctx}");
            }
        }
    }

    #[test]
    fn replace_with_fresh_after_edits_reproduces_the_list() {
        // Revalidation of a mostly unchanged folder: some deleted, some
        // modified, some new.
        let mut rng = Rng(42);
        for case in 0..200 {
            let initial = random_batch(&mut rng, 40);
            let mut s = ListingState::new(random_sort(&mut rng), FilterOptions::default());
            s.apply_batch(initial.clone());
            let mut mirror = visible(&s);
            let mut fresh: Vec<Entry> = initial.into_iter().filter(|_| rng.chance(80)).collect();
            for e in &mut fresh {
                if rng.chance(20) {
                    e.size = Some(e.size.unwrap_or(0) + 7);
                }
            }
            fresh.extend(random_batch(&mut rng, 8));
            let changes = s.replace_with_fresh(fresh);
            apply_to_mirror(&mut mirror, &changes, &s);
            assert!(!s.is_stale(), "case {case}");
        }
    }

    #[test]
    fn unchanged_rows_keep_their_place() {
        let mut s = ListingState::default();
        s.apply_batch(vec![file("a", 1), file("b", 1), file("c", 1), file("d", 1)]);
        // b removed, e added at the end, c modified.
        let changes = s.replace_with_fresh(vec![file("a", 1), file("c", 9), file("d", 1), file("e", 1)]);
        assert_eq!(
            changes,
            [
                ListChange::Remove { index: 1, count: 1 },
                ListChange::Update { index: 1 },
                ListChange::Insert { index: 3, count: 1 },
            ]
        );
        assert_eq!(names(&s), ["a", "c", "d", "e"]);
    }

    #[test]
    fn identical_refresh_yields_no_changes() {
        let mut s = ListingState::default();
        let list = vec![file("a", 1), file("b", 2), Entry::new(b"dir", Kind::Dir)];
        s.apply_batch(list.clone());
        assert_eq!(s.replace_with_fresh(list), []);
        assert!(s.set_sort(SortOptions::default()).is_empty());
        assert!(s.set_filter(FilterOptions::default()).is_empty());
    }

    #[test]
    fn insert_runs_are_coalesced() {
        let mut s = ListingState::default();
        assert_eq!(
            s.apply_batch(vec![file("c", 1), file("a", 1), file("b", 1)]),
            [ListChange::Insert { index: 0, count: 3 }]
        );
        assert_eq!(
            s.apply_batch(vec![file("d", 1), file("e", 1), file("a", 1)]),
            [ListChange::Insert { index: 3, count: 2 }],
            "a duplicate of an unchanged entry changes nothing"
        );
        assert_eq!(names(&s), ["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn batch_replacing_an_entry_updates_that_row() {
        let mut s = ListingState::default();
        s.apply_batch(vec![file("a", 1), file("b", 1), file("c", 1)]);
        let changes = s.apply_batch(vec![file("b", 99)]);
        assert_eq!(changes, [ListChange::Update { index: 1 }]);
        assert_eq!(s.entry_at(1).and_then(|e| e.size), Some(99));
        assert_eq!(s.total_len(), 3);
    }

    #[test]
    fn resorting_moves_rows_without_losing_any() {
        let mut s = ListingState::default();
        s.apply_batch(vec![file("a", 3), file("b", 2), file("c", 1)]);
        let mut mirror = visible(&s);
        let changes = s.set_sort(SortOptions {
            key: SortKey::Size,
            ..SortOptions::default()
        });
        assert_eq!(names(&s), ["c", "b", "a"]);
        assert!(!changes.is_empty());
        apply_to_mirror(&mut mirror, &changes, &s);
    }

    #[test]
    fn filter_removes_and_restores_rows() {
        let mut s = ListingState::default();
        s.apply_batch(vec![
            file("apple", 1),
            file("banana", 1),
            file("avocado", 1),
            file(".hid", 1),
        ]);
        assert_eq!(names(&s), ["apple", "avocado", "banana"]);
        let mut f = FilterOptions::default();
        f.set_text("AV\u{d3}");
        let changes = s.set_filter(f.clone());
        assert_eq!(names(&s), ["avocado"]);
        assert_eq!(
            changes,
            [
                ListChange::Remove { index: 2, count: 1 },
                ListChange::Remove { index: 0, count: 1 }
            ]
        );
        f.set_text("");
        f.show_hidden = true;
        let changes = s.set_filter(f);
        assert_eq!(names(&s), [".hid", "apple", "avocado", "banana"]);
        assert_eq!(
            changes,
            [
                ListChange::Insert { index: 0, count: 2 },
                ListChange::Insert { index: 3, count: 1 }
            ]
        );
        assert_eq!(s.total_len(), 4);
    }

    #[test]
    fn lookup_helpers() {
        let mut s = ListingState::default();
        s.apply_batch(vec![file("b", 1), file("a", 1), file(".h", 1)]);
        assert_eq!(s.len(), 2);
        assert!(!s.is_empty());
        assert_eq!(s.index_of(b"a"), Some(0));
        assert_eq!(s.index_of(b"b"), Some(1));
        assert_eq!(s.index_of(b".h"), None, "hidden by the filter");
        assert_eq!(s.index_of(b"nope"), None);
        assert_eq!(s.entry_at(1).map(|e| e.name.as_slice()), Some(&b"b"[..]));
        assert!(s.entry_at(2).is_none());
        assert_eq!(s.permutation(), [1, 0]);
        assert!(s.entry_named(b".h").is_some());
        assert!(s.entry_named(b"zz").is_none());
        assert_eq!(s.sort().key, SortKey::Name);
        assert!(!s.filter().show_hidden);
    }

    #[test]
    fn loading_and_stale_flags() {
        let mut s = ListingState::default();
        assert!(s.is_loading());
        s.show_cached(vec![file("a", 1)]);
        assert!(s.is_stale());
        assert!(s.is_loading());
        s.replace_with_fresh(vec![file("a", 1)]);
        assert!(!s.is_stale());
        assert!(!s.is_loading());

        let mut s = ListingState::default();
        s.set_stale(true);
        s.apply_batch(vec![file("a", 1)]);
        assert!(s.is_stale(), "batches do not clear staleness");
        s.finish();
        assert!(!s.is_stale());
        assert!(!s.is_loading());
    }

    #[test]
    fn from_prefs_applies_sort_and_hidden() {
        let prefs = ViewPrefs {
            sort: SortKey::Size,
            descending: true,
            show_hidden: true,
            folders_first: false,
            ..ViewPrefs::default()
        };
        let mut s = ListingState::from_prefs(&prefs);
        s.apply_batch(vec![file(".h", 5), file("a", 1)]);
        assert_eq!(names(&s), [".h", "a"]);
        assert_eq!(s.sort().key, SortKey::Size);
    }

    #[test]
    fn selection_by_name_survives_refresh_and_resort() {
        let mut s = ListingState::default();
        s.apply_batch(vec![file("a", 1), file("b", 2), file("c", 3)]);
        assert_eq!(s.select(b"b"), [ListChange::Update { index: 1 }]);
        assert!(s.select(b"b").is_empty(), "already selected");
        assert!(s.select(b"missing").is_empty());
        s.toggle(b"c");
        assert_eq!(s.selection_len(), 2);
        assert_eq!(s.selected_names(), [b"b".to_vec(), b"c".to_vec()]);

        s.set_sort(SortOptions {
            descending: true,
            ..SortOptions::default()
        });
        assert_eq!(s.selected_names(), [b"c".to_vec(), b"b".to_vec()]);
        assert!(s.is_selected(b"b"));

        // Refresh: b modified and c deleted, d new.
        s.replace_with_fresh(vec![file("a", 1), file("b", 20), file("d", 1)]);
        assert!(s.is_selected(b"b"));
        assert!(!s.is_selected(b"c"), "vanished entries leave the selection");
        assert_eq!(s.selected_names(), [b"b".to_vec()]);

        assert_eq!(
            s.toggle(b"b"),
            [ListChange::Update {
                index: s.index_of(b"b").unwrap()
            }]
        );
        assert_eq!(s.selection_len(), 0);
        assert!(s.toggle(b"zzz").is_empty());
    }

    #[test]
    fn select_all_and_clear() {
        let mut s = ListingState::default();
        s.apply_batch(vec![file("a", 1), file("b", 1), file(".h", 1)]);
        s.select(b"a");
        let changes = s.select_all();
        assert_eq!(
            changes,
            [ListChange::Update { index: 1 }],
            "a was already selected"
        );
        assert_eq!(s.selection_len(), 2);
        assert!(!s.is_selected(b".h"), "hidden rows are not selected");
        let changes = s.clear_selection();
        assert_eq!(
            changes,
            [ListChange::Update { index: 0 }, ListChange::Update { index: 1 }]
        );
        assert_eq!(s.selection_len(), 0);
        assert!(s.clear_selection().is_empty());
    }

    #[test]
    fn selected_entries_hidden_by_filter_stay_selected() {
        let mut s = ListingState::default();
        s.apply_batch(vec![file("apple", 1), file("pear", 1), file("plum", 1)]);
        s.select(b"plum");
        s.select(b"apple");
        let mut f = FilterOptions::default();
        f.set_text("pear");
        s.set_filter(f);
        s.select(b"pear");
        assert_eq!(
            s.selected_names(),
            [b"pear".to_vec(), b"apple".to_vec(), b"plum".to_vec()],
            "visible first, then hidden in byte order"
        );
        assert!(s.select(b"plum").is_empty());
        assert!(
            s.toggle(b"apple").is_empty(),
            "deselecting a hidden row changes no row"
        );
        assert!(!s.is_selected(b"apple"));
    }

    fn many(n: usize) -> Vec<Entry> {
        (0..n)
            .map(|i| file(&format!("f{i:06}"), (i % 97) as u64))
            .collect()
    }

    #[test]
    fn large_folders_fall_back_to_name_order() {
        let mut s = ListingState::from_prefs(&ViewPrefs {
            sort: SortKey::Size,
            descending: true,
            ..ViewPrefs::default()
        });
        let mut mirror = Vec::new();
        let changes = s.apply_batch(many(LARGE_FOLDER_THRESHOLD));
        apply_to_mirror(&mut mirror, &changes, &s);
        assert!(!s.is_large());
        assert!(s.sections_enabled());
        assert!(s.thumbnails_enabled());
        assert_eq!(s.effective_sort().key, SortKey::Size);
        assert_eq!(s.entry_at(0).and_then(|e| e.size), Some(96));

        let changes = s.apply_batch(vec![file("zzz-extra", 1000)]);
        assert_eq!(changes, [ListChange::Reset]);
        apply_to_mirror(&mut mirror, &changes, &s);
        assert!(s.is_large());
        assert!(!s.sections_enabled());
        assert!(!s.thumbnails_enabled());
        let eff = s.effective_sort();
        assert_eq!((eff.key, eff.descending), (SortKey::Name, false));
        assert_eq!(
            s.entry_at(0).map(|e| e.display_name()).as_deref(),
            Some("f000000")
        );
        assert_eq!(s.sort().key, SortKey::Size, "the chosen order is kept for later");

        // Changing the chosen order does not reorder a large folder.
        assert!(s.set_sort(SortOptions::default()).is_empty());

        // Shrinking back below the threshold restores the chosen order.
        let changes = s.replace_with_fresh(many(10));
        assert_eq!(changes, [ListChange::Reset]);
        apply_to_mirror(&mut mirror, &changes, &s);
        assert!(!s.is_large());
    }

    #[test]
    fn sorting_twenty_thousand_entries_is_fast_enough() {
        let mut s = ListingState::default();
        let started = Instant::now();
        s.apply_batch(many(LARGE_FOLDER_THRESHOLD));
        let fresh = many(LARGE_FOLDER_THRESHOLD);
        assert!(s.replace_with_fresh(fresh).is_empty());
        s.set_sort(SortOptions {
            key: SortKey::Modified,
            descending: true,
            folders_first: true,
        });
        assert_eq!(s.len(), LARGE_FOLDER_THRESHOLD);
        assert!(
            started.elapsed().as_secs_f64() < 5.0,
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn lis_keeps_the_longest_run() {
        let m = [Some(0), Some(3), Some(1), Some(2), None, Some(4)];
        let mask = lis_mask(&m);
        assert_eq!(mask, [true, false, true, true, false, true]);
        assert_eq!(lis_mask(&[]), Vec::<bool>::new());
        assert_eq!(lis_mask(&[None, None]), [false, false]);
    }
}
