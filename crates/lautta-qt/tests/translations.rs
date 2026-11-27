// SPDX-License-Identifier: LGPL-2.1-or-later

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn catalogue() -> String {
    std::fs::read_to_string(root().join("translations/harbour-lautta.ts")).unwrap()
}

fn release(ts: &str, dir: &Path, name: &str) {
    let source = dir.join(format!("{name}.ts"));
    std::fs::write(&source, ts).unwrap();
    let lrelease = std::env::var("LRELEASE").unwrap_or_else(|_| "lrelease".to_owned());
    let out = Command::new(lrelease)
        .args(["-idbased", "-silent"])
        .arg(&source)
        .output()
        .expect("lrelease (Qt 5 tools) is needed: see tools/ci/install-host-deps.sh");
    assert!(out.status.success(), "lrelease failed: {out:?}");
    assert!(dir.join(format!("{name}.qm")).exists());
}

fn check(dir: &Path, qml: &Path) -> i32 {
    lautta_qt::qml_check(&[
        "-T".to_owned(),
        dir.display().to_string(),
        qml.canonicalize().unwrap().display().to_string(),
    ])
}

fn read_in_qml(ts: &str) -> i32 {
    let dir = tempfile::tempdir().unwrap();
    release(ts, dir.path(), "harbour-lautta");
    check(
        dir.path(),
        &root().join("tests/translations/tst_english_plurals.qml"),
    )
}

const GERMAN: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de">
<context>
    <name></name>
    <message id="lautta-dir-grid-items" numerus="yes">
        <source>%n items</source>
        <translation>
            <numerusform>%n Objekt</numerusform>
            <numerusform>%n Objekte</numerusform>
        </translation>
    </message>
</context>
</TS>
"#;

const LAYERS: &str = r#"import QtQuick 2.6
QtObject {
    Component.onCompleted: {
        var got = [qsTrId("lautta-dir-grid-items", 1), qsTrId("lautta-dir-grid-items", 2),
                   qsTrId("lautta-dir-menu-paste", 1), qsTrId("lautta-dir-menu-paste", 2)]
        var want = ["1 Objekt", "2 Objekte", "Paste 1 item", "Paste 2 items"]
        if (JSON.stringify(got) !== JSON.stringify(want))
            console.error("FAILED: " + JSON.stringify(got) + ", not " + JSON.stringify(want))
    }
}
"#;

fn german_over_english(ts: &str) -> i32 {
    let dir = tempfile::tempdir().unwrap();
    release(ts, dir.path(), "harbour-lautta");
    release(GERMAN, dir.path(), "harbour-lautta-de");
    let qml = dir.path().join("layers.qml");
    std::fs::write(&qml, LAYERS).unwrap();
    check(dir.path(), &qml)
}

fn plural_singular(ts: &str, id: &str) -> String {
    let marker = format!("<message id=\"{id}\" numerus=\"yes\">");
    let (before, message) = ts.split_once(&marker).unwrap();
    let (message, after) = message.split_once("</message>").unwrap();
    let singular = message.split_once("<numerusform>").unwrap().1;
    let singular = singular.split_once("</numerusform>").unwrap().0;
    let plural = message.split("<numerusform>").nth(2).unwrap();
    let plural = plural.split_once("</numerusform>").unwrap().0;
    assert_ne!(singular, plural);
    format!(
        "{before}{marker}{}</message>{after}",
        message.replacen(&format!(">{singular}<"), &format!(">{plural}<"), 1)
    )
}

fn source_of(ts: &str, id: &str) -> String {
    let (_, message) = ts.split_once(&format!("<message id=\"{id}\">")).unwrap();
    let source = message.split_once("<source>").unwrap().1;
    source.split_once("</source>").unwrap().0.to_owned()
}

#[test]
fn the_start_page_and_its_settings_are_called_browse() {
    let ts = catalogue();
    assert_eq!(source_of(&ts, "lautta-home-title"), "Browse");
    assert_eq!(source_of(&ts, "lautta-settings-home-page"), "Browse page");
}

#[test]
fn the_built_catalogue_has_english_plural_forms() {
    let home = tempfile::tempdir().unwrap();
    std::env::set_var("HOME", home.path());
    std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    std::env::set_var("LC_ALL", "de_DE.UTF-8");
    std::env::set_var("LANGUAGE", "de");
    let ts = catalogue();
    assert_eq!(read_in_qml(&ts), 0, "the shipped catalogue fails the plural test");
    assert_eq!(german_over_english(&ts), 0, "German does not come over English");

    let broken_grid = plural_singular(&ts, "lautta-dir-grid-items");
    assert_ne!(read_in_qml(&broken_grid), 0, "a wrong singular goes unnoticed");
    let broken_paste = plural_singular(&ts, "lautta-dir-menu-paste");
    assert_ne!(
        german_over_english(&broken_paste),
        0,
        "a language without the message hides a wrong singular"
    );

    let no_rules = ts.replacen(" language=\"en\"", "", 1);
    assert_ne!(no_rules, ts);
    assert_ne!(
        read_in_qml(&no_rules),
        0,
        "a catalogue without a language goes unnoticed"
    );

    let empty = tempfile::tempdir().unwrap();
    let nothing = empty.path().join("nothing.qml");
    std::fs::write(&nothing, "import QtQuick 2.6\nQtObject {}\n").unwrap();
    assert_ne!(
        check(empty.path(), &nothing),
        0,
        "a missing catalogue goes unnoticed"
    );
}

fn fill(text: &str) -> (i32, String, String) {
    let mut child = Command::new("python3")
        .arg(root().join("tools/english-plurals.py"))
        .arg("fill")
        .env("PYTHONIOENCODING", "ascii")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 is needed");
    child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stderr).unwrap(),
        String::from_utf8(out.stdout).unwrap(),
    )
}

fn lupdate_output(messages: &[(&str, &str, &[&str])]) -> String {
    let mut ts = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"en\">\n<context>\n    <name></name>\n",
    );
    for (id, source, forms) in messages {
        if !source.contains("%n") {
            ts += &format!("    <message id=\"{id}\">\n        <source>{source}</source>\n        <translation type=\"unfinished\"></translation>\n    </message>\n");
            continue;
        }
        let forms: Vec<&str> = if forms.is_empty() {
            vec![""]
        } else {
            forms.to_vec()
        };
        let forms: String = forms
            .iter()
            .map(|f| format!("            <numerusform>{f}</numerusform>\n"))
            .collect();
        ts += &format!("    <message id=\"{id}\" numerus=\"yes\">\n        <source>{source}</source>\n        <translation type=\"unfinished\">\n{forms}        </translation>\n    </message>\n");
    }
    ts + "</context>\n</TS>\n"
}

fn forms_of(ts: &str, id: &str) -> Vec<String> {
    let (_, message) = ts.split_once(&format!("<message id=\"{id}\"")).unwrap();
    let message = message.split_once("</message>").unwrap().0;
    message
        .split("<numerusform>")
        .skip(1)
        .map(|f| f.split_once("</numerusform>").unwrap().0.to_owned())
        .collect()
}

#[test]
fn a_new_message_gets_the_singular_by_rule_and_the_source_as_plural() {
    let cases: &[(&str, &str, &str)] = &[
        ("a", "%n items", "%n item"),
        ("b", "Paste %n files to %1", "Paste %n file to %1"),
        ("c", "%n entries", "%n entry"),
        ("d", "Not found in %n folders", "Not found in %n folder"),
        ("e", "%n boxes", "%n box"),
        ("f", "%n classes", "%n class"),
        ("g", "%n files are skipped.", "%n file is skipped."),
        (
            "h",
            "%n items were moved and %n are left",
            "%n item was moved and %n is left",
        ),
        ("i", "%n tasks have finished", "%n task has finished"),
        ("j", "%n paused", "%n paused"),
        ("k", "%n more", "%n more"),
        ("m", "%n files are skipped and are not copied", "%n file is skipped and is not copied"),
        ("n", "Total: %n", "Total: %n"),
        (
            "o",
            "%n files weren&apos;t copied, %n aren&apos;t sent, %n haven&apos;t moved and %n don&apos;t exist",
            "%n file wasn&apos;t copied, %n isn&apos;t sent, %n hasn&apos;t moved and %n doesn&apos;t exist",
        ),
        ("p", "%n moved to %1", "%n moved to %1"),
        ("q", "%n items in the area", "%n item in the area"),
        ("r", "%n running", "%n running"),
        ("s", "%n at once", "%n at once"),
        ("t", "%n items in the software", "%n item in the software"),
        (
            "l",
            "%n files couldn&apos;t be copied",
            "%n file couldn&apos;t be copied",
        ),
    ];
    let input: Vec<(&str, &str, &[&str])> = cases.iter().map(|(id, s, _)| (*id, *s, &[][..])).collect();
    let (code, stderr, out) = fill(&lupdate_output(&input));
    assert_eq!(code, 0);
    for (id, plural, single) in cases {
        assert_eq!(forms_of(&out, id), [*single, *plural], "{id}");
        assert!(stderr.contains(&format!("english-plurals: {id}: {single} | {plural}")));
    }
    assert!(!out.contains("unfinished"), "the forms are final");
}

#[test]
fn filling_twice_changes_nothing() {
    let (_, _, once) = fill(&lupdate_output(&[("a", "%n items", &[]), ("b", "Plain", &[])]));
    let (code, stderr, twice) = fill(&once);
    assert_eq!((code, twice.as_str(), stderr.as_str()), (0, once.as_str(), ""));
}

#[test]
fn a_form_taken_away_is_made_again() {
    let ts = catalogue();
    for (from, to) in [
        (
            "<numerusform>%n item</numerusform>",
            "<numerusform></numerusform>",
        ),
        (
            "<numerusform>%n items</numerusform>",
            "<numerusform></numerusform>",
        ),
        ("<numerusform>%n item</numerusform>\n", ""),
        (
            "<numerusform>%n items</numerusform>\n",
            "<numerusform>%n items</numerusform>\n            <numerusform>x</numerusform>\n",
        ),
    ] {
        let broken = ts.replacen(from, to, 1);
        assert_ne!(broken, ts, "{from}");
        let (code, _, fixed) = fill(&broken);
        assert_eq!(code, 0);
        assert_eq!(fixed, ts, "{from} -> {to}");
    }
}

#[test]
fn a_changed_source_gets_new_forms_and_loses_the_old_source() {
    let ts = lupdate_output(&[("a", "Pack %n things", &["Compress %n item", "Compress %n items"])]).replace(
        "        <translation type=\"unfinished\">",
        "        <oldsource>Compress %n items</oldsource>\n        <translation type=\"unfinished\">",
    );
    let (code, _, out) = fill(&ts);
    assert_eq!(code, 0);
    assert_eq!(forms_of(&out, "a"), ["Pack %n thing", "Pack %n things"]);
    assert!(!out.contains("oldsource"));
}

#[test]
fn a_singular_written_by_hand_stays_while_the_source_does() {
    let ts = lupdate_output(&[("a", "%n new files", &["%n new file", "%n new files"])])
        .replace(" type=\"unfinished\"", "");
    let (code, stderr, out) = fill(&ts);
    assert_eq!((code, out.as_str(), stderr.as_str()), (0, ts.as_str(), ""));
}

#[test]
fn messages_the_rule_must_not_guess_are_refused_and_the_rest_is_filled() {
    for (source, why) in [
        ("%n file(s)", "write a plain plural, not (s)"),
        ("%n match(es)", "write a plain plural, not (s)"),
        ("%n caches", "cannot tell the singular of \"caches\""),
        ("%n matches", "cannot tell the singular of \"matches\""),
        ("%n hashes", "cannot tell the singular of \"hashes\""),
        ("%n status changes", "cannot tell the singular of \"status\""),
        ("%n process steps", "cannot tell the singular of \"process\""),
        ("%n analysis results", "cannot tell the singular of \"analysis\""),
        ("%n new files", "cannot tell the singular of \"new\""),
    ] {
        let ts = lupdate_output(&[("fine", "%n items", &[]), ("bad", source, &[])]);
        let (code, stderr, out) = fill(&ts);
        assert_eq!(code, 1, "{source}");
        assert!(
            stderr.contains(&format!("english-plurals: bad: {why}")),
            "{source}: {stderr}"
        );
        assert_eq!(forms_of(&out, "fine"), ["%n item", "%n items"], "{source}");
        assert_eq!(forms_of(&out, "bad"), [""], "{source}: left for a hand");
    }
}

#[test]
fn a_count_that_qstrid_does_not_get_is_refused() {
    let ts = lupdate_output(&[("fine", "%n items", &[]), ("bad", "Moved %n items", &[])])
        .replace("<message id=\"bad\" numerus=\"yes\">", "<message id=\"bad\">");
    let (code, stderr, out) = fill(&ts);
    assert_eq!(code, 1);
    assert!(stderr.contains("english-plurals: bad: has %n but qsTrId gets no count"));
    assert_eq!(forms_of(&out, "fine"), ["%n item", "%n items"]);
}

#[test]
fn a_catalogue_without_a_language_gets_english() {
    let ts = lupdate_output(&[("a", "%n items", &[])]).replace(" language=\"en\"", "");
    let (code, _, out) = fill(&ts);
    assert_eq!(code, 0);
    assert!(out.contains("<TS version=\"2.1\" language=\"en\">"));
    assert_eq!(forms_of(&out, "a"), ["%n item", "%n items"]);
}

#[test]
fn a_catalogue_of_another_language_is_refused_whole() {
    let ts = lupdate_output(&[("a", "%n items", &[])]).replace("language=\"en\"", "language=\"de\"");
    let (code, stderr, out) = fill(&ts);
    assert_eq!(code, 1);
    assert!(stderr.contains("must be language=\"en\""), "{stderr}");
    assert_eq!(out, "", "nothing to write over the catalogue with");
}

#[test]
fn text_that_is_no_catalogue_is_refused() {
    let (code, stderr, out) = fill("hello\n");
    assert_eq!(code, 1);
    assert!(stderr.contains("not a TS catalogue"), "{stderr}");
    assert_eq!(out, "");
}

#[test]
fn the_script_fills_stdin_and_takes_no_file() {
    for args in [vec![], vec!["fill", "x.ts"], vec!["frobnicate"]] {
        let out = Command::new("python3")
            .arg(root().join("tools/english-plurals.py"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
}

fn style_breaks(ts: &str) -> Vec<String> {
    const BANNED: [&str; 9] = [
        "Canceled",
        "Canceling",
        "Favorite",
        "Color",
        "\u{2019}",
        "MiB",
        "(s)",
        "Glob",
        "glob",
    ];
    ts.split("<source>")
        .skip(1)
        .filter_map(|rest| rest.split("</source>").next())
        .filter(|source| BANNED.iter().any(|word| source.contains(word)))
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_catalogue_keeps_one_spelling() {
    assert_eq!(style_breaks(&catalogue()), Vec::<String>::new());
    let broken = "<source>Canceled</source><source>Fine</source><source>it\u{2019}s</source>";
    assert_eq!(style_breaks(broken), ["Canceled", "it\u{2019}s"]);
}

fn without_english(ts: &str) -> Vec<String> {
    ts.split("<message id=\"")
        .skip(1)
        .filter(|message| message.contains("<source></source>"))
        .filter_map(|message| message.split('"').next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_message_has_its_english() {
    assert_eq!(without_english(&catalogue()), Vec::<String>::new());
    let broken =
        "<message id=\"a\"><source>A</source></message><message id=\"b\"><source></source></message>";
    assert_eq!(without_english(broken), ["b"]);
}

const LANGUAGES: [&str; 39] = [
    "bg", "bn", "cs", "da", "de", "el", "es", "et", "fi", "fr", "gu", "hi", "hu", "it", "kn", "lt", "lv",
    "ml", "mr", "nb", "nl", "pa", "pl", "pt", "pt_BR", "ro", "ru", "sk", "sl", "sv", "ta", "te", "tr", "tt",
    "uk", "vi", "zh_CN", "zh_HK", "zh_TW",
];

struct Message {
    id: String,
    source: String,
    finished: bool,
    forms: Vec<String>,
}

fn unescape(text: &str) -> String {
    let mut decoded = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("&#") {
        decoded.push_str(&rest[..at]);
        let (code, after) = rest[at + 2..].split_once(';').unwrap();
        let value = match code.strip_prefix('x') {
            Some(hex) => u32::from_str_radix(hex, 16).unwrap(),
            None => code.parse().unwrap(),
        };
        decoded.push(char::from_u32(value).unwrap());
        rest = after;
    }
    decoded.push_str(rest);
    decoded
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let rest = text.split_once(open)?.1;
    Some(rest.split_once(close)?.0)
}

fn language_of(ts: &str) -> String {
    between(ts, "<TS version=\"2.1\" language=\"", "\"")
        .unwrap_or_default()
        .to_owned()
}

fn messages(ts: &str) -> Vec<Message> {
    ts.split("<message id=\"")
        .skip(1)
        .map(|block| {
            let block = block.split_once("</message>").map_or(block, |(b, _)| b);
            let id = block.split('"').next().unwrap_or_default().to_owned();
            let translation = block.split_once("<translation").map_or("", |(_, t)| t);
            let finished = !translation.starts_with(" type=\"unfinished\"");
            let body = translation
                .split_once('>')
                .map_or("", |(_, t)| t)
                .split_once("</translation>")
                .map_or("", |(t, _)| t);
            let forms = if body.contains("<numerusform>") {
                body.split("<numerusform>")
                    .skip(1)
                    .map(|f| unescape(f.split_once("</numerusform>").map_or(f, |(f, _)| f)))
                    .collect()
            } else {
                vec![unescape(body)]
            };
            let source = unescape(between(block, "<source>", "</source>").unwrap_or_default());
            Message {
                id,
                source,
                finished,
                forms,
            }
        })
        .collect()
}

fn language_catalogue(language: &str) -> String {
    std::fs::read_to_string(root().join(format!("translations/harbour-lautta-{language}.ts")))
        .unwrap_or_default()
}

#[test]
fn every_sailfish_language_has_a_catalogue() {
    let missing: Vec<&str> = LANGUAGES
        .iter()
        .copied()
        .filter(|language| {
            !root()
                .join(format!("translations/harbour-lautta-{language}.ts"))
                .exists()
        })
        .collect();
    assert_eq!(missing, Vec::<&str>::new());
    let mut found: Vec<String> = std::fs::read_dir(root().join("translations"))
        .unwrap()
        .filter_map(|entry| entry.unwrap().file_name().into_string().ok())
        .filter_map(|name| {
            name.strip_prefix("harbour-lautta-")
                .and_then(|n| n.strip_suffix(".ts"))
                .map(str::to_owned)
        })
        .collect();
    found.sort();
    assert_eq!(found, LANGUAGES, "a catalogue the list above does not know");
}

fn gaps(language: &str, ts: &str, source: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if language_of(ts) != language {
        problems.push(format!("{language}: language is \"{}\"", language_of(ts)));
    }
    let mut want: Vec<String> = messages(source).into_iter().map(|m| m.id).collect();
    let mut have: Vec<String> = messages(ts).iter().map(|m| m.id.clone()).collect();
    want.sort();
    have.sort();
    if want != have {
        problems.push(format!("{language}: not the source's ids: run make translations"));
    }
    for message in messages(ts) {
        if !message.finished || message.forms.iter().any(String::is_empty) {
            problems.push(format!("{language}: {} untranslated", message.id));
        }
    }
    problems
}

#[test]
fn every_catalogue_has_every_message_translated() {
    let source = catalogue();
    assert!(messages(&source).len() > 500, "the source catalogue is cut short");
    let problems: Vec<String> = LANGUAGES
        .iter()
        .flat_map(|language| gaps(language, &language_catalogue(language), &source))
        .collect();
    assert_eq!(problems, Vec::<String>::new());

    let german = language_catalogue("de");
    let first = messages(&german).remove(0).id;
    let unfinished = german.replacen("<translation>", "<translation type=\"unfinished\">", 1);
    assert_eq!(
        gaps("de", &unfinished, &source),
        [format!("de: {first} untranslated")]
    );
    let emptied = german.replacen("<numerusform>", "<numerusform></numerusform><x>", 1);
    assert_eq!(gaps("de", &emptied, &source).len(), 1, "an empty plural form");
    let short = german.replacen("<message id=\"", "<gone id=\"", 1);
    assert_eq!(
        gaps("de", &short, &source),
        ["de: not the source's ids: run make translations"]
    );
    let misnamed = german.replacen("language=\"de\"", "language=\"en\"", 1);
    assert_eq!(gaps("de", &misnamed, &source), ["de: language is \"en\""]);
}

fn placeholders(text: &str) -> Vec<String> {
    let mut found: Vec<String> = text
        .match_indices('%')
        .filter_map(|(at, _)| text[at + 1..].chars().next())
        .filter(|c| c.is_ascii_digit() || *c == 'n')
        .map(|c| format!("%{c}"))
        .collect();
    found.sort();
    found
}

fn lost_placeholders(language: &str, ts: &str, source: &str) -> Vec<String> {
    let english: std::collections::HashMap<String, Message> =
        messages(source).into_iter().map(|m| (m.id.clone(), m)).collect();
    let mut problems = Vec::new();
    for message in messages(ts) {
        let Some(en) = english.get(&message.id) else {
            continue;
        };
        let count_optional = en.forms.len() > 1 && en.forms.iter().any(|f| !f.contains("%n"));
        let want: Vec<String> = placeholders(&en.source)
            .into_iter()
            .filter(|p| !count_optional || p != "%n")
            .collect();
        for form in &message.forms {
            let got: Vec<String> = placeholders(form)
                .into_iter()
                .filter(|p| !count_optional || p != "%n")
                .collect();
            if got != want {
                problems.push(format!(
                    "{language}: {}: \"{form}\" has {got:?}, not {want:?}",
                    message.id
                ));
            }
        }
    }
    problems
}

#[test]
fn every_translation_keeps_the_placeholders_of_its_english() {
    let source = catalogue();
    let problems: Vec<String> = LANGUAGES
        .iter()
        .flat_map(|language| lost_placeholders(language, &language_catalogue(language), &source))
        .collect();
    assert_eq!(problems, Vec::<String>::new());

    let german = language_catalogue("de");
    let sizes = messages(&german)
        .into_iter()
        .find(|m| m.id == "lautta-err-no-space-sizes")
        .unwrap();
    assert_eq!(placeholders(&sizes.forms[0]), ["%1", "%2", "%3"]);
    let without_three = german.replacen(
        &format!(">{}<", escape(&sizes.forms[0])),
        &format!(">{}<", escape(&sizes.forms[0].replace("%3", ""))),
        1,
    );
    assert_ne!(without_three, german);
    assert_eq!(lost_placeholders("de", &without_three, &source).len(), 1);
    let items = messages(&german)
        .into_iter()
        .find(|m| m.id == "lautta-dir-grid-items")
        .unwrap();
    let no_count = german.replacen(
        &format!("<numerusform>{}</numerusform>", escape(&items.forms[0])),
        &format!(
            "<numerusform>{}</numerusform>",
            escape(&items.forms[0].replace("%n", "1"))
        ),
        1,
    );
    assert_ne!(no_count, german);
    assert_eq!(lost_placeholders("de", &no_count, &source).len(), 1);
}

fn release_all(dir: &Path) {
    release(&catalogue(), dir, "harbour-lautta");
    for language in LANGUAGES {
        release(
            &language_catalogue(language),
            dir,
            &format!("harbour-lautta-{language}"),
        );
    }
}

fn js(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii() => out.push(c),
            c => out.push_str(&format!("\\u{:04x}", c as u32)),
        }
    }
    out + "\""
}

fn in_own_process(test: &str, locale: &str, body: impl FnOnce()) {
    if std::env::var("LAUTTA_LOCALE_TEST").as_deref() == Ok(test) {
        body();
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads", "1"])
        .env("LAUTTA_LOCALE_TEST", test)
        .env("HOME", home.path())
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("LC_ALL", format!("{locale}.UTF-8"))
        .env("LANGUAGE", locale)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("1 passed"),
        "{test} in {locale}:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn reads_in(want: &[(String, i32, String)]) -> i32 {
    let dir = tempfile::tempdir().unwrap();
    release_all(dir.path());
    let checks: String = want
        .iter()
        .map(|(id, n, text)| format!("        same({}, {n}, {}),\n", js(id), js(text)))
        .collect();
    let qml = format!(
        "import QtQuick 2.6\nQtObject {{\n    function same(id, n, want) {{\n        var got = n < 0 ? qsTrId(id) : qsTrId(id, n)\n        if (got !== want)\n            console.error(\"FAILED: \" + id + \" \" + n + \": \" + got + \", not \" + want)\n    }}\n    Component.onCompleted: [\n{checks}    ]\n}}\n"
    );
    let file = dir.path().join("reader.qml");
    std::fs::write(&file, qml).unwrap();
    check(dir.path(), &file)
}

fn plain_messages_of(language: &str) -> Vec<(String, i32, String)> {
    messages(&language_catalogue(language))
        .into_iter()
        .filter(|m| m.forms.len() == 1 && !m.forms[0].contains("%n"))
        .map(|m| (m.id, -1, m.forms[0].clone()))
        .collect()
}

fn readers_get(locale: &str, language: &str) {
    let want = plain_messages_of(language);
    assert!(want.len() > 400, "{language}: too few plain messages");
    assert_eq!(reads_in(&want), 0, "{locale} does not read {language}");
    let english: Vec<(String, i32, String)> = messages(&catalogue())
        .into_iter()
        .filter(|m| {
            want.iter()
                .any(|(id, _, text)| *id == m.id && *text != m.forms[0])
        })
        .take(1)
        .map(|m| (m.id, -1, m.forms[0].clone()))
        .collect();
    assert_eq!(english.len(), 1);
    assert_ne!(reads_in(&english), 0, "{locale} check cannot tell English apart");
}

#[test]
fn a_finnish_reader_gets_finnish() {
    in_own_process("a_finnish_reader_gets_finnish", "fi_FI", || {
        readers_get("fi_FI", "fi")
    });
}

#[test]
fn an_austrian_reader_gets_german() {
    in_own_process("an_austrian_reader_gets_german", "de_AT", || {
        readers_get("de_AT", "de")
    });
}

#[test]
fn a_brazilian_reader_gets_brazilian_portuguese() {
    in_own_process("a_brazilian_reader_gets_brazilian_portuguese", "pt_BR", || {
        readers_get("pt_BR", "pt_BR")
    });
}

#[test]
fn a_reader_in_portugal_gets_portuguese() {
    in_own_process("a_reader_in_portugal_gets_portuguese", "pt_PT", || {
        readers_get("pt_PT", "pt")
    });
}

#[test]
fn a_reader_in_taiwan_gets_taiwans_chinese() {
    in_own_process("a_reader_in_taiwan_gets_taiwans_chinese", "zh_TW", || {
        readers_get("zh_TW", "zh_TW")
    });
}

#[test]
fn a_reader_in_hong_kong_gets_hong_kongs_chinese() {
    in_own_process("a_reader_in_hong_kong_gets_hong_kongs_chinese", "zh_HK", || {
        readers_get("zh_HK", "zh_HK")
    });
}

#[test]
fn a_language_without_a_catalogue_gets_english() {
    in_own_process("a_language_without_a_catalogue_gets_english", "ja_JP", || {
        let want: Vec<(String, i32, String)> = vec![
            ("lautta-home-title".to_owned(), -1, "Browse".to_owned()),
            ("lautta-dir-grid-items".to_owned(), 1, "1 item".to_owned()),
            ("lautta-dir-grid-items".to_owned(), 3, "3 items".to_owned()),
        ];
        assert_eq!(reads_in(&want), 0);
        let wrong = vec![("lautta-dir-grid-items".to_owned(), 1, "1 items".to_owned())];
        assert_ne!(reads_in(&wrong), 0);
    });
}

#[test]
fn polish_counts_follow_polish_rules() {
    in_own_process("polish_counts_follow_polish_rules", "pl_PL", || {
        let polish = messages(&language_catalogue("pl"))
            .into_iter()
            .find(|m| m.id == "lautta-dir-grid-items")
            .unwrap();
        assert_eq!(polish.forms.len(), 3);
        let form_of = [(1, 0), (2, 1), (4, 1), (5, 2), (12, 2), (22, 1), (25, 2), (0, 2)];
        let want: Vec<(String, i32, String)> = form_of
            .iter()
            .map(|(n, form)| {
                (
                    polish.id.clone(),
                    *n,
                    polish.forms[*form].replace("%n", &n.to_string()),
                )
            })
            .collect();
        assert_eq!(reads_in(&want), 0);
        let swapped: Vec<(String, i32, String)> = want
            .iter()
            .map(|(id, n, text)| {
                let other = if *n == 22 {
                    polish.forms[2].replace("%n", "22")
                } else {
                    text.clone()
                };
                (id.clone(), *n, other)
            })
            .collect();
        assert_ne!(reads_in(&swapped), 0);
    });
}
