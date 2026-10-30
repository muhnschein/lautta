// SPDX-License-Identifier: LGPL-2.1-or-later
//! Highlighting tests: grammar choice, classes of samples, state across lines, limits, and that Qt
//! shows exactly the file's characters (model of Qt's StyledText parser reads output back).

use super::*;

fn syntax_name(name: &str, first_line: &str) -> Option<&'static str> {
    syntax_for(name.as_bytes(), first_line).map(|s| s.name.as_str())
}

fn coloured(name: &str, text: &str) -> ColouredText {
    let limits = Limits {
        budget: Duration::from_secs(600),
        max_line_bytes: 4096,
    };
    colour(name.as_bytes(), text.to_owned(), &limits)
}

fn pieces(doc: &ColouredText, index: usize) -> Vec<(&str, Class)> {
    let line = doc.line(index).unwrap_or_default();
    let mut from = 0;
    let mut out = Vec::new();
    for span in doc.spans(index).unwrap_or_default() {
        out.push((&line[from..span.end as usize], span.class));
        from = span.end as usize;
    }
    out
}

fn class_of(doc: &ColouredText, index: usize, text: &str) -> Option<Class> {
    pieces(doc, index)
        .into_iter()
        .find(|(p, _)| p.contains(text))
        .map(|(_, c)| c)
}

#[test]
fn grammars_by_name_extension_and_first_line() {
    let bash = Some("Bourne Again Shell (bash)");
    assert_eq!(syntax_name("build.sh", ""), bash);
    assert_eq!(syntax_name(".bashrc", ""), bash);
    assert_eq!(syntax_name("README.MD", ""), Some("Markdown"));
    assert_eq!(syntax_name("Makefile", ""), Some("Makefile"));
    assert_eq!(syntax_name("main.rs", ""), Some("Rust"));
    assert_eq!(syntax_name("\u{FFFD}.rs", ""), Some("Rust"));
    assert_eq!(
        syntax_name("x.h", ""),
        Some("C"),
        "a header is C, not Objective-C"
    );
    assert_eq!(syntax_name("Main.qml", ""), Some("JavaScript"));
    assert_eq!(syntax_name("harbour-lautta.desktop", ""), Some("Java Properties"));
    assert_eq!(syntax_name("configure", "#!/bin/sh"), bash);
    assert_eq!(syntax_name("run", "#!/usr/bin/env bash"), bash);
    assert_eq!(syntax_name("tool", "#!/usr/bin/python3"), Some("Python"));
    assert_eq!(syntax_name("meson.build", "#!/bin/sh"), bash, "not NAnt");
    assert_eq!(syntax_name("notes.txt", "#!/bin/sh"), None);
    assert_eq!(syntax_name("meson.build", "project('x')"), None);
    assert_eq!(syntax_name("start.s", ""), None);
    assert_eq!(syntax_name("data.xyz", "hello"), None);
    assert_eq!(syntax_name("", ""), None);
}

#[test]
fn class_names_round_trip() {
    for (i, (class, name)) in CLASSES.iter().enumerate() {
        assert_eq!(*class as usize, i, "{name}: CLASSES is in declaration order");
        assert_eq!(class.name(), *name);
        assert_eq!(Class::from_name(name), Some(*class));
    }
    assert_eq!(Class::from_name("Keyword"), None);
    assert_eq!(Class::from_name(""), None);
}

#[test]
fn rules_refine_before_they_generalise() {
    let scopes: Vec<Scope> = RULES.iter().map(|(p, _)| Scope::new(p).unwrap()).collect();
    for (i, general) in scopes.iter().enumerate() {
        for (j, special) in scopes.iter().enumerate().skip(i + 1) {
            assert!(
                !general.is_prefix_of(*special),
                "{} hides {} after it",
                RULES[i].0,
                RULES[j].0
            );
        }
    }
}

const BASH: &str = "#!/bin/sh
# list the text files
for f in *.txt; do
  echo \"$f is ${f%.txt}\" > /dev/null
done
name=lautta
cat <<EOF
here $name
EOF
show() { ls -l; }";

#[test]
fn a_shell_script_has_its_classes() {
    let doc = coloured("x.sh", BASH);
    assert_eq!(doc.syntax(), Some("Bourne Again Shell (bash)"));
    assert_eq!(doc.coloured_lines(), doc.line_count());
    assert_eq!(pieces(&doc, 1), vec![("# list the text files", Class::Comment)]);
    assert_eq!(class_of(&doc, 2, "for"), Some(Class::Keyword));
    assert_eq!(class_of(&doc, 2, ";"), Some(Class::Operator));
    assert_eq!(class_of(&doc, 3, "echo"), Some(Class::Function));
    assert_eq!(class_of(&doc, 3, "\""), Some(Class::String));
    assert_eq!(class_of(&doc, 3, " is "), Some(Class::String));
    assert_eq!(
        class_of(&doc, 3, "$f"),
        Some(Class::Variable),
        "{:?}",
        pieces(&doc, 3)
    );
    assert_eq!(class_of(&doc, 3, "%"), Some(Class::Operator));
    assert_eq!(class_of(&doc, 3, ">"), Some(Class::Operator));
    assert_eq!(class_of(&doc, 4, "done"), Some(Class::Keyword));
    assert_eq!(class_of(&doc, 5, "name"), Some(Class::Variable));
    assert_eq!(class_of(&doc, 7, "here "), Some(Class::String));
    assert_eq!(class_of(&doc, 8, "EOF"), Some(Class::Keyword));
    assert_eq!(class_of(&doc, 9, "show"), Some(Class::Function));
    assert_eq!(class_of(&doc, 9, "ls"), Some(Class::Function));
    assert_eq!(
        class_of(&doc, 9, "-l"),
        Some(Class::Variable),
        "options are parameters"
    );
}

const MARKDOWN: &str = "# Title

Some *emphasis*, **bold** and `code`.

- item with [a link](https://example.org)

> quoted

```
let x = 1;
```
Back to text.";

#[test]
fn markdown_has_its_classes() {
    let doc = coloured("README.md", MARKDOWN);
    assert_eq!(doc.syntax(), Some("Markdown"));
    assert_eq!(class_of(&doc, 0, "#"), Some(Class::Heading));
    assert_eq!(class_of(&doc, 0, "Title"), Some(Class::Heading));
    assert_eq!(class_of(&doc, 2, "Some"), Some(Class::Plain));
    assert_eq!(class_of(&doc, 2, "emphasis"), Some(Class::Markup));
    assert_eq!(class_of(&doc, 2, "bold"), Some(Class::Markup));
    assert_eq!(class_of(&doc, 2, "code"), Some(Class::String));
    assert_eq!(class_of(&doc, 4, "-"), Some(Class::Markup));
    assert_eq!(class_of(&doc, 4, "item"), Some(Class::Plain));
    assert_eq!(class_of(&doc, 4, "a link"), Some(Class::Link));
    assert_eq!(class_of(&doc, 4, "https://example.org"), Some(Class::Link));
    assert_eq!(class_of(&doc, 6, "quoted"), Some(Class::Markup));
    assert_eq!(pieces(&doc, 9), vec![("let x = 1;", Class::String)]);
    assert_eq!(class_of(&doc, 11, "Back"), Some(Class::Plain));
}

#[test]
fn a_comment_over_lines_keeps_its_state() {
    let doc = coloured("x.c", "int a; /* starts\nall comment, int b;\nends */ int c = 2;");
    assert_eq!(pieces(&doc, 1), vec![("all comment, int b;", Class::Comment)]);
    assert_eq!(class_of(&doc, 2, "ends */"), Some(Class::Comment));
    assert_eq!(class_of(&doc, 2, "int"), Some(Class::Keyword));
    assert_eq!(class_of(&doc, 2, "2"), Some(Class::Number));
    assert_eq!(class_of(&doc, 0, "int"), Some(Class::Keyword));
}

#[test]
fn a_diff_has_its_classes() {
    let doc = coloured("fix.patch", "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n same");
    assert_eq!(class_of(&doc, 0, "a/x"), Some(Class::Heading));
    assert_eq!(class_of(&doc, 2, "-1"), Some(Class::Heading));
    assert_eq!(pieces(&doc, 3), vec![("-old", Class::Deleted)]);
    assert_eq!(pieces(&doc, 4), vec![("+new", Class::Inserted)]);
    assert_eq!(class_of(&doc, 5, "same"), Some(Class::Plain));
}

#[test]
fn spans_are_ordered_and_cover_their_lines() {
    for (name, text) in [("x.sh", BASH), ("x.md", MARKDOWN)] {
        let doc = coloured(name, text);
        for i in 0..doc.line_count() {
            let spans = doc.spans(i).unwrap();
            let line = doc.line(i).unwrap();
            let mut end = 0;
            for (k, span) in spans.iter().enumerate() {
                assert!(span.end as usize > end, "{name}:{i} span {k} is empty");
                if k > 0 {
                    assert_ne!(span.class, spans[k - 1].class, "{name}:{i} like spans merge");
                }
                end = span.end as usize;
            }
            assert_eq!(end, line.len(), "{name}:{i}");
        }
    }
}

#[test]
fn lines_split_as_the_viewer_splits_them() {
    let doc = ColouredText::plain("a\n\nb\n".to_owned());
    assert_eq!(doc.line_count(), 4);
    let lines: Vec<_> = (0..5).map(|i| doc.line(i)).collect();
    assert_eq!(lines, [Some("a"), Some(""), Some("b"), Some(""), None]);
    assert_eq!(ColouredText::plain(String::new()).line_count(), 1);
    assert_eq!(doc.coloured_lines(), 0);
    assert_eq!(doc.spans(0), None);
    assert_eq!(doc.syntax(), None);
    assert_eq!(doc.text(), "a\n\nb\n");
}

#[test]
fn plain_text_gets_no_grammar() {
    let doc = coloured("notes.txt", "for x in y; do\n# no comment\n");
    assert_eq!((doc.syntax(), doc.coloured_lines()), (None, 0));
    assert_eq!(doc.line(1), Some("# no comment"));
}

#[test]
fn an_overlong_line_ends_the_colours() {
    let long = "echo ".to_owned() + &"x".repeat(5000);
    let text = format!("echo a\n{long}\necho b");
    let doc = coloured("x.sh", &text);
    assert_eq!(doc.coloured_lines(), 1);
    assert!(doc.spans(1).is_none() && doc.spans(2).is_none());
    let palette = Palette::parse("function=#112233");
    assert!(doc.styled_row(0, 0, "echo a", &palette).contains("<font"));
    assert_eq!(doc.styled_row(2, 0, "echo b", &palette), "echo b");
    let at_limit = format!("echo a\n{}\necho b", &long[..4096]);
    assert_eq!(coloured("x.sh", &at_limit).coloured_lines(), 3);
}

#[test]
fn colours_stop_when_time_is_up() {
    let syntax = syntax_for(b"x.sh", "").unwrap();
    let mut asked = 0;
    let doc = colour_with("a\nb\nc\nd\ne".to_owned(), syntax, 4096, || {
        asked += 1;
        asked > 2
    });
    assert_eq!(doc.coloured_lines(), 2);
    assert_eq!(doc.line_count(), 5);
    assert_eq!(doc.syntax(), Some("Bourne Again Shell (bash)"));
}

fn colour_against_the_clock(name: &str, text: String) -> ColouredText {
    let limits = Limits {
        budget: Duration::from_millis(200),
        max_line_bytes: 4096,
    };
    let started = std::time::Instant::now();
    let doc = colour(name.as_bytes(), text, &limits);
    let took = started.elapsed();
    assert!(
        took < limits.budget + Duration::from_secs(5),
        "{name} took {took:?}"
    );
    assert!(
        doc.spans(doc.coloured_lines()).is_none(),
        "{name}: plain after the coloured lines"
    );
    doc
}

#[test]
fn pathological_files_stay_within_their_limits() {
    let doc = colour_against_the_clock("x.sh", "$((".repeat(350_000));
    assert_eq!((doc.coloured_lines(), doc.line_count()), (0, 1));
    let doc = colour_against_the_clock("x.sh", "echo \"$(echo \"$( [[ ${a[(((1\n".repeat(100_000));
    assert!(doc.coloured_lines() > 0);
    // Markup openers just under the limit: tens of ms a line, so time runs out.
    let doc = colour_against_the_clock("x.md", ("*_[`<".repeat(800) + "\n").repeat(2000));
    assert!(doc.coloured_lines() > 0 && doc.coloured_lines() < doc.line_count());
    let doc = colour_against_the_clock("x.html", "<a href=\"".repeat(400).repeat(1000));
    assert_eq!(doc.coloured_lines(), 0, "one long line");
}

#[test]
fn palettes_take_only_hex_colours_of_known_classes() {
    let p = Palette::parse(
        " keyword = #A1b2C3 ,string=#80ff0000,comment=red,number=#12345g,\
         type=#123,link=#1234567,function=#abcdef\" onload=\"x,plain=#ffffff,\
         nonsense=#000000,variable,=#000000,heading=#010203",
    );
    assert_eq!(p.colour(Class::Keyword), Some("#A1b2C3"));
    assert_eq!(p.colour(Class::String), Some("#80ff0000"));
    assert_eq!(p.colour(Class::Heading), Some("#010203"));
    for class in [
        Class::Comment,
        Class::Number,
        Class::Type,
        Class::Link,
        Class::Function,
        Class::Plain,
        Class::Variable,
    ] {
        assert_eq!(p.colour(class), None, "{class:?}");
    }
    assert_eq!(Palette::parse(""), Palette::default());
}

fn palette() -> Palette {
    let spec: Vec<String> = CLASSES
        .iter()
        .enumerate()
        .map(|(i, (_, name))| format!("{name}=#0000{i:02x}"))
        .collect();
    Palette::parse(&spec.join(","))
}

fn doc_of(pieces: &[(&str, Class)]) -> ColouredText {
    let line: String = pieces.iter().map(|(p, _)| *p).collect();
    let mut doc = ColouredText::plain(line);
    let mut end = 0;
    for (piece, class) in pieces {
        end += piece.len();
        doc.spans.push(Span {
            end: end as u32,
            class: *class,
        });
    }
    doc.span_starts.push(0);
    doc
}

#[test]
fn styled_rows_escape_and_colour() {
    let line = "if <img src=\"a.png\"> &lt; & a;b";
    let doc = doc_of(&[
        ("if", Class::Keyword),
        (" ", Class::Plain),
        ("<img src=\"a.png\">", Class::String),
        (" &lt; & a;b", Class::Plain),
    ]);
    assert_eq!(
        doc.styled_row(0, 0, line, &palette()),
        "<font color=\"#000002\">if</font> <font color=\"#000004\">&lt;img src=&quot;a.png&quot;&gt;</font> \
         &amp;lt; &amp; a;b"
    );
    assert_eq!(
        doc.styled_row(0, 0, line, &Palette::default()),
        "if &lt;img src=&quot;a.png&quot;&gt; &amp;lt; &amp; a;b"
    );
    assert_eq!(doc.styled_row(1, 0, "<b>", &palette()), "&lt;b&gt;");
    assert_eq!(
        doc.styled_row(usize::MAX, 0, "\"x\"", &palette()),
        "&quot;x&quot;"
    );
    let plain = ColouredText::plain("<b>&nbsp;</b>\n\0x".to_owned());
    assert_eq!(
        plain.styled_row(0, 0, "<b>&nbsp;</b>", &palette()),
        "&lt;b&gt;&amp;nbsp;&lt;/b&gt;"
    );
    assert_eq!(plain.styled_row(1, 0, "\0x", &palette()), "\u{FFFD}x");
}

#[test]
fn styled_rows_keep_whitespace() {
    let row = |text: &str| ColouredText::plain(text.to_owned()).styled_row(0, 0, text, &Palette::default());
    assert_eq!(row("a b"), "a b", "a lone space can wrap");
    assert_eq!(row(" a  b "), "&nbsp;a&nbsp;&nbsp;b&nbsp;");
    assert_eq!(row("a\u{a0}b\u{3000}c"), "a&nbsp;b&nbsp;c");
    assert_eq!(row("a & b"), "a &amp; b");
}

#[test]
fn colours_follow_tabs_and_rows() {
    let p = palette();
    let doc = doc_of(&[
        ("\t", Class::Plain),
        ("if", Class::Keyword),
        ("\t", Class::Plain),
        ("x", Class::String),
    ]);
    assert_eq!(rows_of(doc.line(0).unwrap()), [(0, "    if  x".to_owned())]);
    assert_eq!(
        doc.styled_row(0, 0, "    if  x", &p),
        "&nbsp;&nbsp;&nbsp;&nbsp;<font color=\"#000002\">if</font>&nbsp;&nbsp;<font color=\"#000004\">x</font>"
    );
    let doc = doc_of(&[("a", Class::Plain), ("\tb", Class::Comment)]);
    assert_eq!(
        doc.styled_row(0, 0, "a   b", &p),
        "a<font color=\"#000001\">&nbsp;&nbsp;&nbsp;b</font>"
    );
    // Columns are UTF-16 units: tab after an emoji is two columns.
    let doc = doc_of(&[
        ("\u{1F600}\t", Class::Plain),
        ("x", Class::Keyword),
        ("\u{1F600}", Class::Plain),
        ("y", Class::Keyword),
    ]);
    let row = "\u{1F600}  x\u{1F600}y";
    assert_eq!(rows_of(doc.line(0).unwrap()), [(0, row.to_owned())]);
    assert_eq!(
        doc.styled_row(0, 0, row, &p),
        "\u{1F600}&nbsp;&nbsp;<font color=\"#000002\">x</font>\u{1F600}<font color=\"#000002\">y</font>"
    );
    let doc = doc_of(&[
        ("a".repeat(998).as_str(), Class::Plain),
        ("echo", Class::Function),
        (" b", Class::Plain),
    ]);
    let rows = rows_of(doc.line(0).unwrap());
    assert_eq!(rows[1], (1000, "ho b".to_owned()));
    assert_eq!(
        doc.styled_row(0, 1000, "ho b", &p),
        "<font color=\"#000008\">ho</font> b"
    );
    for (start, row) in &rows {
        check_row(&doc, 0, (*start, row), &p);
    }
}

/// Mirrors Qt 5.6 qquickstyledtext.cpp: fails on any tag but lautta's font tags, any entity but the
/// five escapes, and NUL (where Qt stops).
fn shown_by_qt(styled: &str) -> Vec<(char, Option<String>)> {
    let mut qt = QtParser {
        shown: Vec::new(),
        colours: Vec::new(),
        pending: String::new(),
        has_space: true,
        prepend_space: false,
    };
    let mut rest = styled;
    while let Some(c) = rest.chars().next() {
        rest = &rest[c.len_utf8()..];
        match c {
            '<' => rest = qt.tag(rest),
            '&' => rest = qt.entity(rest),
            '\0' => panic!("NUL ends Qt's parsing: {styled:?}"),
            c if c.is_whitespace() => {
                qt.append_pending();
                qt.prepend_space = !qt.has_space;
                rest = rest.trim_start_matches(char::is_whitespace);
            }
            c => qt.pending.push(c),
        }
    }
    qt.append_pending();
    qt.shown
}

struct QtParser {
    shown: Vec<(char, Option<String>)>,
    colours: Vec<String>,
    pending: String,
    has_space: bool,
    prepend_space: bool,
}

impl QtParser {
    fn colour(&self) -> Option<String> {
        self.colours.last().cloned()
    }

    fn append_text(&mut self) {
        let colour = self.colour();
        if self.prepend_space {
            self.shown.push((' ', colour.clone()));
        }
        for c in std::mem::take(&mut self.pending).chars() {
            self.shown.push((c, colour.clone()));
        }
        self.prepend_space = false;
        self.has_space = false;
    }

    fn append_pending(&mut self) {
        if !self.pending.is_empty() {
            self.append_text();
        }
    }

    fn tag<'a>(&mut self, rest: &'a str) -> &'a str {
        if !self.pending.is_empty() {
            self.append_text();
        } else if self.prepend_space {
            self.shown.push((' ', self.colour()));
            self.prepend_space = false;
            self.has_space = true;
        }
        let (tag, rest) = rest.split_once('>').expect("an open tag");
        if tag == "/font" {
            self.colours.pop().expect("a font tag to close");
        } else {
            let colour = tag
                .strip_prefix("font color=\"")
                .and_then(|t| t.strip_suffix('"'));
            let colour = colour.unwrap_or_else(|| panic!("a tag from the file: <{tag}>"));
            assert!(is_hex_colour(colour), "colour {colour}");
            self.colours.push(colour.to_owned());
        }
        rest
    }

    fn entity<'a>(&mut self, rest: &'a str) -> &'a str {
        self.append_text();
        let (entity, rest) = rest.split_once(';').expect("an open entity");
        let c = match entity {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "nbsp" => ' ',
            other => panic!("an entity from the file: &{other};"),
        };
        self.shown.push((c, self.colour()));
        rest
    }
}

/// Rows as TextRows.js makes them, with starts: tabs to next multiple of four, row ends after 1000
/// columns or before (never inside a surrogate pair), UTF-16 units.
fn rows_of(line: &str) -> Vec<(usize, String)> {
    let mut expanded = String::new();
    let mut column = 0;
    for c in line.chars() {
        if c == '\t' {
            let pad = 4 - column % 4;
            expanded.extend(std::iter::repeat_n(' ', pad));
            column += pad;
        } else {
            expanded.push(c);
            column += c.len_utf16();
        }
    }
    let mut rows = vec![(0, String::new())];
    let mut column = 0;
    for c in expanded.chars() {
        let start = rows.last().unwrap().0;
        if column + c.len_utf16() - start > 1000 {
            rows.push((column, c.to_string()));
        } else {
            rows.last_mut().unwrap().1.push(c);
        }
        column += c.len_utf16();
    }
    rows
}

/// Whitespace as space, NUL as U+FFFD.
fn expected(row: &str) -> String {
    row.chars()
        .map(|c| match c {
            '\0' => '\u{FFFD}',
            c if c.is_whitespace() => ' ',
            c => c,
        })
        .collect()
}

fn column_classes(doc: &ColouredText, index: usize) -> Vec<Option<Class>> {
    let line = doc.line(index).unwrap();
    let spans = doc.spans(index).unwrap_or_default();
    let mut out: Vec<Option<Class>> = Vec::new();
    let mut spans = spans.iter().peekable();
    for (at, c) in line.char_indices() {
        while spans.peek().is_some_and(|span| span.end as usize <= at) {
            spans.next();
        }
        let class = spans.peek().map(|span| span.class);
        let width = if c == '\t' {
            4 - out.len() % 4
        } else {
            c.len_utf16()
        };
        out.extend(std::iter::repeat_n(class, width));
    }
    out
}

fn check_row(doc: &ColouredText, index: usize, (start, row): (usize, &str), palette: &Palette) {
    let styled = doc.styled_row(index, start, row, palette);
    let shown = shown_by_qt(&styled);
    let text: String = shown.iter().map(|(c, _)| *c).collect();
    assert_eq!(text, expected(row), "{row:?} became {styled:?}");
    let classes = column_classes(doc, index);
    let mut column = start;
    let mut colours = Vec::new();
    for c in row.chars() {
        if !c.is_whitespace() {
            let class = classes.get(column).copied().flatten();
            colours.push(class.and_then(|class| palette.colour(class)).map(str::to_owned));
        }
        column += c.len_utf16();
    }
    let shown_colours: Vec<_> = shown
        .into_iter()
        .filter(|(c, _)| *c != ' ')
        .map(|(_, k)| k)
        .collect();
    assert_eq!(shown_colours, colours, "{row:?} became {styled:?}");
}

fn check_shown(doc: &ColouredText, index: usize, palette: &Palette) {
    for (start, row) in rows_of(doc.line(index).unwrap()) {
        check_row(doc, index, (start, &row), palette);
    }
}

/// Deterministic xorshift: failures reproduce.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

const NASTY: &[&str] = &[
    "<",
    ">",
    "&",
    ";",
    "\"",
    "'",
    " ",
    "  ",
    "\t",
    "\0",
    "a",
    "é",
    "漢",
    "\u{1F600}",
    "\u{a0}",
    "\u{2028}",
    "\u{3000}",
    "#",
    "/",
    "\\",
    "1",
    "x;",
    "<img src=\"http://example.org/a.png\">",
    "<img src=image://lautta-thumb/x>",
    "&lt;",
    "&amp;",
    "&nbsp;",
    "&#60;",
    "&quot",
    "<font color=\"#ff0000\">",
    "</font>",
    "<b>",
    "</a>",
    "<a href=\"x\">",
    "<br>",
    "<p",
    "-->",
    "<!--",
    "`",
    "*",
    "$(",
    "\"$x\"",
    "'''",
    "/*",
    "*/",
    "#!/bin/sh",
    "```",
    "> ",
    "- ",
];

fn nasty_line(rng: &mut Rng) -> String {
    (0..rng.below(24))
        .map(|_| NASTY[rng.below(NASTY.len())])
        .collect()
}

#[test]
fn qt_shows_random_markup_as_text() {
    let mut rng = Rng(0x5eed_1a77a);
    let classes: Vec<Class> = CLASSES.iter().map(|(c, _)| *c).collect();
    for n in 0..2000 {
        let line = nasty_line(&mut rng).repeat(if n % 20 == 0 { 120 } else { 1 });
        let cuts: Vec<usize> = line.char_indices().map(|(i, _)| i).skip(1).collect();
        let cuts: Vec<usize> = cuts.into_iter().filter(|_| rng.below(3) == 0).collect();
        let mut pieces = Vec::new();
        let mut from = 0;
        for cut in cuts {
            pieces.push((&line[from..cut], classes[rng.below(classes.len())]));
            from = cut;
        }
        pieces.push((&line[from..], classes[rng.below(classes.len())]));
        check_shown(&doc_of(&pieces), 0, &palette());
        check_shown(&ColouredText::plain(line.clone()), 0, &palette());
    }
}

#[test]
fn qt_shows_highlighted_random_files_as_text() {
    let mut rng = Rng(0xc0de);
    let names = [
        "x.sh", "x.md", "x.html", "x.rs", "x.py", "x.js", "x.xml", "x.diff", "x.c", "Makefile",
    ];
    for name in names {
        for _ in 0..3 {
            let text: Vec<String> = (0..40).map(|_| nasty_line(&mut rng)).collect();
            let doc = coloured(name, &text.join("\n"));
            assert_eq!(doc.coloured_lines(), doc.line_count(), "{name}");
            for i in 0..doc.line_count() {
                check_shown(&doc, i, &palette());
            }
        }
    }
}

#[test]
fn the_model_of_qt_catches_unescaped_text() {
    for styled in ["<img src=x>", "&apos;", "a\0b"] {
        assert!(
            std::panic::catch_unwind(|| shown_by_qt(styled)).is_err(),
            "{styled}"
        );
    }
    let text: String = shown_by_qt("a  b").iter().map(|(c, _)| *c).collect();
    assert_eq!(text, "a b");
    let text: String = shown_by_qt(" a").iter().map(|(c, _)| *c).collect();
    assert_eq!(text, "a");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_colours_off_the_runtime() {
    // Budget cannot run out: coverage build on busy runner can exceed LIMITS' 3 s, then `finish`
    // races GRACE.
    let limits = Limits {
        budget: Duration::from_secs(600),
        ..LIMITS
    };
    let job = start_with(b"x.sh".to_vec(), BASH.to_owned(), limits, GRACE);
    let doc = job.wait(Duration::from_secs(600)).await;
    let doc = doc.unwrap().unwrap();
    assert_eq!(doc.syntax(), Some("Bourne Again Shell (bash)"));
    assert_eq!(doc.coloured_lines(), doc.line_count());
    let job = start_with(b"x.md".to_vec(), MARKDOWN.repeat(20), limits, GRACE);
    let job = job.wait(Duration::ZERO).await.unwrap_err();
    let doc = job.finish().await.unwrap();
    assert_eq!(doc.syntax(), Some("Markdown"));
    assert_eq!(doc.coloured_lines(), doc.line_count());
}

#[test]
fn a_job_that_overruns_its_time_is_dropped() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let limits = Limits {
        budget: Duration::from_millis(200),
        max_line_bytes: usize::MAX,
    };
    let line = "echo \"$x\" 'y' ${z} # c ".repeat(80_000);
    let finished = rt.block_on(async {
        start_with(b"x.sh".to_vec(), line, limits, Duration::ZERO)
            .finish()
            .await
    });
    assert!(finished.is_none());
    rt.shutdown_background();
}
