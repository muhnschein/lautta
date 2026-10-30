// SPDX-License-Identifier: LGPL-2.1-or-later
//! First test builds the catalogue like the RPM (`lrelease -idbased`) and has the real QML engine
//! read it. Others run `tools/english-plurals.py` on catalogues with a form missing or a message it
//! must not guess.

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

/// Grid's message (German has it) and paste entry's (it has not).
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

/// User's language over engineering English, falling back to English plural forms (install order of
/// `lauttaInstallTranslations`).
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

/// Qt takes one application per process: every run is in this test. Failing runs log QML errors on
/// purpose.
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

    // Without the language lrelease writes no plural rules: every n reads the first form.
    let no_rules = ts.replacen(" language=\"en\"", "", 1);
    assert_ne!(no_rules, ts);
    assert_ne!(
        read_in_qml(&no_rules),
        0,
        "a catalogue without a language goes unnoticed"
    );

    // Folder without the engineering English catalogue fails the check itself.
    let empty = tempfile::tempdir().unwrap();
    let nothing = empty.path().join("nothing.qml");
    std::fs::write(&nothing, "import QtQuick 2.6\nQtObject {}\n").unwrap();
    assert_ne!(
        check(empty.path(), &nothing),
        0,
        "a missing catalogue goes unnoticed"
    );
}

/// stdio ASCII as in a locale without UTF-8. Returns exit code, stderr, filled catalogue (stdout).
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

/// As lupdate writes them: `(id, source, forms)`; no forms = new message (empty form), numerus when
/// source has `%n`.
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
