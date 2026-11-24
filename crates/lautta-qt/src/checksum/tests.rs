// SPDX-License-Identifier: LGPL-2.1-or-later

use super::*;
use lautta_core::ErrorKind;

#[test]
fn algorithm_groups() {
    let common: Vec<serde_json::Value> = serde_json::from_str(&algorithms_json("common")).unwrap();
    let ids: Vec<_> = common.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["sha256", "md5", "sha1", "sha512"]);
    assert_eq!(common[0]["label"], "SHA-256");
    let more: Vec<serde_json::Value> = serde_json::from_str(&algorithms_json("more")).unwrap();
    assert_eq!(more.len(), 9);
    assert_eq!(more[5]["label"], "BLAKE3");
    assert_eq!(algorithms_json("anything"), algorithms_json("common"));
}

#[test]
fn checksum_list_names() {
    assert!(is_checksum_list("lautta://user-downloads/iso/SHA256SUMS"));
    assert!(is_checksum_list("lautta://user-downloads/a%20b.iso.sha256"));
    assert!(is_checksum_list("x.md5"));
    assert!(!is_checksum_list("lautta://user-downloads/a.iso"));
    assert!(!is_checksum_list("lautta://user-downloads/"));
}

#[test]
fn fractions() {
    assert_eq!(fraction(5, Some(10)), 0.5);
    assert_eq!(fraction(0, Some(0)), 1.0);
    assert_eq!(fraction(12, Some(10)), 1.0);
    assert_eq!(fraction(5, None), -1.0);
}

fn step(r: &mut VerifyRows, event: VerifyEvent) -> Change {
    let change = r.apply(event);
    if let Change::Moved(from, to) = change {
        r.move_row(from, to);
    }
    change
}

fn names(r: &VerifyRows) -> Vec<(&str, &'static str)> {
    r.rows.iter().map(|x| (x.name.as_str(), x.state.name())).collect()
}

#[test]
fn problems_move_to_the_top_in_order_found() {
    let mut r = VerifyRows::default();
    let listed = ["a", "b", "c", "d"]
        .iter()
        .map(|n| n.as_bytes().to_vec())
        .collect();
    assert_eq!(step(&mut r, VerifyEvent::Listed(listed)), Change::Reset);
    assert_eq!(
        names(&r),
        [
            ("a", "waiting"),
            ("b", "waiting"),
            ("c", "waiting"),
            ("d", "waiting")
        ]
    );
    assert_eq!(
        step(
            &mut r,
            VerifyEvent::Progress {
                index: 0,
                done: 1,
                total: Some(4)
            }
        ),
        Change::Updated(0)
    );
    assert_eq!((r.rows[0].state.name(), r.rows[0].progress), ("checking", 0.25));
    assert_eq!(
        step(
            &mut r,
            VerifyEvent::Checked {
                index: 0,
                state: ItemState::Matches
            }
        ),
        Change::Updated(0)
    );
    assert_eq!(
        step(
            &mut r,
            VerifyEvent::Checked {
                index: 2,
                state: ItemState::Differs
            }
        ),
        Change::Moved(2, 0)
    );
    assert_eq!(
        names(&r),
        [
            ("c", "differs"),
            ("a", "matches"),
            ("b", "waiting"),
            ("d", "waiting")
        ]
    );
    assert_eq!(
        step(
            &mut r,
            VerifyEvent::Checked {
                index: 3,
                state: ItemState::Missing
            }
        ),
        Change::Moved(3, 1)
    );
    assert_eq!(
        step(
            &mut r,
            VerifyEvent::Progress {
                index: 1,
                done: 1,
                total: None
            }
        ),
        Change::Updated(3)
    );
    assert_eq!(r.rows[3].progress, -1.0);
    assert_eq!(
        step(
            &mut r,
            VerifyEvent::Checked {
                index: 1,
                state: ItemState::Failed(ErrorKind::PermissionDenied)
            }
        ),
        Change::Moved(3, 2)
    );
    assert_eq!(
        names(&r),
        [
            ("c", "differs"),
            ("d", "missing"),
            ("b", "failed"),
            ("a", "matches")
        ]
    );
    assert_eq!(
        step(
            &mut r,
            VerifyEvent::Checked {
                index: 9,
                state: ItemState::Matches
            }
        ),
        Change::None
    );
    assert_eq!(
        step(
            &mut r,
            VerifyEvent::Progress {
                index: 9,
                done: 1,
                total: None
            }
        ),
        Change::None
    );
}

#[test]
fn a_problem_already_in_place_is_only_updated() {
    let mut r = VerifyRows::default();
    step(&mut r, VerifyEvent::Listed(vec![b"a".to_vec(), b"b".to_vec()]));
    assert_eq!(
        step(
            &mut r,
            VerifyEvent::Checked {
                index: 0,
                state: ItemState::NotChecked
            }
        ),
        Change::Updated(0)
    );
    assert_eq!(
        step(
            &mut r,
            VerifyEvent::Checked {
                index: 1,
                state: ItemState::Missing
            }
        ),
        Change::Updated(1)
    );
    assert_eq!(names(&r), [("a", "notChecked"), ("b", "missing")]);
    step(&mut r, VerifyEvent::Listed(vec![b"x".to_vec(), b"y".to_vec()]));
    assert_eq!(
        step(
            &mut r,
            VerifyEvent::Checked {
                index: 1,
                state: ItemState::Differs
            }
        ),
        Change::Moved(1, 0)
    );
}

#[test]
fn closing_the_page_stops_the_read() {
    let cancel = Cancel::default();
    drop(Running(cancel.clone()));
    assert!(cancel.is_canceled());
}
