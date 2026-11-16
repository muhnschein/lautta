// SPDX-License-Identifier: LGPL-2.1-or-later
//! Pages as the screen shows them. Removing and moving happen here at once; the session follows
//! through its queue and reports its pages after every job, some of them still behind.

use lautta_core::scan::session::PageInfo;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Change(usize),
    Append(PageInfo),
}

#[derive(Default)]
pub struct Rows {
    pub pages: Vec<PageInfo>,
    /// Removed on screen; the session may still report them.
    gone: HashSet<u32>,
}

impl Rows {
    pub fn index_of(&self, id: u32) -> Option<usize> {
        self.pages.iter().position(|p| p.id == id)
    }

    pub fn forget(&mut self, index: usize) -> PageInfo {
        let page = self.pages.remove(index);
        self.gone.insert(page.id);
        page
    }

    /// What brings the screen up to the session's report: changed pages in place, unknown ones
    /// at the end. Order on screen stays: moves were made here first.
    pub fn steps(&self, reported: &[PageInfo]) -> Vec<Step> {
        reported
            .iter()
            .filter(|p| !self.gone.contains(&p.id))
            .filter_map(|p| match self.index_of(p.id) {
                Some(i) if self.pages[i] == *p => None,
                Some(i) => Some(Step::Change(i)),
                None => Some(Step::Append(p.clone())),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(id: u32, view: &str) -> PageInfo {
        PageInfo {
            id,
            picture: format!("/s/work-{id}.jpg"),
            view: view.to_owned(),
            quad: [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0],
            turns: 0,
            colour: "original",
            found: false,
        }
    }

    #[test]
    fn new_pages_are_appended_and_changed_ones_marked() {
        let mut rows = Rows::default();
        assert_eq!(rows.steps(&[page(1, "a")]), [Step::Append(page(1, "a"))]);
        rows.pages = vec![page(1, "a"), page(2, "b")];
        assert_eq!(rows.steps(&[page(1, "a"), page(2, "b")]), []);
        let report = [page(1, "a"), page(2, "b2"), page(3, "c")];
        assert_eq!(rows.steps(&report), [Step::Change(1), Step::Append(page(3, "c"))]);
        assert_eq!((rows.index_of(2), rows.index_of(9)), (Some(1), None));
    }

    #[test]
    fn page_moved_on_screen_is_changed_at_its_new_place() {
        let rows = Rows {
            pages: vec![page(2, "b"), page(1, "a")],
            ..Rows::default()
        };
        // Session has not moved yet.
        assert_eq!(rows.steps(&[page(1, "a2"), page(2, "b")]), [Step::Change(1)]);
    }

    #[test]
    fn page_removed_on_screen_does_not_come_back() {
        let mut rows = Rows {
            pages: vec![page(1, "a"), page(2, "b")],
            ..Rows::default()
        };
        assert_eq!(rows.forget(0), page(1, "a"));
        assert_eq!(rows.pages, [page(2, "b")]);
        // Session still has page 1 when an earlier job reports.
        assert_eq!(rows.steps(&[page(1, "a"), page(2, "b")]), []);
        assert_eq!(
            rows.steps(&[page(2, "b"), page(3, "c")]),
            [Step::Append(page(3, "c"))]
        );
    }
}
