// SPDX-License-Identifier: LGPL-2.1-or-later
//! Syntax colours for the text viewer (SPEC PRV-13).
//!
//! The grammars are the Sublime Text ones bundled with `syntect`, run by its
//! pure-Rust regex engine (no oniguruma, SPEC RS-2). A grammar's scopes are
//! reduced to a few [`Class`]es; the viewer picks a colour of the ambience for
//! each, so no colour ever comes from a grammar or theme file.
//!
//! The viewer shows a line in rows (TextRows.js: tabs expanded, a long line
//! split), each as Qt StyledText: every character of the row is escaped and
//! whitespace is kept with `&nbsp;`, so nothing in a file is read as markup
//! (no `<img>`, no links, no entities of its own); the only tags are this
//! module's `<font color>` with a colour checked to be `#` and hex digits.
//!
//! Multi-line constructs carry the parser's state from line to line, so the
//! document is coloured as a whole, off the GUI thread and within
//! [`Limits`]: lines after one that is too long, or once time is up, stay
//! plain. [`start`] runs it on the blocking pool (ARC-7).

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use syntect::easy::ScopeRangeIterator;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};
use tokio::task::JoinHandle;

/// What a piece of a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Class {
    Plain,
    Comment,
    Keyword,
    Operator,
    String,
    Number,
    Constant,
    Type,
    Function,
    Variable,
    /// Markdown headings, diff file and hunk headers.
    Heading,
    /// Emphasis, quotes and list bullets in Markdown.
    Markup,
    Link,
    /// Added and removed lines of a diff.
    Inserted,
    Deleted,
}

/// Every class with its name in a [`Palette`].
pub const CLASSES: [(Class, &str); 15] = [
    (Class::Plain, "plain"),
    (Class::Comment, "comment"),
    (Class::Keyword, "keyword"),
    (Class::Operator, "operator"),
    (Class::String, "string"),
    (Class::Number, "number"),
    (Class::Constant, "constant"),
    (Class::Type, "type"),
    (Class::Function, "function"),
    (Class::Variable, "variable"),
    (Class::Heading, "heading"),
    (Class::Markup, "markup"),
    (Class::Link, "link"),
    (Class::Inserted, "inserted"),
    (Class::Deleted, "deleted"),
];

impl Class {
    pub fn name(self) -> &'static str {
        CLASSES[self as usize].1
    }

    pub fn from_name(name: &str) -> Option<Class> {
        CLASSES.iter().find(|(_, n)| *n == name).map(|(c, _)| *c)
    }
}

/// Scope prefixes and their class. For one scope the first prefix that
/// matches decides, so a prefix comes before the shorter ones it refines
/// (checked by a test). A scope no prefix matches says nothing: the one
/// enclosing it decides, and plain text is what is left.
const RULES: &[(&str, Class)] = &[
    ("comment", Class::Comment),
    ("punctuation.definition.variable", Class::Variable),
    ("punctuation.definition.list_item", Class::Markup),
    ("string.other.link", Class::Link),
    ("string", Class::String),
    ("constant.numeric", Class::Number),
    ("constant", Class::Constant),
    ("keyword.operator", Class::Operator),
    ("keyword", Class::Keyword),
    ("storage", Class::Keyword),
    ("entity.name.section", Class::Heading),
    ("entity.name.tag", Class::Keyword),
    ("entity.name.constant", Class::Constant),
    ("entity.name.type", Class::Type),
    ("entity.name.class", Class::Type),
    ("entity.name.struct", Class::Type),
    ("entity.name.enum", Class::Type),
    ("entity.name.trait", Class::Type),
    ("entity.name.interface", Class::Type),
    ("entity.name.namespace", Class::Type),
    ("entity.name", Class::Function),
    ("entity.other.inherited-class", Class::Type),
    ("entity.other.attribute-name", Class::Function),
    ("support.function", Class::Function),
    ("support.macro", Class::Function),
    ("support.type", Class::Type),
    ("support.class", Class::Type),
    ("support.constant", Class::Constant),
    ("support.variable", Class::Variable),
    ("variable.function", Class::Function),
    ("variable.parameter", Class::Variable),
    ("variable.language", Class::Keyword),
    ("variable.other.constant", Class::Constant),
    // Shell variables: `x=…`, `$x`, `${x}`. Other grammars (JavaScript)
    // mark every identifier as a variable, which would colour all of it.
    ("variable.other.readwrite.assignment", Class::Variable),
    ("meta.group.expansion.parameter", Class::Variable),
    ("markup.heading", Class::Heading),
    ("markup.bold", Class::Markup),
    ("markup.italic", Class::Markup),
    ("markup.quote", Class::Markup),
    ("markup.underline.link", Class::Link),
    ("meta.link", Class::Link),
    ("markup.raw", Class::String),
    ("markup.inserted", Class::Inserted),
    ("markup.deleted", Class::Deleted),
    ("meta.diff.header", Class::Heading),
    ("meta.diff.range", Class::Heading),
    // Code embedded in a string or a document is code again.
    ("source", Class::Plain),
];

/// When colouring gives up (PRV-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Wall time for the whole document; the lines after it stay plain.
    pub budget: Duration,
    /// A longer line, and every line after it, stays plain.
    pub max_line_bytes: usize,
}

pub const LIMITS: Limits = Limits {
    budget: Duration::from_secs(3),
    max_line_bytes: 4096,
};

/// How long the viewer waits for the colours before it shows the text
/// plain and colours it when they arrive (no flash of plain text for the
/// files that are coloured at once).
pub const FIRST_WAIT: Duration = Duration::from_millis(250);

/// A colouring whose last line ran this much over the budget is dropped:
/// one line can only be cut short between lines.
const GRACE: Duration = Duration::from_millis(500);

/// Columns per tab in the viewer's rows (TextRows.js).
const TAB_WIDTH: usize = 4;

/// Columns `c` takes at `column` in the viewer's rows, which count UTF-16
/// units as JavaScript does and expand a tab to the next tab stop.
fn columns(column: usize, c: char) -> usize {
    if c == '\t' {
        TAB_WIDTH - column % TAB_WIDTH
    } else {
        c.len_utf16()
    }
}

/// A piece of a line: up to byte `end` of the line (from the previous
/// span's end), in `class`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub end: u32,
    pub class: Class,
}

/// A document's lines (split at `\n`, as the viewer does) and the colours
/// of its first [`ColouredText::coloured_lines`] lines.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ColouredText {
    text: String,
    /// Where each line starts in `text`.
    line_starts: Vec<usize>,
    syntax: Option<&'static str>,
    spans: Vec<Span>,
    /// Where each coloured line's spans start in `spans`.
    span_starts: Vec<usize>,
}

impl ColouredText {
    /// The text without colours.
    pub fn plain(text: String) -> ColouredText {
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        ColouredText {
            text,
            line_starts,
            ..ColouredText::default()
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Lines from the first that have colours (0: none).
    pub fn coloured_lines(&self) -> usize {
        self.span_starts.len()
    }

    /// The grammar's name, `None` for plain text.
    pub fn syntax(&self) -> Option<&'static str> {
        self.syntax
    }

    pub fn line(&self, index: usize) -> Option<&str> {
        let start = *self.line_starts.get(index)?;
        let end = self
            .line_starts
            .get(index + 1)
            .map_or(self.text.len(), |next| next - 1);
        self.text.get(start..end)
    }

    /// The spans of a coloured line.
    pub fn spans(&self, index: usize) -> Option<&[Span]> {
        let start = *self.span_starts.get(index)?;
        let end = self
            .span_starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.spans.len());
        self.spans.get(start..end)
    }

    /// The viewer's row `row` of line `index` as StyledText (see the module
    /// docs). The row starts at column `start` of the line as the viewer
    /// lays it out (UTF-16 units, tabs expanded), and each of its
    /// characters takes the colour in `palette` of the span it comes from.
    /// The characters are the row's own: a row that does not match the line
    /// could only get wrong colours, never other text. Lines without
    /// colours, or past the end, are escaped all the same.
    pub fn styled_row(&self, index: usize, start: usize, row: &str, palette: &Palette) -> String {
        let ends = self.column_spans(index);
        let mut spans = ends.iter().peekable();
        let mut out = StyledWriter::with_capacity(row.len());
        let mut column = start;
        let mut chars = row.chars().peekable();
        while let Some(c) = chars.next() {
            while spans.peek().is_some_and(|(end, _)| *end <= column) {
                spans.next();
            }
            out.set_colour(spans.peek().and_then(|(_, class)| palette.colour(*class)));
            out.push_char(c, chars.peek().copied());
            column += c.len_utf16();
        }
        out.finish()
    }

    /// The column at which each span of line `index` ends, with its class;
    /// none for a line without colours.
    fn column_spans(&self, index: usize) -> Vec<(usize, Class)> {
        let (Some(line), Some(spans)) = (self.line(index), self.spans(index)) else {
            return Vec::new();
        };
        let mut column = 0;
        let mut from = 0;
        spans
            .iter()
            .map(|span| {
                let to = (span.end as usize).clamp(from, line.len());
                let piece = line.get(from..to).unwrap_or_default();
                column = piece.chars().fold(column, |at, c| at + columns(at, c));
                from = to;
                (column, span.class)
            })
            .collect()
    }
}

/// Colours of the classes, as the viewer sends them:
/// `class=#rrggbb` (or `#aarrggbb`) separated by commas. Unknown classes
/// and anything that is not such a colour are ignored, so nothing from it
/// but a checked colour ends up in the markup. Plain text has no colour of
/// its own: it is the text's.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Palette {
    colours: [Option<String>; CLASSES.len()],
}

impl Palette {
    pub fn parse(spec: &str) -> Palette {
        let mut palette = Palette::default();
        for (name, colour) in spec.split(',').filter_map(|item| item.split_once('=')) {
            let colour = colour.trim();
            match Class::from_name(name.trim()) {
                Some(class) if class != Class::Plain && is_hex_colour(colour) => {
                    palette.colours[class as usize] = Some(colour.to_owned());
                }
                _ => {}
            }
        }
        palette
    }

    pub fn colour(&self, class: Class) -> Option<&str> {
        self.colours[class as usize].as_deref()
    }
}

fn is_hex_colour(s: &str) -> bool {
    s.strip_prefix('#')
        .is_some_and(|hex| matches!(hex.len(), 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Builds one StyledText row.
///
/// Qt's StyledText parser collapses whitespace runs and drops leading
/// whitespace, takes `&` up to the next `;` as an entity (unknown ones
/// vanish) and stops at a NUL character. So `<`, `>`, `&` and `"` become
/// entities, NUL becomes U+FFFD and whitespace `&nbsp;` (a row's tabs are
/// already spaces), except a lone space between two visible characters,
/// which stays a space so that wrapping can break there.
struct StyledWriter<'a> {
    out: String,
    colour: Option<&'a str>,
    open: bool,
    after_space: bool,
}

impl<'a> StyledWriter<'a> {
    fn with_capacity(len: usize) -> StyledWriter<'a> {
        StyledWriter {
            out: String::with_capacity(len + len / 2),
            colour: None,
            open: false,
            after_space: true,
        }
    }

    fn set_colour(&mut self, colour: Option<&'a str>) {
        if self.colour != colour {
            self.close();
            self.colour = colour;
        }
    }

    fn close(&mut self) {
        if self.open {
            self.out.push_str("</font>");
            self.open = false;
        }
    }

    fn open(&mut self) {
        if let (false, Some(colour)) = (self.open, self.colour) {
            self.out.push_str("<font color=\"");
            self.out.push_str(colour);
            self.out.push_str("\">");
            self.open = true;
        }
    }

    /// `c`, followed by `next` in the row.
    fn push_char(&mut self, c: char, next: Option<char>) {
        self.open();
        let lone_space = c == ' ' && !self.after_space && next.is_some_and(|n| !n.is_whitespace());
        match c {
            _ if lone_space => self.out.push(' '),
            _ if c.is_whitespace() => self.out.push_str("&nbsp;"),
            '<' => self.out.push_str("&lt;"),
            '>' => self.out.push_str("&gt;"),
            '&' => self.out.push_str("&amp;"),
            '"' => self.out.push_str("&quot;"),
            '\0' => self.out.push('\u{FFFD}'),
            _ => self.out.push(c),
        }
        self.after_space = c.is_whitespace();
    }

    fn finish(mut self) -> String {
        self.close();
        self.out
    }
}

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();

/// The bundled grammars, loaded on first use (a few ms; their regexes are
/// compiled as they are first needed).
fn syntaxes() -> &'static SyntaxSet {
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Extensions the bundled grammars do not list, or list for another
/// language than the one a phone's files most likely are in.
const BY_EXTENSION: &[(&str, &str)] = &[
    ("h", "C"),
    ("mjs", "JavaScript"),
    ("qml", "JavaScript"),
    ("ts", "JavaScript"),
    ("ini", "Java Properties"),
    ("desktop", "Java Properties"),
    ("service", "Java Properties"),
    ("conf", "Java Properties"),
    ("cfg", "Java Properties"),
    ("toml", "Java Properties"),
];

/// Extensions a bundled grammar claims that are mostly something else
/// (meson.build is not NAnt, `.s` is assembly, not R).
const NOT_BY_EXTENSION: &[&str] = &["build", "inc", "s"];

/// The grammar for a file: by its whole name (`Makefile`, `.bashrc`), its
/// extension, then its first line (`#!/bin/sh`). `None` for plain text.
pub fn syntax_for(name: &[u8], first_line: &str) -> Option<&'static SyntaxReference> {
    let set = syntaxes();
    let name = String::from_utf8_lossy(name);
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    let by_name = set.find_syntax_by_extension(&name).or_else(|| {
        let ext = ext.as_deref()?;
        match BY_EXTENSION.iter().find(|(e, _)| *e == ext) {
            Some((_, syntax)) => set.find_syntax_by_name(syntax),
            None if NOT_BY_EXTENSION.contains(&ext) => None,
            None => set.find_syntax_by_extension(ext),
        }
    });
    by_name
        .or_else(|| set.find_syntax_by_first_line(first_line))
        .filter(|s| s.scope != set.find_syntax_plain_text().scope)
}

/// Colours `text` for the file `name` within `limits`; plain when no
/// grammar fits. Blocking: run it off the GUI and runtime threads.
pub fn colour(name: &[u8], text: String, limits: &Limits) -> ColouredText {
    colour_until(name, text, limits.max_line_bytes, Instant::now() + limits.budget)
}

/// [`colour`] with the time running out at `deadline`.
fn colour_until(name: &[u8], text: String, max_line_bytes: usize, deadline: Instant) -> ColouredText {
    let first_line = text.lines().next().unwrap_or_default();
    let Some(syntax) = syntax_for(name, first_line) else {
        return ColouredText::plain(text);
    };
    colour_with(text, syntax, max_line_bytes, || Instant::now() >= deadline)
}

/// Colours `text` with `syntax` line by line until a line is longer than
/// `max_line_bytes`, the grammar fails, or `out_of_time()` (asked before
/// each line) says so.
fn colour_with(
    text: String,
    syntax: &'static SyntaxReference,
    max_line_bytes: usize,
    mut out_of_time: impl FnMut() -> bool,
) -> ColouredText {
    let mut doc = ColouredText::plain(text);
    let mut parser = LineParser::new(syntax);
    let mut spans = Vec::new();
    let mut span_starts = Vec::new();
    for line in doc.text.split('\n') {
        if line.len() > max_line_bytes || out_of_time() {
            break;
        }
        let start = spans.len();
        if !parser.parse(line, &mut spans) {
            spans.truncate(start);
            break;
        }
        span_starts.push(start);
    }
    doc.syntax = Some(syntax.name.as_str());
    doc.spans = spans;
    doc.span_starts = span_starts;
    doc
}

/// The parser's state from line to line.
struct LineParser {
    state: ParseState,
    stack: ScopeStack,
    classes: Classes,
    buffer: String,
}

impl LineParser {
    fn new(syntax: &SyntaxReference) -> LineParser {
        LineParser {
            state: ParseState::new(syntax),
            stack: ScopeStack::new(),
            classes: Classes::new(),
            buffer: String::new(),
        }
    }

    /// Appends the spans of `line`; false when the grammar fails on it.
    fn parse(&mut self, line: &str, spans: &mut Vec<Span>) -> bool {
        // The bundled grammars are written for lines that end in a break.
        self.buffer.clear();
        self.buffer.push_str(line);
        self.buffer.push('\n');
        let Ok(ops) = self.state.parse_line(&self.buffer, syntaxes()) else {
            return false;
        };
        let first = spans.len();
        for (range, op) in ScopeRangeIterator::new(&ops, &self.buffer) {
            if self.stack.apply(op).is_err() {
                return false;
            }
            let end = range.end.min(line.len());
            if range.start >= end {
                continue;
            }
            let class = self.classes.of(&self.stack);
            let end = u32::try_from(end).unwrap_or(u32::MAX);
            let merges = spans.len() > first && spans.last().is_some_and(|last| last.class == class);
            match spans.last_mut() {
                Some(last) if merges => last.end = end,
                _ => spans.push(Span { end, class }),
            }
        }
        true
    }
}

/// [`RULES`] as scopes, and the class of each scope seen so far.
struct Classes {
    rules: Vec<(Scope, Class)>,
    seen: HashMap<Scope, Option<Class>>,
}

impl Classes {
    fn new() -> Classes {
        Classes {
            rules: RULES
                .iter()
                .filter_map(|(prefix, class)| Some((Scope::new(prefix).ok()?, *class)))
                .collect(),
            seen: HashMap::new(),
        }
    }

    /// The class of the innermost scope that has one.
    fn of(&mut self, stack: &ScopeStack) -> Class {
        let Classes { rules, seen } = self;
        stack
            .as_slice()
            .iter()
            .rev()
            .find_map(|scope| {
                *seen.entry(*scope).or_insert_with(|| {
                    rules
                        .iter()
                        .find(|(prefix, _)| prefix.is_prefix_of(*scope))
                        .map(|(_, c)| *c)
                })
            })
            .unwrap_or(Class::Plain)
    }
}

/// A colouring at work on the blocking pool.
#[derive(Debug)]
pub struct ColourJob {
    handle: JoinHandle<ColouredText>,
    deadline: Instant,
}

/// Starts colouring `text` of the file `name` within [`LIMITS`]; call
/// from the runtime.
pub fn start(name: Vec<u8>, text: String) -> ColourJob {
    start_with(name, text, LIMITS, GRACE)
}

fn start_with(name: Vec<u8>, text: String, limits: Limits, grace: Duration) -> ColourJob {
    // One clock for both: loading the grammars counts against the budget.
    let out_of_time = Instant::now() + limits.budget;
    ColourJob {
        deadline: out_of_time + grace,
        handle: tokio::task::spawn_blocking(move || {
            colour_until(&name, text, limits.max_line_bytes, out_of_time)
        }),
    }
}

impl ColourJob {
    /// The colours when they come within `at_most`, else the job back.
    pub async fn wait(mut self, at_most: Duration) -> Result<Option<ColouredText>, ColourJob> {
        match tokio::time::timeout(at_most, &mut self.handle).await {
            Ok(done) => Ok(done.ok()),
            Err(_) => Err(self),
        }
    }

    /// The colours, `None` when the colouring failed or overran its time
    /// (on a line that took too long, the text stays plain).
    pub async fn finish(self) -> Option<ColouredText> {
        let deadline = tokio::time::Instant::from_std(self.deadline);
        match tokio::time::timeout_at(deadline, self.handle).await {
            Ok(Ok(doc)) => Some(doc),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
