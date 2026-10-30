// SPDX-License-Identifier: LGPL-2.1-or-later
//! Date sections of Activity and Recents. Calendar days in the device's time zone: yesterday starts
//! at last midnight, not 24 hours ago; today counts as one of the 7 and of the 30.

use chrono::{Local, TimeZone};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DateGroup {
    InProgress,
    Today,
    Yesterday,
    LastSevenDays,
    LastThirtyDays,
    Older,
}

impl DateGroup {
    pub fn name(self) -> &'static str {
        match self {
            DateGroup::InProgress => "inProgress",
            DateGroup::Today => "today",
            DateGroup::Yesterday => "yesterday",
            DateGroup::LastSevenDays => "last7Days",
            DateGroup::LastThirtyDays => "last30Days",
            DateGroup::Older => "older",
        }
    }

    /// A day after today (clock moved back) is today.
    pub fn of_days_ago(days_ago: i64) -> DateGroup {
        match days_ago {
            i64::MIN..=0 => DateGroup::Today,
            1 => DateGroup::Yesterday,
            2..=6 => DateGroup::LastSevenDays,
            7..=29 => DateGroup::LastThirtyDays,
            _ => DateGroup::Older,
        }
    }

    /// Each instant gets its own UTC offset, so a daylight saving change in between moves no
    /// midnight.
    pub fn at_in<Tz: TimeZone>(tz: &Tz, at_ms: i64, now_ms: i64) -> DateGroup {
        let day = |ms: i64| tz.timestamp_millis_opt(ms).single().map(|t| t.date_naive());
        let days_ago = match (day(at_ms), day(now_ms)) {
            (Some(at), Some(today)) => today.signed_duration_since(at).num_days(),
            // Beyond chrono's calendar (some 260,000 years out): only which side of now it is on
            // matters.
            _ if at_ms > now_ms => 0,
            _ => i64::MAX,
        };
        DateGroup::of_days_ago(days_ago)
    }

    pub fn at(at_ms: i64, now_ms: i64) -> DateGroup {
        DateGroup::at_in(&Local, at_ms, now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, NaiveDate, Utc};

    const HOUR: i64 = 3_600_000;
    const DAY: i64 = 24 * HOUR;

    fn ms<Tz: TimeZone>(tz: &Tz, (y, m, d): (i32, u32, u32), (h, min): (u32, u32)) -> i64 {
        let wall = NaiveDate::from_ymd_opt(y, m, d)
            .and_then(|date| date.and_hms_opt(h, min, 0))
            .unwrap();
        tz.from_local_datetime(&wall).single().unwrap().timestamp_millis()
    }

    #[test]
    fn days_back_map_to_sections() {
        let expect = [
            (i64::MIN, DateGroup::Today),
            (-1, DateGroup::Today),
            (0, DateGroup::Today),
            (1, DateGroup::Yesterday),
            (2, DateGroup::LastSevenDays),
            (6, DateGroup::LastSevenDays),
            (7, DateGroup::LastThirtyDays),
            (29, DateGroup::LastThirtyDays),
            (30, DateGroup::Older),
            (i64::MAX, DateGroup::Older),
        ];
        for (days, group) in expect {
            assert_eq!(DateGroup::of_days_ago(days), group, "{days} days ago");
        }
    }

    #[test]
    fn names_are_the_roles_values() {
        let all = [
            DateGroup::InProgress,
            DateGroup::Today,
            DateGroup::Yesterday,
            DateGroup::LastSevenDays,
            DateGroup::LastThirtyDays,
            DateGroup::Older,
        ];
        assert_eq!(
            all.map(DateGroup::name),
            [
                "inProgress",
                "today",
                "yesterday",
                "last7Days",
                "last30Days",
                "older"
            ]
        );
        let mut sorted = all;
        sorted.sort();
        assert_eq!(sorted, all, "declared in display order");
    }

    #[test]
    fn local_midnight_starts_a_new_day() {
        let utc = Utc;
        let now = ms(&utc, (2026, 10, 19), (0, 30));
        let before_midnight = ms(&utc, (2026, 10, 18), (23, 59));
        let at_midnight = ms(&utc, (2026, 10, 19), (0, 0));
        assert_eq!(DateGroup::at_in(&utc, now, now), DateGroup::Today);
        assert_eq!(DateGroup::at_in(&utc, at_midnight, now), DateGroup::Today);
        assert_eq!(DateGroup::at_in(&utc, at_midnight - 1, now), DateGroup::Yesterday);
        assert_eq!(
            DateGroup::at_in(&utc, before_midnight, now),
            DateGroup::Yesterday,
            "31 minutes ago, but yesterday"
        );
        let late = ms(&utc, (2026, 10, 19), (23, 59));
        assert_eq!(
            DateGroup::at_in(&utc, at_midnight, late),
            DateGroup::Today,
            "almost 24 hours ago, but today"
        );
    }

    #[test]
    fn calendar_days_back_reach_each_boundary() {
        let utc = Utc;
        let now = ms(&utc, (2026, 10, 19), (9, 0));
        let first = |m: u32, d: u32| ms(&utc, (2026, m, d), (0, 0));
        let last = |m: u32, d: u32| ms(&utc, (2026, m, d), (23, 59));
        let expect = [
            (first(10, 18), DateGroup::Yesterday),
            (last(10, 17), DateGroup::LastSevenDays),
            (first(10, 13), DateGroup::LastSevenDays),
            (now - 6 * DAY, DateGroup::LastSevenDays),
            (last(10, 12), DateGroup::LastThirtyDays),
            (now - 7 * DAY, DateGroup::LastThirtyDays),
            (now - 29 * DAY, DateGroup::LastThirtyDays),
            (first(9, 20), DateGroup::LastThirtyDays),
            (last(9, 19), DateGroup::Older),
            (now - 30 * DAY, DateGroup::Older),
            (ms(&utc, (1999, 1, 1), (12, 0)), DateGroup::Older),
        ];
        for (at, group) in expect {
            assert_eq!(DateGroup::at_in(&utc, at, now), group, "{at}");
        }
    }

    #[test]
    fn later_than_now_is_today() {
        let utc = Utc;
        let now = ms(&utc, (2026, 10, 19), (9, 0));
        assert_eq!(DateGroup::at_in(&utc, now + HOUR, now), DateGroup::Today);
        assert_eq!(DateGroup::at_in(&utc, now + 40 * DAY, now), DateGroup::Today);
        assert_eq!(DateGroup::at_in(&utc, i64::MAX, now), DateGroup::Today);
        assert_eq!(DateGroup::at_in(&utc, i64::MIN, now), DateGroup::Older);
    }

    #[test]
    fn the_utc_offset_moves_midnight() {
        let helsinki = FixedOffset::east_opt(3 * 3600).unwrap();
        let now = ms(&Utc, (2026, 10, 19), (6, 0));
        let at = ms(&Utc, (2026, 10, 18), (23, 30));
        assert_eq!(DateGroup::at_in(&Utc, at, now), DateGroup::Yesterday);
        assert_eq!(DateGroup::at_in(&helsinki, at, now), DateGroup::Today);
        let local_midnight = ms(&helsinki, (2026, 10, 19), (0, 0));
        assert_eq!(DateGroup::at_in(&helsinki, local_midnight, now), DateGroup::Today);
        assert_eq!(
            DateGroup::at_in(&helsinki, local_midnight - 1, now),
            DateGroup::Yesterday
        );
        assert_eq!(
            DateGroup::at_in(&Utc, local_midnight, now),
            DateGroup::Yesterday,
            "21:00 UTC on the 18th"
        );
        let new_york = FixedOffset::west_opt(4 * 3600).unwrap();
        let night = ms(&Utc, (2026, 10, 19), (2, 0));
        let noon = ms(&Utc, (2026, 10, 19), (12, 0));
        assert_eq!(DateGroup::at_in(&Utc, night, noon), DateGroup::Today);
        assert_eq!(DateGroup::at_in(&new_york, night, noon), DateGroup::Yesterday);
    }

    #[test]
    fn the_device_zone_agrees_on_the_obvious() {
        let now = ms(&Utc, (2026, 10, 19), (12, 0));
        assert_eq!(DateGroup::at(now, now), DateGroup::Today);
        assert_eq!(DateGroup::at(now - 45 * DAY, now), DateGroup::Older);
        assert_eq!(DateGroup::at(now - 10 * DAY, now), DateGroup::LastThirtyDays);
    }
}
