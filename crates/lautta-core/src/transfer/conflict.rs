// SPDX-License-Identifier: LGPL-2.1-or-later
//! Run-time conflict decisions (OPS-2). The planner may already have found a
//! conflict, but only the destination at the moment of writing is
//! authoritative, so the engine asks `decide` for every item. Pure logic.

use crate::entry::{Entry, Kind};
use crate::ops::{Conflict, ConflictChoice, PlanItem};

/// How to proceed when writing is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WriteMode {
    /// Overwrite the existing destination (rename with `Replace`).
    pub replace: bool,
    /// Continue the partial destination file in place (*Resume*).
    pub resume: bool,
    /// The destination folder exists and is merged into.
    pub merge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Proceed(WriteMode),
    Skip,
    KeepBoth,
    Ask(Conflict),
}

fn is_real_dir(e: &Entry) -> bool {
    e.kind == Kind::Dir
}

/// Builds the question for the UI. The default choice is never *Replace*
/// (OPS-2), which is why it is not listed first.
pub fn build_conflict(plan: &PlanItem, dst: &Entry, can_resume: bool) -> Conflict {
    let src_is_dir = plan.kind == Kind::Dir;
    let dst_is_dir = is_real_dir(dst);
    let resumable = !src_is_dir
        && !dst_is_dir
        && can_resume
        && matches!((dst.size, plan.size), (Some(d), Some(s)) if d > 0 && d < s);
    let choices = if src_is_dir && dst_is_dir {
        vec![
            ConflictChoice::Merge,
            ConflictChoice::KeepBoth,
            ConflictChoice::Skip,
        ]
    } else if src_is_dir || dst_is_dir {
        vec![ConflictChoice::KeepBoth, ConflictChoice::Skip]
    } else {
        let mut c = vec![
            ConflictChoice::KeepBoth,
            ConflictChoice::Skip,
            ConflictChoice::Replace,
            ConflictChoice::ReplaceIfNewer,
        ];
        if resumable {
            c.push(ConflictChoice::Resume);
        }
        c
    };
    Conflict {
        dst_is_dir,
        src_is_dir,
        src_size: plan.size,
        dst_size: dst.size,
        src_mtime_ms: plan.mtime_ms,
        dst_mtime_ms: dst.modified_ms(),
        resumable,
        choices,
    }
}

/// Decides what to do with `plan` given what exists at the destination.
/// `default` is the "apply to all remaining" answer.
pub fn decide(
    plan: &PlanItem,
    dst: Option<&Entry>,
    default: Option<ConflictChoice>,
    can_resume: bool,
) -> Decision {
    let Some(dst) = dst else {
        return Decision::Proceed(WriteMode::default());
    };
    let conflict = build_conflict(plan, dst, can_resume);
    let choice = plan.resolution.or(default);
    let both_dirs = conflict.src_is_dir && conflict.dst_is_dir;
    let both_files = !conflict.src_is_dir && !conflict.dst_is_dir;
    let replace = Decision::Proceed(WriteMode {
        replace: true,
        ..WriteMode::default()
    });
    match choice {
        Some(ConflictChoice::Skip) => Decision::Skip,
        Some(ConflictChoice::KeepBoth) => Decision::KeepBoth,
        // A folder "replaced" or "replaced if newer" merges: replacing a
        // whole tree would destroy files the user did not mention.
        Some(ConflictChoice::Merge | ConflictChoice::Replace | ConflictChoice::ReplaceIfNewer)
            if both_dirs =>
        {
            Decision::Proceed(WriteMode {
                merge: true,
                ..WriteMode::default()
            })
        }
        Some(ConflictChoice::Replace) if both_files => replace,
        Some(ConflictChoice::ReplaceIfNewer) if both_files => match (plan.mtime_ms, conflict.dst_mtime_ms) {
            (Some(s), Some(d)) if s > d => replace,
            _ => Decision::Skip,
        },
        Some(ConflictChoice::Resume) if conflict.resumable => Decision::Proceed(WriteMode {
            resume: true,
            ..WriteMode::default()
        }),
        _ => Decision::Ask(conflict),
    }
}

/// `name 2.ext`, `name 3.ext` (OPS-2 *Keep both*). Folders and dot files keep
/// their whole name as the stem.
pub fn numbered_name(name: &[u8], n: u32, is_dir: bool) -> Vec<u8> {
    let dot = if is_dir {
        None
    } else {
        name.iter().rposition(|b| *b == b'.').filter(|i| *i > 0)
    };
    let (stem, ext) = match dot {
        Some(i) => name.split_at(i),
        None => (name, &name[name.len()..]),
    };
    let mut out = stem.to_vec();
    out.extend_from_slice(format!(" {n}").as_bytes());
    out.extend_from_slice(ext);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::ms_to_system_time;
    use crate::transfer::testutil::plan_item;

    fn dst_entry(kind: Kind, size: u64, mtime_ms: i64) -> Entry {
        let mut e = Entry::new(b"x", kind);
        e.size = Some(size);
        e.modified = Some(ms_to_system_time(mtime_ms));
        e
    }

    fn file(size: u64) -> PlanItem {
        plan_item("x", "x", Kind::File, size)
    }

    fn resolved(mut p: PlanItem, c: ConflictChoice) -> PlanItem {
        p.resolution = Some(c);
        p
    }

    fn proceed(replace: bool, resume: bool, merge: bool) -> Decision {
        Decision::Proceed(WriteMode {
            replace,
            resume,
            merge,
        })
    }

    #[test]
    fn nothing_there_proceeds_even_with_a_resolution() {
        let p = resolved(file(5), ConflictChoice::Skip);
        assert_eq!(decide(&p, None, None, true), proceed(false, false, false));
    }

    #[test]
    fn unresolved_file_conflict_asks_and_never_defaults_to_replace() {
        let d = dst_entry(Kind::File, 3, 500);
        let Decision::Ask(c) = decide(&file(10), Some(&d), None, true) else {
            panic!("expected a question");
        };
        assert_eq!(c.choices[0], ConflictChoice::KeepBoth);
        assert!(c.choices.contains(&ConflictChoice::Replace));
        assert!(c.resumable && c.choices.contains(&ConflictChoice::Resume));
        assert_eq!((c.src_size, c.dst_size), (Some(10), Some(3)));
        assert_eq!((c.src_mtime_ms, c.dst_mtime_ms), (Some(1000), Some(500)));
        assert!(!c.dst_is_dir && !c.src_is_dir);
    }

    #[test]
    fn resume_is_offered_only_for_a_shorter_nonempty_prefix_on_capable_dest() {
        let ask = |dst_size: u64, can: bool| match decide(
            &file(10),
            Some(&dst_entry(Kind::File, dst_size, 0)),
            None,
            can,
        ) {
            Decision::Ask(c) => c.resumable,
            other => panic!("{other:?}"),
        };
        assert!(ask(5, true));
        assert!(!ask(5, false));
        assert!(!ask(0, true));
        assert!(!ask(10, true));
        assert!(!ask(11, true));
    }

    #[test]
    fn file_choices() {
        let d = dst_entry(Kind::File, 10, 500);
        let go = |c| decide(&resolved(file(10), c), Some(&d), None, true);
        assert_eq!(go(ConflictChoice::Replace), proceed(true, false, false));
        assert_eq!(go(ConflictChoice::Skip), Decision::Skip);
        assert_eq!(go(ConflictChoice::KeepBoth), Decision::KeepBoth);
        assert!(matches!(go(ConflictChoice::Merge), Decision::Ask(_)));
        assert!(matches!(go(ConflictChoice::Resume), Decision::Ask(_)));
    }

    #[test]
    fn replace_if_newer_compares_mtimes() {
        let newer = |dst_mtime: i64| {
            decide(
                &resolved(file(1), ConflictChoice::ReplaceIfNewer),
                Some(&dst_entry(Kind::File, 1, dst_mtime)),
                None,
                false,
            )
        };
        assert_eq!(newer(999), proceed(true, false, false));
        assert_eq!(newer(1000), Decision::Skip);
        assert_eq!(newer(2000), Decision::Skip);
        let mut unknown = dst_entry(Kind::File, 1, 0);
        unknown.modified = None;
        let p = resolved(file(1), ConflictChoice::ReplaceIfNewer);
        assert_eq!(decide(&p, Some(&unknown), None, false), Decision::Skip);
    }

    #[test]
    fn resume_in_place() {
        let p = resolved(file(10), ConflictChoice::Resume);
        let d = dst_entry(Kind::File, 4, 0);
        assert_eq!(decide(&p, Some(&d), None, true), proceed(false, true, false));
        assert!(matches!(decide(&p, Some(&d), None, false), Decision::Ask(_)));
    }

    #[test]
    fn folders_merge_and_offer_their_own_choices() {
        let dir = plan_item("d", "d", Kind::Dir, 0);
        let d = dst_entry(Kind::Dir, 0, 0);
        let Decision::Ask(c) = decide(&dir, Some(&d), None, true) else {
            panic!()
        };
        assert_eq!(
            c.choices,
            vec![
                ConflictChoice::Merge,
                ConflictChoice::KeepBoth,
                ConflictChoice::Skip
            ]
        );
        for choice in [
            ConflictChoice::Merge,
            ConflictChoice::Replace,
            ConflictChoice::ReplaceIfNewer,
        ] {
            assert_eq!(
                decide(&resolved(dir.clone(), choice), Some(&d), None, true),
                proceed(false, false, true)
            );
        }
    }

    #[test]
    fn kind_mismatch_offers_only_keep_both_or_skip() {
        let d = dst_entry(Kind::Dir, 0, 0);
        let Decision::Ask(c) = decide(&file(1), Some(&d), None, true) else {
            panic!()
        };
        assert!(c.dst_is_dir && !c.src_is_dir);
        assert_eq!(c.choices, vec![ConflictChoice::KeepBoth, ConflictChoice::Skip]);
        assert!(matches!(
            decide(&resolved(file(1), ConflictChoice::Replace), Some(&d), None, true),
            Decision::Ask(_)
        ));
        let dir = plan_item("d", "d", Kind::Dir, 0);
        let f = dst_entry(Kind::File, 1, 0);
        let Decision::Ask(c) = decide(&dir, Some(&f), None, true) else {
            panic!()
        };
        assert!(c.src_is_dir && !c.dst_is_dir);
    }

    #[test]
    fn default_answer_applies_and_item_resolution_wins() {
        let d = dst_entry(Kind::File, 1, 0);
        assert_eq!(
            decide(&file(1), Some(&d), Some(ConflictChoice::Skip), false),
            Decision::Skip
        );
        let p = resolved(file(1), ConflictChoice::KeepBoth);
        assert_eq!(
            decide(&p, Some(&d), Some(ConflictChoice::Skip), false),
            Decision::KeepBoth
        );
    }

    #[test]
    fn numbering() {
        assert_eq!(numbered_name(b"photo.jpg", 2, false), b"photo 2.jpg");
        assert_eq!(numbered_name(b"a.tar.gz", 3, false), b"a.tar 3.gz");
        assert_eq!(numbered_name(b"README", 2, false), b"README 2");
        assert_eq!(numbered_name(b".bashrc", 2, false), b".bashrc 2");
        assert_eq!(numbered_name(b"v1.2", 2, true), b"v1.2 2");
        assert_eq!(numbered_name(b"x.", 4, false), b"x 4.");
    }
}
