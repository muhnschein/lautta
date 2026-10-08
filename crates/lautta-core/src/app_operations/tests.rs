// SPDX-License-Identifier: LGPL-2.1-or-later
use super::*;
use crate::ops::{PlanItem, PlanTotals};

fn uri(s: &str) -> Uri {
    Uri::parse(s).unwrap()
}

fn item(src: &str, dst: &str, kind: Kind, proposed: Option<&str>) -> PlanItem {
    PlanItem {
        src: uri(src),
        dst: uri(dst),
        kind,
        size: Some(1),
        mtime_ms: None,
        mode: None,
        link_target: None,
        conflict: None,
        proposed_name: proposed.map(|p| p.as_bytes().to_vec()),
        resolution: None,
    }
}

fn plan(items: Vec<PlanItem>) -> Plan {
    Plan {
        kind: OperationKind::Copy,
        destination: uri("lautta://b/"),
        totals: PlanTotals {
            renamed: items.iter().filter(|i| i.proposed_name.is_some()).count() as u64,
            ..PlanTotals::default()
        },
        items,
        needs_summary: false,
    }
}

#[test]
fn pending_plans_are_kept_by_id() {
    let pending = PendingPlans::new();
    assert!(pending.is_empty());
    let a = pending.insert(plan(vec![]));
    let b = pending.insert(plan(vec![item("lautta://a/x", "lautta://b/x", Kind::File, None)]));
    assert_eq!((a, b), (1, 2));
    assert_eq!(pending.len(), 2);
    assert_eq!(pending.with(b, |p| p.items.len()), Some(1));
    let taken = pending.take(b).unwrap();
    assert!(pending.with(b, |_| ()).is_none(), "taken plans are gone");
    pending.put_back(b, taken);
    assert_eq!(pending.with(b, |p| p.items.len()), Some(1));
    assert!(pending.discard(a));
    assert!(!pending.discard(a), "discarding twice finds nothing");
    assert!(pending.take(99).is_none());
}

#[test]
fn start_options_merge_from_json() {
    let base = StartOptions {
        verify_checksums: false,
        preserve_mtime: true,
        preserve_mode: false,
        suggested_names: true,
    };
    let merged =
        base.merged_with_json(r#"{"verifyChecksums":true,"preserveMtime":false,"suggestedNames":false}"#);
    assert!(merged.verify_checksums && !merged.preserve_mtime && !merged.suggested_names);
    assert!(!merged.preserve_mode, "missing keys keep the old value");
    assert_eq!(base.merged_with_json("nonsense"), base);
    assert_eq!(base.merged_with_json("[1]"), base);
    let s = Settings {
        verify_checksums: true,
        ..Settings::default()
    };
    assert!(StartOptions::from_settings(&s).verify_checksums);
}

#[test]
fn reverting_names_moves_the_whole_subtree() {
    let mut p = plan(vec![
        item("lautta://a/aux", "lautta://b/aux_", Kind::Dir, Some("aux_")),
        item("lautta://a/aux/f.txt", "lautta://b/aux_/f.txt", Kind::File, None),
        item("lautta://a/ok.txt", "lautta://b/ok.txt", Kind::File, None),
        item(
            "lautta://a/n: x.md",
            "lautta://b/n- x.md",
            Kind::File,
            Some("n- x.md"),
        ),
    ]);
    revert_suggested_names(&mut p);
    let dsts: Vec<String> = p.items.iter().map(|i| i.dst.to_string()).collect();
    assert_eq!(
        dsts,
        [
            "lautta://b/aux",
            "lautta://b/aux/f.txt",
            "lautta://b/ok.txt",
            "lautta://b/n:%20x.md"
        ]
    );
    assert!(p.items.iter().all(|i| i.proposed_name.is_none()));
    assert_eq!(p.totals.renamed, 0);
}

#[test]
fn rules_parse_every_kind() {
    let set = parse_rules(
        r#"{"includeExtension":true,"rules":[
        {"type":"findReplace","find":"a","replace":"b","regex":true,"caseSensitive":false},
        {"type":"prefix","text":"p-"},{"type":"suffix","text":"-s"},
        {"type":"numbering","start":5,"step":2,"padding":3,"position":"prefix","separator":"_"},
        {"type":"case","mode":"title"},{"type":"extension","mode":"change","value":"txt"},
        {"type":"extension","mode":"remove"},{"type":"extension","mode":"lowercase"},
        {"type":"date","format":"YYYY","position":"suffix","separator":"-","utcOffsetSecs":3600}]}"#,
    )
    .unwrap();
    assert!(set.include_extension);
    assert_eq!(set.rules.len(), 9);
    assert_eq!(
        set.rules[0],
        Rule::FindReplace {
            find: "a".into(),
            replace: "b".into(),
            regex: true,
            case_sensitive: false
        }
    );
    assert_eq!(
        set.rules[3],
        Rule::Numbering(Numbering {
            start: 5,
            step: 2,
            padding: 3,
            position: Position::Prefix,
            separator: "_".into()
        })
    );
    assert_eq!(set.rules[4], Rule::Case(CaseMode::Title));
    assert_eq!(set.rules[5], Rule::Extension(ExtensionMode::Change("txt".into())));
    assert!(
        matches!(&set.rules[8], Rule::Date(d) if d.utc_offset_secs == 3600 && d.position == Position::Suffix)
    );
}

#[test]
fn bad_rules_are_invalid_arguments() {
    for text in [
        "not json",
        "[]",
        r#"{"rules":[1]}"#,
        r#"{"rules":[{"type":"nope"}]}"#,
        r#"{"rules":[{"type":"case","mode":"x"}]}"#,
        r#"{"rules":[{"type":"extension","mode":"x"}]}"#,
    ] {
        assert_eq!(
            parse_rules(text).unwrap_err().kind,
            ErrorKind::InvalidArgument,
            "{text}"
        );
    }
    assert_eq!(parse_rules("{}").unwrap(), RuleSet::default());
}

#[test]
fn numbering_defaults_and_clamps() {
    let set = parse_rules(r#"{"rules":[{"type":"numbering","padding":99}]}"#).unwrap();
    assert_eq!(
        set.rules[0],
        Rule::Numbering(Numbering {
            padding: 12,
            ..Numbering::default()
        })
    );
}

#[test]
fn mode_as_text() {
    assert_eq!(mode_text(0o644), "rw-r--r--");
    assert_eq!(mode_text(0o755), "rwxr-xr-x");
    assert_eq!(mode_text(0), "---------");
}

#[test]
fn days_left_rounds_up_and_stops_at_zero() {
    let day = SECS_PER_DAY;
    assert_eq!(days_left(0, 0, 30), 30);
    assert_eq!(days_left(0, day, 30), 29);
    assert_eq!(days_left(0, day + 1, 30), 29, "a started day still counts");
    assert_eq!(days_left(0, 30 * day, 30), 0);
    assert_eq!(days_left(0, 99 * day, 30), 0);
}

#[test]
fn archive_names() {
    assert_eq!(archive_file_name("Thesis", ArchiveKind::Zip), "Thesis.zip");
    assert_eq!(archive_file_name("Thesis.ZIP", ArchiveKind::Zip), "Thesis.ZIP");
    assert_eq!(archive_file_name("a", ArchiveKind::TarGz), "a.tar.gz");
    assert_eq!(archive_folder_name("dataset.tar.gz"), "dataset");
    assert_eq!(archive_folder_name("photos.zip"), "photos");
    assert_eq!(archive_folder_name("noext"), "noext");
    assert_eq!(archive_folder_name(".zip"), ".zip");
    assert_eq!(ArchiveKind::parse("zip"), Some(ArchiveKind::Zip));
    assert_eq!(ArchiveKind::parse("tar.gz"), Some(ArchiveKind::TarGz));
    assert_eq!(ArchiveKind::parse("rar"), None);
}

#[test]
fn conflict_choice_names() {
    for c in [
        ConflictChoice::Replace,
        ConflictChoice::Skip,
        ConflictChoice::KeepBoth,
        ConflictChoice::Merge,
        ConflictChoice::ReplaceIfNewer,
        ConflictChoice::Resume,
    ] {
        assert_eq!(parse_choice(&format!("{c:?}")), Some(c));
    }
    assert_eq!(parse_choice("keepboth"), None);
}

#[test]
fn names_of_kinds_status_and_filesystems() {
    assert_eq!(kind_name(OperationKind::Copy), "copy");
    assert_eq!(kind_name(OperationKind::Move), "move");
    assert_eq!(status_name(bulkrename::RenameStatus::Collision), "collision");
    assert_eq!(status_name(bulkrename::RenameStatus::Ok), "ok");
    assert_eq!(fs_type_name(0xEF53), Some("ext4"));
    assert_eq!(fs_type_name(0x2011_BAB0), Some("exFAT"));
    assert_eq!(fs_type_name(1), None);
}
