// SPDX-License-Identifier: LGPL-2.1-or-later
//! Bulk rename (OPS-11): a pipeline of rules applied in order to every name,
//! with a preview that checks collisions and validity.
//!
//! Rules work on the stem (the name without its extension) unless
//! [`RuleSet::include_extension`] is set. Names that are not valid UTF-8 are
//! reported as [`RenameStatus::Invalid`]: rewriting them lossily would destroy
//! the original bytes. Executing the renames is the caller's job; since a
//! preview may swap names (`a`→`b`, `b`→`a`), the caller must rename in two
//! phases through temporary names.

use super::names::{is_valid_name, split_extension, NameRules};
use crate::error::{Error, ErrorKind, Result};
use regex::{NoExpand, Regex, RegexBuilder};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseMode {
    Lower,
    Upper,
    /// First letter of every word upper case.
    Title,
    /// Only the first letter upper case.
    Sentence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionMode {
    /// New extension, with or without the leading dot; empty removes it.
    Change(String),
    Remove,
    Lowercase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Prefix,
    Suffix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Numbering {
    pub start: i64,
    pub step: i64,
    /// Minimum digits, zero padded.
    pub padding: usize,
    pub position: Position,
    pub separator: String,
}

impl Default for Numbering {
    fn default() -> Self {
        Numbering {
            start: 1,
            step: 1,
            padding: 0,
            position: Position::Suffix,
            separator: " ".to_owned(),
        }
    }
}

/// Inserts the modification time formatted with `format`, which understands
/// `YYYY YY MM DD HH mm ss`; other characters are copied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateRule {
    pub format: String,
    pub position: Position,
    pub separator: String,
    /// Offset of the local time zone from UTC, in seconds.
    pub utc_offset_secs: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    FindReplace {
        find: String,
        replace: String,
        /// `find` is a regular expression and `replace` may use `$1` groups.
        regex: bool,
        case_sensitive: bool,
    },
    Prefix(String),
    Suffix(String),
    Numbering(Numbering),
    Case(CaseMode),
    Extension(ExtensionMode),
    Date(DateRule),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
    /// Apply the rules to the whole name instead of the stem.
    pub include_extension: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameStatus {
    Ok,
    Unchanged,
    Collision,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePreview {
    pub old: Vec<u8>,
    pub new: Vec<u8>,
    pub status: RenameStatus,
}

/// One entry to rename: its name and modification time (ms since the epoch).
pub type RenameEntry = (Vec<u8>, Option<i64>);

// ------------------------------------------------------------- compile

enum Step {
    Plain { re: Regex, replace: String },
    Pattern { re: Regex, replace: String },
    Nothing,
    Prefix(String),
    Suffix(String),
    Numbering(Numbering),
    Case(CaseMode),
    Extension(ExtensionMode),
    Date(DateRule),
}

fn compile_regex(pattern: &str, case_sensitive: bool) -> Result<Regex> {
    RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| Error::new(ErrorKind::InvalidArgument, format!("invalid pattern: {e}")))
}

fn compile(rule: &Rule) -> Result<Step> {
    Ok(match rule {
        Rule::FindReplace {
            find,
            replace,
            regex,
            case_sensitive,
        } => {
            if *regex {
                Step::Pattern {
                    re: compile_regex(find, *case_sensitive)?,
                    replace: replace.clone(),
                }
            } else if find.is_empty() {
                Step::Nothing
            } else {
                Step::Plain {
                    re: compile_regex(&regex::escape(find), *case_sensitive)?,
                    replace: replace.clone(),
                }
            }
        }
        Rule::Prefix(p) => Step::Prefix(p.clone()),
        Rule::Suffix(s) => Step::Suffix(s.clone()),
        Rule::Numbering(n) => Step::Numbering(n.clone()),
        Rule::Case(c) => Step::Case(*c),
        Rule::Extension(e) => Step::Extension(e.clone()),
        Rule::Date(d) => Step::Date(d.clone()),
    })
}

// --------------------------------------------------------------- apply

/// A name being rewritten; `ext` keeps its dot and is empty when the rules
/// work on the whole name.
struct Parts {
    stem: String,
    ext: String,
}

impl Parts {
    fn split(name: &str, include_ext: bool) -> Parts {
        if include_ext {
            return Parts {
                stem: name.to_owned(),
                ext: String::new(),
            };
        }
        let (stem, ext) = split_extension(name.as_bytes(), false);
        // The split is at a `.`, an ASCII byte, so both halves stay valid UTF-8.
        Parts {
            stem: String::from_utf8_lossy(stem).into_owned(),
            ext: String::from_utf8_lossy(ext).into_owned(),
        }
    }

    fn join(&self) -> String {
        format!("{}{}", self.stem, self.ext)
    }
}

struct EntryCtx {
    index: usize,
    mtime_ms: Option<i64>,
    include_ext: bool,
}

fn add_at(stem: &mut String, text: &str, separator: &str, position: Position) {
    *stem = match position {
        Position::Prefix => format!("{text}{separator}{stem}"),
        Position::Suffix => format!("{stem}{separator}{text}"),
    };
}

/// `start + index * step`, saturating, zero padded to `padding` digits.
pub fn format_number(start: i64, step: i64, index: usize, padding: usize) -> String {
    let n = i128::from(start) + i128::from(step) * i128::try_from(index).unwrap_or(0);
    let n = n.clamp(i128::from(i64::MIN), i128::from(i64::MAX));
    let sign = if n < 0 { "-" } else { "" };
    format!("{sign}{:0padding$}", n.unsigned_abs())
}

impl Step {
    /// `None` when the step cannot be applied to this entry (no mtime).
    fn apply(&self, parts: &mut Parts, ctx: &EntryCtx) -> Option<()> {
        match self {
            Step::Plain { re, replace } => {
                parts.stem = re.replace_all(&parts.stem, NoExpand(replace)).into_owned();
            }
            Step::Pattern { re, replace } => {
                parts.stem = re.replace_all(&parts.stem, replace.as_str()).into_owned();
            }
            Step::Nothing => {}
            Step::Prefix(p) => parts.stem.insert_str(0, p),
            Step::Suffix(s) => parts.stem.push_str(s),
            Step::Numbering(n) => {
                let text = format_number(n.start, n.step, ctx.index, n.padding);
                add_at(&mut parts.stem, &text, &n.separator, n.position);
            }
            Step::Case(mode) => parts.stem = change_case(&parts.stem, *mode),
            Step::Extension(mode) => apply_extension(parts, mode, ctx.include_ext),
            Step::Date(d) => {
                let text = format_date(ctx.mtime_ms?, &d.format, d.utc_offset_secs);
                add_at(&mut parts.stem, &text, &d.separator, d.position);
            }
        }
        Some(())
    }
}

fn apply_extension(parts: &mut Parts, mode: &ExtensionMode, include_ext: bool) {
    if include_ext {
        let full = parts.join();
        *parts = Parts::split(&full, false);
    }
    parts.ext = match mode {
        ExtensionMode::Remove => String::new(),
        ExtensionMode::Lowercase => parts.ext.to_lowercase(),
        ExtensionMode::Change(new) => {
            let bare = new.trim_start_matches('.');
            if bare.is_empty() {
                String::new()
            } else {
                format!(".{bare}")
            }
        }
    };
    if include_ext {
        parts.stem = parts.join();
        parts.ext = String::new();
    }
}

fn is_word_separator(c: char) -> bool {
    !c.is_alphanumeric() && c != '\''
}

fn change_case(text: &str, mode: CaseMode) -> String {
    match mode {
        CaseMode::Lower => text.to_lowercase(),
        CaseMode::Upper => text.to_uppercase(),
        CaseMode::Title => capitalise(text, true),
        CaseMode::Sentence => capitalise(text, false),
    }
}

/// Lower-cases everything and upper-cases the first letter of each word
/// (`every_word`) or of the text only.
fn capitalise(text: &str, every_word: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_start = true;
    for c in text.chars() {
        if at_start && c.is_alphabetic() {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        at_start = every_word && is_word_separator(c);
    }
    out
}

// ---------------------------------------------------------------- date

/// Civil date and time of `ms` shifted by `offset_secs` (proleptic Gregorian).
fn civil(ms: i64, offset_secs: i32) -> (i64, u32, u32, u32, u32, u32) {
    let secs = ms.div_euclid(1000).saturating_add(i64::from(offset_secs));
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Days since 1970-01-01 to y-m-d (Howard Hinnant's civil_from_days).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = yoe + era * 400 + i64::from(month <= 2);
    let hour = u32::try_from(rem / 3600).unwrap_or(0);
    let minute = u32::try_from(rem % 3600 / 60).unwrap_or(0);
    let second = u32::try_from(rem % 60).unwrap_or(0);
    (year, month, day, hour, minute, second)
}

/// Formats a time with the tokens `YYYY YY MM DD HH mm ss`.
pub fn format_date(ms: i64, pattern: &str, utc_offset_secs: i32) -> String {
    let (y, mo, d, h, mi, s) = civil(ms, utc_offset_secs);
    let mut out = String::new();
    let mut rest = pattern;
    while !rest.is_empty() {
        let (text, used) = match date_token(rest) {
            Some(("YYYY", n)) => (format!("{y:04}"), n),
            Some(("YY", n)) => (format!("{:02}", y.rem_euclid(100)), n),
            Some(("MM", n)) => (format!("{mo:02}"), n),
            Some(("DD", n)) => (format!("{d:02}"), n),
            Some(("HH", n)) => (format!("{h:02}"), n),
            Some(("mm", n)) => (format!("{mi:02}"), n),
            Some(("ss", n)) => (format!("{s:02}"), n),
            _ => {
                let c = rest.chars().next().map_or(1, char::len_utf8);
                (rest[..c].to_owned(), c)
            }
        };
        out.push_str(&text);
        rest = &rest[used..];
    }
    out
}

fn date_token(s: &str) -> Option<(&'static str, usize)> {
    ["YYYY", "YY", "MM", "DD", "HH", "mm", "ss"]
        .into_iter()
        .find(|t| s.starts_with(t))
        .map(|t| (t, t.len()))
}

// ------------------------------------------------------------- preview

/// The new name for every entry (index = position in the selection), before
/// validity and collision checks. `None` for names that cannot be processed.
pub fn apply_rules(entries: &[RenameEntry], rules: &RuleSet) -> Result<Vec<Option<String>>> {
    let steps = rules.rules.iter().map(compile).collect::<Result<Vec<_>>>()?;
    Ok(entries
        .iter()
        .enumerate()
        .map(|(index, (name, mtime_ms))| {
            let text = std::str::from_utf8(name).ok()?;
            let ctx = EntryCtx {
                index,
                mtime_ms: *mtime_ms,
                include_ext: rules.include_extension,
            };
            let mut parts = Parts::split(text, rules.include_extension);
            for step in &steps {
                step.apply(&mut parts, &ctx)?;
            }
            Some(parts.join())
        })
        .collect())
}

/// Preview with POSIX-like name rules.
pub fn preview(
    entries: &[RenameEntry],
    rules: &RuleSet,
    existing_names: &[Vec<u8>],
) -> Result<Vec<RenamePreview>> {
    preview_with(entries, rules, existing_names, NameRules::permissive())
}

/// Previews the renames. `existing_names` are all names in the folder,
/// including the selected ones; a new name that equals an unselected name, an
/// unchanged selected name or another new name is a collision. Regex errors
/// are `InvalidArgument`.
pub fn preview_with(
    entries: &[RenameEntry],
    rules: &RuleSet,
    existing_names: &[Vec<u8>],
    name_rules: NameRules,
) -> Result<Vec<RenamePreview>> {
    let computed = apply_rules(entries, rules)?;
    let ci = name_rules.case_insensitive;
    let mut drafts: Vec<RenamePreview> = entries
        .iter()
        .zip(&computed)
        .map(|((old, _), new)| draft(old, new.as_deref(), name_rules))
        .collect();
    let selected: HashSet<Vec<u8>> = entries.iter().map(|(n, _)| fold(n, ci)).collect();
    let outside: HashSet<Vec<u8>> = existing_names
        .iter()
        .map(|n| fold(n, ci))
        .filter(|n| !selected.contains(n))
        .collect();
    let mut finals: HashMap<Vec<u8>, usize> = HashMap::new();
    for d in &drafts {
        *finals.entry(fold(final_name(d), ci)).or_insert(0) += 1;
    }
    for d in &mut drafts {
        if d.status != RenameStatus::Ok {
            continue;
        }
        let key = fold(&d.new, ci);
        if finals.get(&key).copied().unwrap_or(0) > 1 || outside.contains(&key) {
            d.status = RenameStatus::Collision;
        }
    }
    Ok(drafts)
}

fn draft(old: &[u8], new: Option<&str>, name_rules: NameRules) -> RenamePreview {
    let (new_bytes, status) = match new {
        None => (old.to_vec(), RenameStatus::Invalid),
        Some(n) if n.as_bytes() == old => (old.to_vec(), RenameStatus::Unchanged),
        Some(n) if !is_valid_name(n.as_bytes(), name_rules) => (n.as_bytes().to_vec(), RenameStatus::Invalid),
        Some(n) => (n.as_bytes().to_vec(), RenameStatus::Ok),
    };
    RenamePreview {
        old: old.to_vec(),
        new: new_bytes,
        status,
    }
}

/// The name an entry will have once the batch ran: renamed only when `Ok`.
fn final_name(d: &RenamePreview) -> &[u8] {
    if d.status == RenameStatus::Ok {
        &d.new
    } else {
        &d.old
    }
}

fn fold(name: &[u8], case_insensitive: bool) -> Vec<u8> {
    if case_insensitive {
        String::from_utf8_lossy(name).to_lowercase().into_bytes()
    } else {
        name.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str) -> RenameEntry {
        (name.as_bytes().to_vec(), Some(0))
    }

    fn run(names: &[&str], rules: Vec<Rule>) -> Vec<String> {
        let entries: Vec<RenameEntry> = names.iter().map(|n| e(n)).collect();
        let set = RuleSet {
            rules,
            include_extension: false,
        };
        apply_rules(&entries, &set)
            .unwrap()
            .into_iter()
            .map(|n| n.unwrap())
            .collect()
    }

    fn fr(find: &str, replace: &str, regex: bool, case_sensitive: bool) -> Rule {
        Rule::FindReplace {
            find: find.into(),
            replace: replace.into(),
            regex,
            case_sensitive,
        }
    }

    #[test]
    fn plain_find_replace_hits_stem_only() {
        let out = run(&["a.a.txt", "banana.a"], vec![fr("a", "o", false, true)]);
        assert_eq!(out, vec!["o.o.txt", "bonono.a"]);
    }

    #[test]
    fn plain_replace_is_literal_and_case_option_works() {
        assert_eq!(run(&["a.b"], vec![fr("a", "$1", false, true)]), vec!["$1.b"]);
        assert_eq!(run(&["a.b"], vec![fr("a.b", "x", false, true)]), vec!["a.b"]);
        assert_eq!(run(&["Aa.b"], vec![fr("a", "x", false, true)]), vec!["Ax.b"]);
        assert_eq!(run(&["Aa.b"], vec![fr("a", "x", false, false)]), vec!["xx.b"]);
        assert_eq!(run(&["a.b"], vec![fr("", "x", false, true)]), vec!["a.b"]);
    }

    #[test]
    fn regex_with_groups() {
        let r = fr(r"(\d+)-(\w+)", "${2}_${1}", true, true);
        assert_eq!(run(&["12-ab.txt"], vec![r]), vec!["ab_12.txt"]);
        let r = fr("IMG", "pic", true, false);
        assert_eq!(run(&["img_1.jpg"], vec![r]), vec!["pic_1.jpg"]);
        let r = fr("IMG", "pic", true, true);
        assert_eq!(run(&["img_1.jpg"], vec![r]), vec!["img_1.jpg"]);
    }

    #[test]
    fn bad_regex_is_invalid_argument() {
        let entries = vec![e("a")];
        let set = RuleSet {
            rules: vec![fr("(", "", true, true)],
            include_extension: false,
        };
        let err = preview(&entries, &set, &[]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidArgument);
        assert!(err.message.contains("invalid pattern"));
    }

    #[test]
    fn prefix_and_suffix() {
        let out = run(
            &["a.txt"],
            vec![Rule::Prefix("x_".into()), Rule::Suffix("_y".into())],
        );
        assert_eq!(out, vec!["x_a_y.txt"]);
        let set = RuleSet {
            rules: vec![Rule::Suffix("_y".into())],
            include_extension: true,
        };
        let out = apply_rules(&[e("a.txt")], &set).unwrap();
        assert_eq!(out, vec![Some("a.txt_y".to_owned())]);
    }

    #[test]
    fn numbering() {
        let n = Numbering {
            start: 5,
            step: 10,
            padding: 3,
            position: Position::Suffix,
            separator: "_".into(),
        };
        let out = run(&["a.x", "b.x", "c.x"], vec![Rule::Numbering(n.clone())]);
        assert_eq!(out, vec!["a_005.x", "b_015.x", "c_025.x"]);
        let p = Numbering {
            position: Position::Prefix,
            separator: "-".into(),
            ..n
        };
        assert_eq!(
            run(&["a.x", "b.x"], vec![Rule::Numbering(p)]),
            vec!["005-a.x", "015-b.x"]
        );
        let d = Numbering::default();
        assert_eq!(run(&["a", "b"], vec![Rule::Numbering(d)]), vec!["a 1", "b 2"]);
    }

    #[test]
    fn number_formatting_edges() {
        assert_eq!(format_number(1, 1, 0, 0), "1");
        assert_eq!(format_number(1, 1, 9, 2), "10");
        assert_eq!(format_number(1, 1, 99, 2), "100");
        assert_eq!(format_number(0, -1, 2, 3), "-002");
        assert_eq!(format_number(0, -1, 0, 3), "000");
        assert_eq!(format_number(i64::MAX, 1, 5, 0), i64::MAX.to_string());
        assert_eq!(format_number(i64::MIN, -1, 5, 0), i64::MIN.to_string());
        assert_eq!(format_number(7, 0, 100, 0), "7");
    }

    #[test]
    fn case_modes() {
        let c = |m| run(&["hELLO wORLD-it's_ok.TXT"], vec![Rule::Case(m)]);
        assert_eq!(c(CaseMode::Lower), vec!["hello world-it's_ok.TXT"]);
        assert_eq!(c(CaseMode::Upper), vec!["HELLO WORLD-IT'S_OK.TXT"]);
        assert_eq!(c(CaseMode::Title), vec!["Hello World-It's_Ok.TXT"]);
        assert_eq!(c(CaseMode::Sentence), vec!["Hello world-it's_ok.TXT"]);
        let s = run(&["  12 abc"], vec![Rule::Case(CaseMode::Sentence)]);
        assert_eq!(s, vec!["  12 abc"]);
        let t = run(&["12abc def"], vec![Rule::Case(CaseMode::Title)]);
        assert_eq!(t, vec!["12abc Def"]);
        let s = run(&["_éa"], vec![Rule::Case(CaseMode::Sentence)]);
        assert_eq!(s, vec!["_éa"]);
        let t = run(&["_éa"], vec![Rule::Case(CaseMode::Title)]);
        assert_eq!(t, vec!["_Éa"]);
    }

    #[test]
    fn extension_rules() {
        let ext = |m| run(&["a.JPG", "b", "c.tar.gz", ".rc"], vec![Rule::Extension(m)]);
        assert_eq!(
            ext(ExtensionMode::Lowercase),
            vec!["a.jpg", "b", "c.tar.gz", ".rc"]
        );
        assert_eq!(ext(ExtensionMode::Remove), vec!["a", "b", "c", ".rc"]);
        assert_eq!(
            ext(ExtensionMode::Change("png".into())),
            vec!["a.png", "b.png", "c.png", ".rc.png"]
        );
        assert_eq!(
            ext(ExtensionMode::Change(".png".into())),
            vec!["a.png", "b.png", "c.png", ".rc.png"]
        );
        assert_eq!(
            ext(ExtensionMode::Change(String::new())),
            vec!["a", "b", "c", ".rc"]
        );
        assert_eq!(
            ext(ExtensionMode::Change("..".into())),
            vec!["a", "b", "c", ".rc"]
        );
    }

    #[test]
    fn include_extension_applies_to_whole_name() {
        let set = RuleSet {
            rules: vec![fr(".txt", ".md", false, true), Rule::Case(CaseMode::Upper)],
            include_extension: true,
        };
        let out = apply_rules(&[e("a.txt")], &set).unwrap();
        assert_eq!(out, vec![Some("A.MD".to_owned())]);
        let stem_only = RuleSet {
            rules: vec![fr(".txt", ".md", false, true)],
            include_extension: false,
        };
        assert_eq!(
            apply_rules(&[e("a.txt")], &stem_only).unwrap(),
            vec![Some("a.txt".to_owned())]
        );
    }

    #[test]
    fn extension_rule_with_include_extension() {
        let set = RuleSet {
            rules: vec![
                Rule::Prefix("x".into()),
                Rule::Extension(ExtensionMode::Change("md".into())),
            ],
            include_extension: true,
        };
        assert_eq!(
            apply_rules(&[e("a.txt")], &set).unwrap(),
            vec![Some("xa.md".to_owned())]
        );
        let set = RuleSet {
            rules: vec![Rule::Extension(ExtensionMode::Remove), Rule::Suffix("!".into())],
            include_extension: true,
        };
        assert_eq!(
            apply_rules(&[e("a.txt")], &set).unwrap(),
            vec![Some("a!".to_owned())]
        );
    }

    #[test]
    fn dates_format() {
        // 2024-02-29 13:05:09 UTC
        let ms = 1_709_211_909_000;
        assert_eq!(format_date(ms, "YYYY-MM-DD", 0), "2024-02-29");
        assert_eq!(format_date(ms, "YYYYMMDD_HHmmss", 0), "20240229_130509");
        assert_eq!(format_date(ms, "YY/MM", 0), "24/02");
        assert_eq!(format_date(ms, "YYYY-MM-DD", 3600 * 11), "2024-03-01");
        assert_eq!(format_date(ms, "HH:mm", -3600 * 14), "23:05");
        assert_eq!(format_date(ms, "Day é DD", 0), "Day é 29");
        assert_eq!(format_date(0, "YYYY-MM-DD HH:mm:ss", 0), "1970-01-01 00:00:00");
        assert_eq!(
            format_date(-1000, "YYYY-MM-DD HH:mm:ss", 0),
            "1969-12-31 23:59:59"
        );
        assert_eq!(format_date(951_782_400_000, "YYYY-MM-DD", 0), "2000-02-29");
        assert_eq!(format_date(4_102_444_800_000, "YYYY-MM-DD", 0), "2100-01-01");
        assert_eq!(format_date(0, "", 0), "");
    }

    #[test]
    fn date_rule_inserts_mtime() {
        let rule = Rule::Date(DateRule {
            format: "YYYY-MM-DD".into(),
            position: Position::Prefix,
            separator: "_".into(),
            utc_offset_secs: 0,
        });
        let set = RuleSet {
            rules: vec![rule.clone()],
            include_extension: false,
        };
        let out = apply_rules(&[("a.jpg".into(), Some(1_709_211_909_000))], &set).unwrap();
        assert_eq!(out, vec![Some("2024-02-29_a.jpg".to_owned())]);
        let out = apply_rules(&[("a.jpg".into(), None)], &set).unwrap();
        assert_eq!(out, vec![None]);
        let suffix = Rule::Date(DateRule {
            format: "YYYY".into(),
            position: Position::Suffix,
            separator: "-".into(),
            utc_offset_secs: 0,
        });
        let set = RuleSet {
            rules: vec![suffix],
            include_extension: false,
        };
        let out = apply_rules(&[("a.jpg".into(), Some(0))], &set).unwrap();
        assert_eq!(out, vec![Some("a-1970.jpg".to_owned())]);
    }

    fn names(v: &[&str]) -> Vec<Vec<u8>> {
        v.iter().map(|s| s.as_bytes().to_vec()).collect()
    }

    fn statuses(p: &[RenamePreview]) -> Vec<RenameStatus> {
        p.iter().map(|x| x.status).collect()
    }

    use RenameStatus::{Collision, Invalid, Ok as Fine, Unchanged};

    #[test]
    fn preview_statuses() {
        let entries = vec![e("a.txt"), e("keep.txt"), e("x")];
        let set = RuleSet {
            rules: vec![fr("a", "b", false, true), fr("^x$", "", true, true)],
            include_extension: false,
        };
        let p = preview(&entries, &set, &names(&["a.txt", "keep.txt"])).unwrap();
        assert_eq!(statuses(&p), vec![Fine, Unchanged, Invalid]);
        assert_eq!(p[0].old, b"a.txt");
        assert_eq!(p[0].new, b"b.txt");
        assert_eq!(p[1].new, b"keep.txt");
    }

    #[test]
    fn collisions_between_new_names_and_with_existing() {
        let entries = vec![e("a1.txt"), e("a2.txt"), e("b.txt"), e("c.txt")];
        let set = RuleSet {
            rules: vec![fr(r"^a\d", "z", true, true), fr("^c", "d", true, true)],
            include_extension: false,
        };
        let existing = names(&["a1.txt", "a2.txt", "b.txt", "c.txt", "d.txt", "other"]);
        let p = preview(&entries, &set, &existing).unwrap();
        assert_eq!(statuses(&p), vec![Collision, Collision, Unchanged, Collision]);
    }

    #[test]
    fn rename_onto_an_unchanged_selected_name_collides() {
        let entries = vec![e("a"), e("b")];
        let set = RuleSet {
            rules: vec![fr("^a$", "b", true, true)],
            include_extension: false,
        };
        let p = preview(&entries, &set, &names(&["a", "b"])).unwrap();
        assert_eq!(statuses(&p), vec![Collision, Unchanged]);
    }

    #[test]
    fn swapping_names_is_not_a_collision() {
        let entries = vec![e("a"), e("b")];
        let set = RuleSet {
            rules: vec![
                fr("^a$", "x", true, true),
                fr("^b$", "a", true, true),
                fr("^x$", "b", true, true),
            ],
            include_extension: false,
        };
        let p = preview(&entries, &set, &names(&["a", "b"])).unwrap();
        assert_eq!(statuses(&p), vec![Fine, Fine]);
    }

    #[test]
    fn invalid_names_do_not_block_others() {
        let entries = vec![e("a"), e("b")];
        let set = RuleSet {
            rules: vec![fr("^a$", "x/y", true, true), fr("^b$", "a", true, true)],
            include_extension: false,
        };
        let p = preview(&entries, &set, &names(&["a", "b"])).unwrap();
        assert_eq!(statuses(&p), vec![Invalid, Collision]);
        assert_eq!(p[0].new, b"x/y");
    }

    #[test]
    fn restricted_destination_rules_apply() {
        let entries = vec![e("a"), e("b")];
        let set = RuleSet {
            rules: vec![Rule::Suffix(":1".into())],
            include_extension: false,
        };
        let restricted = NameRules {
            restricted: true,
            ..NameRules::permissive()
        };
        let p = preview_with(&entries, &set, &names(&["a", "b"]), restricted).unwrap();
        assert_eq!(statuses(&p), vec![Invalid, Invalid]);
        let p = preview(&entries, &set, &names(&["a", "b"])).unwrap();
        assert_eq!(statuses(&p), vec![Fine, Fine]);
    }

    #[test]
    fn case_insensitive_destination_collides_on_case() {
        let entries = vec![e("a"), e("b")];
        let set = RuleSet {
            rules: vec![fr("^b$", "A", true, true)],
            include_extension: false,
        };
        let ci = NameRules {
            case_insensitive: true,
            ..NameRules::permissive()
        };
        let p = preview_with(&entries, &set, &names(&["a", "b"]), ci).unwrap();
        assert_eq!(statuses(&p), vec![Unchanged, Collision]);
        let p = preview(&entries, &set, &names(&["a", "b"])).unwrap();
        assert_eq!(statuses(&p), vec![Unchanged, Fine]);
        // A case-only rename of the entry itself is fine.
        let set = RuleSet {
            rules: vec![Rule::Case(CaseMode::Upper)],
            include_extension: false,
        };
        let p = preview_with(&[e("a")], &set, &names(&["a"]), ci).unwrap();
        assert_eq!(statuses(&p), vec![Fine]);
    }

    #[test]
    fn non_utf8_names_are_invalid() {
        let entries = vec![(b"caf\xe9".to_vec(), Some(0))];
        let set = RuleSet {
            rules: vec![Rule::Prefix("x".into())],
            include_extension: false,
        };
        let p = preview(&entries, &set, &[]).unwrap();
        assert_eq!(p[0].status, Invalid);
        assert_eq!(p[0].new, b"caf\xe9");
    }

    #[test]
    fn empty_rule_set_changes_nothing() {
        let p = preview(&[e("a.txt")], &RuleSet::default(), &names(&["a.txt"])).unwrap();
        assert_eq!(statuses(&p), vec![Unchanged]);
    }

    #[test]
    fn rules_run_in_order() {
        let out = run(
            &["a.txt"],
            vec![Rule::Prefix("b".into()), fr("^ba$", "z", true, true)],
        );
        assert_eq!(out, vec!["z.txt"]);
        let out = run(
            &["a.txt"],
            vec![fr("^ba$", "z", true, true), Rule::Prefix("b".into())],
        );
        assert_eq!(out, vec!["ba.txt"]);
    }
}
