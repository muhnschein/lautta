// SPDX-License-Identifier: LGPL-2.1-or-later
//! Item count of a local folder for grid captions. Names only, no `stat`; hidden files left out
//! unless shown.

use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Bigger folder reports what was read and `capped`: bounds the work on a slow card.
pub const COUNT_CAP: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderCount {
    pub shown: usize,
    /// `shown` is a lower bound.
    pub capped: bool,
}

pub fn count_entries(dir: &Path, show_hidden: bool, cap: usize) -> io::Result<FolderCount> {
    let mut shown = 0;
    for (read, item) in std::fs::read_dir(dir)?.enumerate() {
        let item = item?;
        if read == cap {
            return Ok(FolderCount { shown, capped: true });
        }
        if show_hidden || item.file_name().as_bytes().first() != Some(&b'.') {
            shown += 1;
        }
    }
    Ok(FolderCount { shown, capped: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for name in files {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        dir
    }

    fn count(dir: &tempfile::TempDir, show_hidden: bool, cap: usize) -> FolderCount {
        count_entries(dir.path(), show_hidden, cap).unwrap()
    }

    #[test]
    fn the_cap_is_a_thousand_entries() {
        // "More than 1000 items" past it.
        assert_eq!(COUNT_CAP, 1000);
    }

    #[test]
    fn an_empty_folder_has_no_items() {
        let dir = folder(&[]);
        assert_eq!(
            count(&dir, false, COUNT_CAP),
            FolderCount {
                shown: 0,
                capped: false
            }
        );
    }

    #[test]
    fn files_and_folders_count_alike() {
        let dir = folder(&["a.txt", "b.txt"]);
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        assert_eq!(count(&dir, false, COUNT_CAP).shown, 3);
    }

    #[test]
    fn hidden_files_count_only_when_shown() {
        let dir = folder(&["a.txt", ".hidden", ".config"]);
        assert_eq!(count(&dir, false, COUNT_CAP).shown, 1);
        assert_eq!(count(&dir, true, COUNT_CAP).shown, 3);
    }

    #[test]
    fn only_a_leading_dot_hides() {
        let dir = folder(&["a.b", "c."]);
        assert_eq!(count(&dir, false, COUNT_CAP).shown, 2);
    }

    #[test]
    fn a_folder_at_the_cap_is_exact() {
        let dir = folder(&["1", "2", "3"]);
        assert_eq!(
            count(&dir, false, 3),
            FolderCount {
                shown: 3,
                capped: false
            }
        );
    }

    #[test]
    fn a_folder_over_the_cap_gives_a_lower_bound() {
        let dir = folder(&["1", "2", "3", "4"]);
        assert_eq!(
            count(&dir, false, 3),
            FolderCount {
                shown: 3,
                capped: true
            }
        );
    }

    #[test]
    fn the_cap_counts_entries_read_not_items_shown() {
        let dir = folder(&[".1", ".2", ".3", ".4"]);
        assert_eq!(
            count(&dir, false, 3),
            FolderCount {
                shown: 0,
                capped: true
            }
        );
        assert_eq!(
            count(&dir, true, 3),
            FolderCount {
                shown: 3,
                capped: true
            }
        );
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        let dir = folder(&[]);
        assert!(count_entries(&dir.path().join("gone"), false, COUNT_CAP).is_err());
    }

    #[test]
    fn a_file_is_not_a_folder() {
        let dir = folder(&["a.txt"]);
        assert!(count_entries(&dir.path().join("a.txt"), false, COUNT_CAP).is_err());
    }
}
