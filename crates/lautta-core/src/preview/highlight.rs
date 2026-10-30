// SPDX-License-Identifier: LGPL-2.1-or-later
//! Syntax colours for the text viewer. Syntect's bundled grammars on its pure-Rust regex engine (no
//! oniguruma).
//!
//! Scopes reduce to a few [`Class`]es; viewer picks the colour from the ambience, none from grammar
//! or theme.
//!
//! Rows are Qt StyledText: every char escaped, whitespace as `&nbsp;`; only tag is `<font color>`
//! with checked `#hex` colour.
//!
//! Parser state carries across lines; coloured off GUI thread within [`Limits`]. Lines after a
//! too-long one, or once time is up, stay plain.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use syntect::easy::ScopeRangeIterator;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};
use tokio::task::JoinHandle;

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

/// First matching prefix decides, so refinements come before shorter prefixes (test checks). No
/// match: enclosing scope decides.
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
    // Shell variables only: other grammars (JavaScript) mark every identifier variable.
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
    // Embedded code is code again.
    ("source", Class::Plain),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Wall time for whole document.
    pub budget: Duration,
    /// Longer line and all after it stay plain.
    pub max_line_bytes: usize,
}

pub const LIMITS: Limits = Limits {
    budget: Duration::from_secs(3),
    max_line_bytes: 4096,
};

/// Wait before showing plain text and colouring on arrival; avoids flash for files coloured at
/// once.
pub const FIRST_WAIT: Duration = Duration::from_millis(250);

/// Colouring overrunning budget is dropped: a line can only be cut between lines.
const GRACE: Duration = Duration::from_millis(500);

/// Columns per tab (TextRows.js).
const TAB_WIDTH: usize = 4;

/// Width of `c` at `column`; UTF-16 units as JavaScript, tab to next stop.
fn columns(column: usize, c: char) -> usize {
    if c == '\t' {
        TAB_WIDTH - column % TAB_WIDTH
    } else {
        c.len_utf16()
    }
}

/// Up to byte `end` of line, from previous span's end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub end: u32,
    pub class: Class,
}

/// Lines split at `\n` as the viewer does, with colours of the first `coloured_lines`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ColouredText {
    text: String,
    line_starts: Vec<usize>,
    syntax: Option<&'static str>,
    spans: Vec<Span>,
    span_starts: Vec<usize>,
}

impl ColouredText {
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

    /// 0: none.
    pub fn coloured_lines(&self) -> usize {
        self.span_starts.len()
    }

    /// `None` for plain text.
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

    pub fn spans(&self, index: usize) -> Option<&[Span]> {
        let start = *self.span_starts.get(index)?;
        let end = self
            .span_starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.spans.len());
        self.spans.get(start..end)
    }

    /// Viewer row `row` of line `index` as StyledText. Row starts at column `start` (UTF-16, tabs
    /// expanded); each char takes palette colour of its span. Mismatched row gets wrong colours at
    /// worst, never other text. Uncoloured or out-of-range lines are escaped all the same.
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

/// `class=#rrggbb` (or `#aarrggbb`), comma-separated. Unknown classes and non-colours ignored: only
/// checked colours reach markup. Plain text has no colour: it is the text's.
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

/// Qt StyledText collapses whitespace runs, drops leading whitespace, reads `&` to next `;` as
/// entity (unknown vanish), stops at NUL. `<` `>` `&` `"` become entities, NUL U+FFFD, whitespace
/// `&nbsp;` except a lone space between visible chars (wrapping breaks there).
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

/// Loaded on first use (few ms).
fn syntaxes() -> &'static SyntaxSet {
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Extensions bundled grammars lack, or list for another language than phone files likely are.
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

/// Bundled claims that are mostly something else: meson.build not NAnt, `.s` assembly not R.
const NOT_BY_EXTENSION: &[&str] = &["build", "inc", "s"];

/// By whole name (`Makefile`, `.bashrc`), extension, then first line (`#!/bin/sh`).
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

/// Blocking: run off GUI and runtime threads.
pub fn colour(name: &[u8], text: String, limits: &Limits) -> ColouredText {
    colour_until(name, text, limits.max_line_bytes, Instant::now() + limits.budget)
}

fn colour_until(name: &[u8], text: String, max_line_bytes: usize, deadline: Instant) -> ColouredText {
    let first_line = text.lines().next().unwrap_or_default();
    let Some(syntax) = syntax_for(name, first_line) else {
        return ColouredText::plain(text);
    };
    colour_with(text, syntax, max_line_bytes, || Instant::now() >= deadline)
}

/// Stops at line longer than `max_line_bytes`, grammar failure, or `out_of_time()` (asked before
/// each line).
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

    /// False when grammar fails on `line`.
    fn parse(&mut self, line: &str, spans: &mut Vec<Span>) -> bool {
        // Bundled grammars expect lines ending in a break.
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

#[derive(Debug)]
pub struct ColourJob {
    handle: JoinHandle<ColouredText>,
    deadline: Instant,
}

/// Call from the runtime.
pub fn start(name: Vec<u8>, text: String) -> ColourJob {
    start_with(name, text, LIMITS, GRACE)
}

fn start_with(name: Vec<u8>, text: String, limits: Limits, grace: Duration) -> ColourJob {
    // One clock for both: grammar loading counts against budget.
    let out_of_time = Instant::now() + limits.budget;
    ColourJob {
        deadline: out_of_time + grace,
        handle: tokio::task::spawn_blocking(move || {
            colour_until(&name, text, limits.max_line_bytes, out_of_time)
        }),
    }
}

impl ColourJob {
    /// Colours if ready within `at_most`, else job back.
    pub async fn wait(mut self, at_most: Duration) -> Result<Option<ColouredText>, ColourJob> {
        match tokio::time::timeout(at_most, &mut self.handle).await {
            Ok(done) => Ok(done.ok()),
            Err(_) => Err(self),
        }
    }

    /// `None` when colouring failed or overran (text stays plain).
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
