// SPDX-License-Identifier: LGPL-2.1-or-later

use super::highlight::{self, ColouredText, Palette};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};
use std::collections::HashSet;
use std::time::Instant;

const SAFE_SCHEMES: [&str; 4] = ["http", "https", "mailto", "ftp"];

const PRE: &str = "<pre style=\"white-space: pre-wrap; margin: 0px; ";

const MONO: &str = "font-family: 'JetBrains Mono'";

const MAX_PX: u32 = 400;

const DEFAULT_GAP: u32 = 12;

pub const BLOCK_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    heading: Option<String>,
    accent: Option<String>,
    secondary: Option<String>,
    tint: Option<String>,
    rule: Option<String>,
    gap: u32,
    code_px: Option<u32>,
    syntax: Palette,
}

impl Default for Style {
    fn default() -> Style {
        Style {
            heading: None,
            accent: None,
            secondary: None,
            tint: None,
            rule: None,
            gap: DEFAULT_GAP,
            code_px: None,
            syntax: Palette::default(),
        }
    }
}

impl Style {
    pub fn parse(spec: &str) -> Style {
        let mut style = Style {
            syntax: Palette::parse(spec),
            ..Style::default()
        };
        for (key, value) in spec.split(',').filter_map(|item| item.split_once('=')) {
            let value = value.trim();
            let colour = highlight::is_hex_colour(value).then(|| value.to_owned());
            let px = value.parse::<u32>().ok().filter(|n| *n <= MAX_PX);
            match key.trim() {
                "md-heading" => style.heading = colour,
                "md-accent" => style.accent = colour,
                "md-secondary" => style.secondary = colour,
                "md-tint" => style.tint = colour,
                "md-rule" => style.rule = colour,
                "md-gap" => style.gap = px.unwrap_or(DEFAULT_GAP),
                "md-code" => style.code_px = px,
                _ => {}
            }
        }
        style
    }
}

pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    push_escaped(&mut out, text);
    out
}

fn push_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\0' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
}

fn safe_href(url: &str) -> bool {
    let trimmed = url.trim();
    let scheme_end = trimmed.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')));
    match scheme_end {
        Some(i) if trimmed[i..].starts_with(':') => {
            let scheme = trimmed[..i].to_ascii_lowercase();
            SAFE_SCHEMES.contains(&scheme.as_str())
        }
        _ => true,
    }
}

fn heading_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn align_attr(a: Alignment) -> &'static str {
    match a {
        Alignment::Left => " align=\"left\"",
        Alignment::Center => " align=\"center\"",
        Alignment::Right => " align=\"right\"",
        Alignment::None => "",
    }
}

fn color(c: &Option<String>) -> String {
    c.as_deref().map(|c| format!("color: {c}; ")).unwrap_or_default()
}

fn bgcolor(c: &Option<String>) -> String {
    c.as_deref()
        .map(|c| format!(" bgcolor=\"{c}\""))
        .unwrap_or_default()
}

fn task_lists(events: &[Event<'_>]) -> HashSet<usize> {
    let mut open = Vec::new();
    let mut tasks = HashSet::new();
    for (i, event) in events.iter().enumerate() {
        match event {
            Event::Start(Tag::List(_)) => open.push(i),
            Event::End(Tag::List(_)) => {
                open.pop();
            }
            Event::TaskListMarker(_) => tasks.extend(open.last().copied()),
            _ => {}
        }
    }
    tasks
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum List {
    Bullets,
    Numbers,
    Tasks,
}

struct Code {
    language: String,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Text,
    Heading,
    Quote,
    Code,
}

impl BlockKind {
    pub fn name(self) -> &'static str {
        match self {
            BlockKind::Text => "text",
            BlockKind::Heading => "heading",
            BlockKind::Quote => "quote",
            BlockKind::Code => "code",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub html: String,
    pub kind: BlockKind,
    pub joined: bool,
}

enum Open {
    Quote(String),
    Bullets(String),
    Numbers { next: u64, margins: String },
    Item,
    Table(String),
    Paragraph(String),
}

impl Open {
    fn reopen(&self) -> String {
        match self {
            Open::Quote(html) | Open::Bullets(html) | Open::Table(html) | Open::Paragraph(html) => {
                html.clone()
            }
            Open::Numbers { next, margins } => format!("<ol start=\"{next}\"{margins}>"),
            Open::Item => "<li>".to_owned(),
        }
    }

    fn close(&self) -> &'static str {
        match self {
            Open::Quote(_) => "</blockquote>",
            Open::Bullets(_) => "</ul>",
            Open::Numbers { .. } => "</ol>",
            Open::Item => "</li>",
            Open::Table(_) => "</table>",
            Open::Paragraph(_) => "</p>",
        }
    }
}

struct Renderer<'s> {
    style: &'s Style,
    block_bytes: usize,
    blocks: Vec<Block>,
    out: String,
    kind: BlockKind,
    depth: usize,
    open: Vec<Open>,
    inline: usize,
    deadline: Instant,
    lists: Vec<List>,
    task_line: bool,
    done_span: bool,
    task_paragraphs: u32,
    code: Option<Code>,
    aligns: Vec<Alignment>,
    column: usize,
    in_head: bool,
    body_rows: usize,
    image_alt: Option<String>,
    refused_links: Vec<bool>,
}

impl<'s> Renderer<'s> {
    fn new(style: &'s Style) -> Renderer<'s> {
        Renderer {
            style,
            block_bytes: BLOCK_BYTES,
            blocks: Vec::new(),
            out: String::new(),
            kind: BlockKind::Text,
            depth: 0,
            open: Vec::new(),
            inline: 0,
            deadline: Instant::now() + highlight::LIMITS.budget,
            lists: Vec::new(),
            task_line: false,
            done_span: false,
            task_paragraphs: 0,
            code: None,
            aligns: Vec::new(),
            column: 0,
            in_head: false,
            body_rows: 0,
            image_alt: None,
            refused_links: Vec::new(),
        }
    }

    fn text(&mut self, t: &str) {
        if let Some(code) = &mut self.code {
            code.text.push_str(t);
        } else if let Some(alt) = &mut self.image_alt {
            alt.push_str(t);
        } else {
            for piece in pieces(t, self.block_bytes) {
                push_escaped(&mut self.out, piece);
                if self.inline == 0 && matches!(self.open.last(), Some(Open::Paragraph(_))) {
                    self.split_if_full();
                }
            }
        }
    }

    fn split_if_full(&mut self) {
        if self.out.len() < self.block_bytes || self.task_line || self.image_alt.is_some() {
            return;
        }
        self.end_block(true);
        for open in &self.open {
            self.out.push_str(&open.reopen());
        }
    }

    fn end_block(&mut self, joined: bool) {
        for open in self.open.iter().rev() {
            self.out.push_str(open.close());
        }
        if !self.out.is_empty() {
            self.blocks.push(Block {
                html: std::mem::take(&mut self.out),
                kind: self.kind,
                joined,
            });
        } else if !joined {
            if let Some(last) = self.blocks.last_mut() {
                last.joined = false;
            }
        }
    }

    fn top_level(&self) -> bool {
        self.depth == 1
    }

    fn push_open(&mut self, open: Open) {
        self.out.push_str(&open.reopen());
        self.open.push(open);
    }

    fn pop_open(&mut self) {
        if let Some(open) = self.open.pop() {
            self.out.push_str(open.close());
        }
    }

    fn in_tasks(&self) -> bool {
        self.lists.last() == Some(&List::Tasks)
    }

    fn close_done_span(&mut self) {
        if self.done_span {
            self.out.push_str("</span>");
            self.done_span = false;
        }
    }

    fn close_task_line(&mut self) {
        self.close_done_span();
        if self.task_line {
            self.out.push_str("</p>");
            self.task_line = false;
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.start_paragraph(),
            Tag::Heading(level, _, _) => self.start_heading(level),
            Tag::BlockQuote => self.start_quote(),
            Tag::CodeBlock(kind) => self.start_code(&kind),
            Tag::List(first) => self.start_list(first, false),
            Tag::Item => self.start_item(),
            Tag::Table(aligns) => self.start_table(aligns),
            Tag::TableHead => {
                self.in_head = true;
                self.column = 0;
                self.out.push_str("<tr>");
            }
            Tag::TableRow => self.start_row(),
            Tag::TableCell => self.start_cell(),
            Tag::FootnoteDefinition(_) => self.start_paragraph(),
            inline => self.start_inline(inline),
        }
    }

    fn start_inline(&mut self, tag: Tag<'_>) {
        self.inline += 1;
        match tag {
            Tag::Emphasis => self.out.push_str("<em>"),
            Tag::Strong => self.out.push_str("<strong>"),
            Tag::Strikethrough => self.out.push_str("<s>"),
            Tag::Link(_, url, _) => self.start_link(&url),
            Tag::Image(..) => self.image_alt = Some(String::new()),
            _ => {}
        }
    }

    fn start_paragraph(&mut self) {
        if self.task_line {
            if self.task_paragraphs > 0 {
                self.out.push_str("<br/>");
            }
            self.task_paragraphs += 1;
            return;
        }
        let gap = self.style.gap;
        self.push_open(Open::Paragraph(format!(
            "<p style=\"margin-top: 0px; margin-bottom: {gap}px\">"
        )));
    }

    fn start_heading(&mut self, level: HeadingLevel) {
        self.close_task_line();
        let n = heading_number(level);
        let gap = self.style.gap;
        let top = if self.out.is_empty() { 0 } else { gap };
        let colour = color(&self.style.heading);
        self.out.push_str(&format!(
            "<h{n} style=\"{colour}font-weight: normal; margin-top: {top}px; margin-bottom: {}px\">",
            gap / 2
        ));
    }

    fn start_quote(&mut self) {
        self.close_task_line();
        if self.top_level() {
            return;
        }
        let gap = self.style.gap;
        self.push_open(Open::Quote(format!(
            "<blockquote style=\"{}margin-top: 0px; margin-bottom: {gap}px; margin-left: {gap}px; margin-right: 0px\">",
            color(&self.style.secondary)
        )));
    }

    fn start_code(&mut self, kind: &CodeBlockKind<'_>) {
        self.close_task_line();
        let language = match kind {
            CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or_default().to_owned(),
            CodeBlockKind::Indented => String::new(),
        };
        self.code = Some(Code {
            language,
            text: String::new(),
        });
    }

    fn end_code(&mut self) {
        let Some(Code { language, text }) = self.code.take() else {
            return;
        };
        let text = text.strip_suffix('\n').map(str::to_owned).unwrap_or(text);
        let coloured = highlight::colour_fenced(&language, text, self.deadline);
        let half = self.block_bytes / 2;
        let lines = coloured.line_count();
        let mut body = String::new();
        let mut in_body = 0;
        for index in 0..lines {
            let line = coloured.line(index).unwrap_or_default();
            let last = index + 1 == lines;
            if line.len() < half {
                if in_body > 0 {
                    body.push('\n');
                }
                push_coloured(&mut body, &coloured, index, &self.style.syntax);
                in_body += 1;
                if body.len() >= half && !last {
                    self.code_panel(&std::mem::take(&mut body), false);
                    in_body = 0;
                }
                continue;
            }
            if in_body > 0 {
                self.code_panel(&std::mem::take(&mut body), false);
                in_body = 0;
            }
            self.long_code_line(line, last);
        }
        if in_body > 0 {
            self.code_panel(&body, true);
        }
    }

    fn long_code_line(&mut self, line: &str, last_line: bool) {
        let mut parts = pieces(line, self.block_bytes / 2).peekable();
        while let Some(part) = parts.next() {
            self.code_panel(&escape_html(part), last_line && parts.peek().is_none());
        }
    }

    fn code_panel(&mut self, body: &str, last: bool) {
        if self.top_level() {
            let size = self
                .style
                .code_px
                .map(|px| format!("; font-size: {px}px"))
                .unwrap_or_default();
            self.out.push_str(&format!("{PRE}{MONO}{size}\">{body}</pre>"));
            if !last {
                self.end_block(true);
            }
            return;
        }
        let gap = self.style.gap;
        let bottom = if last { gap } else { 0 };
        let size = self
            .style
            .code_px
            .map(|px| format!("; font-size: {px}px"))
            .unwrap_or_default();
        self.out.push_str(&format!(
            "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"{}\"{} style=\"margin-bottom: {bottom}px\"><tr><td>{PRE}{MONO}{size}\">{body}</pre></td></tr></table>",
            gap / 2,
            bgcolor(&self.style.tint),
        ));
        self.split_if_full();
    }

    fn start_list(&mut self, first: Option<u64>, tasks: bool) {
        self.close_task_line();
        let kind = match first {
            _ if tasks => List::Tasks,
            None => List::Bullets,
            Some(_) => List::Numbers,
        };
        let bottom = if self.lists.is_empty() { self.style.gap } else { 0 };
        let margins = format!(" style=\"margin-top: 0px; margin-bottom: {bottom}px\"");
        match (kind, first) {
            (List::Tasks, _) => {}
            (List::Bullets, _) => self.push_open(Open::Bullets(format!("<ul{margins}>"))),
            (_, Some(n)) if n != 1 => {
                self.out.push_str(&format!("<ol start=\"{n}\"{margins}>"));
                self.open.push(Open::Numbers { next: n, margins });
            }
            _ => {
                self.out.push_str(&format!("<ol{margins}>"));
                self.open.push(Open::Numbers { next: 1, margins });
            }
        }
        self.lists.push(kind);
    }

    fn end_list(&mut self) {
        if self.lists.pop() != Some(List::Tasks) {
            self.pop_open();
        }
    }

    fn start_item(&mut self) {
        if !self.in_tasks() {
            if let Some(Open::Numbers { next, .. }) = self.open.last_mut() {
                *next += 1;
            }
            self.push_open(Open::Item);
            return;
        }
        let gap = self.style.gap;
        let indent = (self.lists.len() - 1) as u32 * gap * 2;
        self.out.push_str(&format!(
            "<p style=\"margin-top: 0px; margin-bottom: {}px; margin-left: {indent}px\">",
            gap / 4
        ));
        self.task_line = true;
        self.task_paragraphs = 0;
    }

    fn end_item(&mut self) {
        if self.in_tasks() {
            self.close_task_line();
        } else {
            self.pop_open();
        }
    }

    fn task_marker(&mut self, done: bool) {
        let glyph = if done { "\u{2611}" } else { "\u{2610}" };
        match &self.style.accent {
            Some(c) => self.out.push_str(&format!("<font color=\"{c}\">{glyph}</font>")),
            None => self.out.push_str(glyph),
        }
        self.out.push_str("&nbsp;&nbsp;");
        if done && !self.done_span {
            self.out
                .push_str(&format!("<span style=\"{}\">", color(&self.style.secondary)));
            self.done_span = true;
        }
    }

    fn start_link(&mut self, url: &str) {
        let ok = safe_href(url);
        self.refused_links.push(!ok);
        if ok {
            self.out.push_str("<a href=\"");
            push_escaped(&mut self.out, url.trim());
            match &self.style.accent {
                Some(accent) => self.out.push_str(&format!("\" style=\"color: {accent}\">")),
                None => self.out.push_str("\">"),
            }
        }
    }

    fn start_table(&mut self, aligns: Vec<Alignment>) {
        self.close_task_line();
        self.aligns = aligns;
        self.body_rows = 0;
        let gap = self.style.gap;
        self.push_open(Open::Table(format!(
            "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"{}\" style=\"margin-bottom: {gap}px\">",
            gap / 4
        )));
    }

    fn rule_row(&mut self, colour: &Option<String>) {
        if colour.is_some() {
            self.out.push_str(&format!(
                "<tr style=\"font-size: 1px\"><td colspan=\"{}\"{}></td></tr>",
                self.aligns.len().max(1),
                bgcolor(colour)
            ));
        }
    }

    fn start_row(&mut self) {
        if self.body_rows > 0 {
            let tint = self.style.tint.clone();
            self.rule_row(&tint);
        }
        self.body_rows += 1;
        self.column = 0;
        self.out.push_str("<tr>");
    }

    fn start_cell(&mut self) {
        let align = self.aligns.get(self.column).copied().unwrap_or(Alignment::None);
        self.column += 1;
        if self.in_head {
            let colour = color(&self.style.accent);
            self.out.push_str(&format!(
                "<th{} style=\"{colour}font-weight: normal\">",
                align_attr(if align == Alignment::None {
                    Alignment::Left
                } else {
                    align
                })
            ));
        } else {
            self.out.push_str(&format!("<td{}>", align_attr(align)));
        }
    }

    fn end(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph | Tag::FootnoteDefinition(_) => self.end_paragraph(),
            Tag::Heading(level, _, _) => self.out.push_str(&format!("</h{}>", heading_number(level))),
            Tag::BlockQuote | Tag::Table(_) => self.pop_open(),
            Tag::CodeBlock(_) => self.end_code(),
            Tag::List(_) => self.end_list(),
            Tag::Item => self.end_item(),
            Tag::TableHead => {
                self.in_head = false;
                self.out.push_str("</tr>");
                let rule = self.style.rule.clone();
                self.rule_row(&rule);
            }
            Tag::TableRow => self.out.push_str("</tr>"),
            Tag::TableCell => {
                self.out.push_str(if self.in_head { "</th>" } else { "</td>" });
                return;
            }
            inline => {
                self.end_inline(inline);
                return;
            }
        }
        self.split_if_full();
    }

    fn end_paragraph(&mut self) {
        if self.task_line {
            self.close_done_span();
        } else {
            self.pop_open();
        }
    }

    fn end_inline(&mut self, tag: Tag<'_>) {
        self.inline = self.inline.saturating_sub(1);
        match tag {
            Tag::Emphasis => self.out.push_str("</em>"),
            Tag::Strong => self.out.push_str("</strong>"),
            Tag::Strikethrough => self.out.push_str("</s>"),
            Tag::Link(..) if self.refused_links.pop() == Some(false) => self.out.push_str("</a>"),
            Tag::Image(..) => self.end_image(),
            _ => {}
        }
    }

    fn end_image(&mut self) {
        let alt = self.image_alt.take().unwrap_or_default();
        let label = if alt.trim().is_empty() {
            "image"
        } else {
            alt.trim()
        };
        self.out
            .push_str(&format!("<em style=\"{}\">[", color(&self.style.secondary)));
        push_escaped(&mut self.out, label);
        self.out.push_str("]</em>");
    }

    fn inline_code(&mut self, c: &str) {
        let tint = self
            .style
            .tint
            .as_deref()
            .map(|t| format!("background-color: {t}; "))
            .unwrap_or_default();
        self.out.push_str(&format!("<code style=\"{tint}{MONO}\">"));
        push_escaped(&mut self.out, c);
        self.out.push_str("</code>");
    }

    fn event(&mut self, event: Event<'_>, task_list: bool) {
        match &event {
            Event::Start(tag) => {
                if self.depth == 0 {
                    self.end_block(false);
                    self.kind = kind_of(tag);
                }
                self.depth += 1;
                self.handle(event, task_list);
            }
            Event::End(_) => {
                self.handle(event, task_list);
                self.depth = self.depth.saturating_sub(1);
                if self.depth == 0 {
                    self.end_block(false);
                    self.kind = BlockKind::Text;
                }
            }
            Event::Rule if self.depth == 0 => {
                self.end_block(false);
                self.handle(event, task_list);
                self.end_block(false);
            }
            _ => self.handle(event, task_list),
        }
    }

    fn handle(&mut self, event: Event<'_>, task_list: bool) {
        match event {
            Event::Start(Tag::List(first)) => self.start_list(first, task_list),
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.text(&t),
            Event::Code(c) => self.inline_code(&c),
            Event::Html(h) => self.text(&h),
            Event::FootnoteReference(name) => self.text(&format!("[{name}]")),
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.out.push_str("<br/>"),
            Event::Rule => {
                self.out.push_str("<hr/>");
                self.split_if_full();
            }
            Event::TaskListMarker(done) => self.task_marker(done),
        }
    }
}

fn push_coloured(out: &mut String, doc: &ColouredText, index: usize, palette: &Palette) {
    let line = doc.line(index).unwrap_or_default();
    let mut from = 0;
    for span in doc.spans(index).unwrap_or_default() {
        let to = (span.end as usize).clamp(from, line.len());
        let piece = line.get(from..to).unwrap_or_default();
        match palette.colour(span.class) {
            Some(c) => {
                out.push_str(&format!("<span style=\"color: {c}\">"));
                push_escaped(out, piece);
                out.push_str("</span>");
            }
            None => push_escaped(out, piece),
        }
        from = to;
    }
    push_escaped(out, line.get(from..).unwrap_or_default());
}

fn kind_of(tag: &Tag<'_>) -> BlockKind {
    match tag {
        Tag::Heading(..) => BlockKind::Heading,
        Tag::BlockQuote => BlockKind::Quote,
        Tag::CodeBlock(_) => BlockKind::Code,
        _ => BlockKind::Text,
    }
}

fn pieces(text: &str, max: usize) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let mut end = rest.len().min(max.max(1));
        while !rest.is_char_boundary(end) {
            end += 1;
        }
        if end < rest.len() {
            let mut half = end / 2;
            while !rest.is_char_boundary(half) {
                half += 1;
            }
            if let Some(space) = rest[half..end].rfind(char::is_whitespace) {
                end = half + space + 1;
            }
        }
        let (piece, tail) = rest.split_at(end);
        rest = tail;
        Some(piece)
    })
}

pub fn render(md: &str, style: &Style) -> Vec<Block> {
    render_in(md, style, BLOCK_BYTES)
}

fn render_in(md: &str, style: &Style, block_bytes: usize) -> Vec<Block> {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let events: Vec<Event<'_>> = Parser::new_ext(md, options).collect();
    let tasks = task_lists(&events);
    let mut r = Renderer::new(style);
    r.block_bytes = block_bytes;
    for (i, event) in events.into_iter().enumerate() {
        r.event(event, tasks.contains(&i));
    }
    r.close_task_line();
    r.open.clear();
    r.end_block(false);
    r.blocks
}

pub fn render_prose(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let piece = pieces(rest, BLOCK_BYTES).next().unwrap_or(rest);
        let cut = match piece.rfind('\n') {
            Some(i) if piece.len() < rest.len() && i > 0 => i,
            _ => piece.len(),
        };
        let mut html = String::with_capacity(cut + 64);
        html.push_str("<p style=\"white-space: pre-wrap; margin: 0px\">");
        push_escaped(&mut html, &rest[..cut]);
        html.push_str("</p>");
        rest = &rest[cut..];
        rest = rest.strip_prefix('\n').unwrap_or(rest);
        blocks.push(Block {
            html,
            kind: BlockKind::Text,
            joined: !rest.is_empty(),
        });
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "md-heading=#111111,md-accent=#222222,md-secondary=#333333,md-tint=#44444444,\
                        md-rule=#555555,md-gap=16,md-code=20,keyword=#ff0000,string=#00ff00";

    fn html(blocks: &[Block]) -> String {
        blocks.iter().map(|b| b.html.as_str()).collect()
    }

    fn plain(md: &str) -> String {
        html(&render(md, &Style::default()))
    }

    fn styled(md: &str) -> String {
        html(&render(md, &Style::parse(SPEC)))
    }

    #[test]
    fn raw_script_is_escaped_not_passed_through() {
        let html = plain("<script>alert(1)</script>\n\nhello <b onclick=\"x\">there</b>");
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<b "), "{html}");
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"), "{html}");
        assert!(html.contains("&lt;b onclick=&quot;x&quot;&gt;"), "{html}");
    }

    #[test]
    fn raw_html_blocks_are_shown_as_text() {
        let html = plain("<div>\n<img src=\"http://evil/x.png\">\n</div>\n");
        assert!(!html.contains("<img") && !html.contains("<div"), "{html}");
        assert!(
            html.contains("&lt;img src=&quot;http://evil/x.png&quot;&gt;"),
            "{html}"
        );
    }

    #[test]
    fn style_takes_checked_values_only() {
        let style = Style::parse(SPEC);
        assert_eq!(style.heading.as_deref(), Some("#111111"));
        assert_eq!(style.accent.as_deref(), Some("#222222"));
        assert_eq!(style.secondary.as_deref(), Some("#333333"));
        assert_eq!(style.tint.as_deref(), Some("#44444444"));
        assert_eq!(style.rule.as_deref(), Some("#555555"));
        assert_eq!((style.gap, style.code_px), (16, Some(20)));
        assert_eq!(style.syntax.colour(highlight::Class::Keyword), Some("#ff0000"));
        let bad = Style::parse(
            "md-heading=red\"><script>,md-tint=#12345,md-gap=-1,md-code=401,md-accent= #abcdef ",
        );
        assert_eq!(bad.heading, None);
        assert_eq!(bad.tint, None);
        assert_eq!(
            bad.accent.as_deref(),
            Some("#abcdef"),
            "spaces around a value are fine"
        );
        assert_eq!((bad.gap, bad.code_px), (DEFAULT_GAP, None));
        assert_eq!(Style::parse("md-code=400").code_px, Some(400));
        assert_eq!(Style::parse(""), Style::default());
    }

    #[test]
    fn headings_take_the_heading_colour_in_normal_weight() {
        let html = styled("# Title\n\ntext\n\n###### Six");
        assert!(
            html.starts_with(
                "<h1 style=\"color: #111111; font-weight: normal; margin-top: 0px; margin-bottom: 8px\">Title</h1>"
            ),
            "a heading block leaves its top gap to the viewer: {html}"
        );
        assert!(
            html.contains("<h6 style=\"color: #111111; font-weight: normal; margin-top: 0px"),
            "{html}"
        );
        assert!(
            styled("- a\n\n  ## h\n").contains("<h2 style=\"color: #111111; font-weight: normal; margin-top: 16px; margin-bottom: 8px\">h</h2>"),
            "a heading inside a list keeps its gap"
        );
        assert!(plain("## a")
            .starts_with("<h2 style=\"font-weight: normal; margin-top: 0px; margin-bottom: 6px\">"));
    }

    #[test]
    fn paragraphs_are_spaced_by_the_gap() {
        assert_eq!(
            styled("one\n\ntwo"),
            "<p style=\"margin-top: 0px; margin-bottom: 16px\">one</p><p style=\"margin-top: 0px; margin-bottom: 16px\">two</p>"
        );
    }

    #[test]
    fn code_asks_for_bundled_face() {
        assert_eq!(MONO, format!("font-family: '{}'", crate::fonts::MONO_FAMILY));
    }

    #[test]
    fn inline_markup_and_code() {
        let html = styled("some *em* and **strong** and `a<b` and ~~gone~~");
        assert!(html.contains("<em>em</em>"));
        assert!(html.contains("<strong>strong</strong>"));
        assert!(
            html.contains(
                "<code style=\"background-color: #44444444; font-family: 'JetBrains Mono'\">a&lt;b</code>"
            ),
            "{html}"
        );
        assert!(html.contains("<s>gone</s>"));
        assert!(plain("`x`").contains("<code style=\"font-family: 'JetBrains Mono'\">x</code>"));
    }

    #[test]
    fn code_blocks_sit_on_a_tinted_panel_and_stay_escaped() {
        let html = styled("```\nfn main() { if a < b && c > d {} }\n```\n");
        assert_eq!(
            html,
            "<pre style=\"white-space: pre-wrap; margin: 0px; font-family: 'JetBrains Mono'; font-size: 20px\">\
             fn main() { if a &lt; b &amp;&amp; c &gt; d {} }</pre>",
            "the viewer draws the panel of a top-level block"
        );
        assert_eq!(
            plain("    x\n"),
            "<pre style=\"white-space: pre-wrap; margin: 0px; font-family: 'JetBrains Mono'\">x</pre>"
        );
    }

    #[test]
    fn fenced_code_with_a_known_language_is_coloured() {
        let html = styled("```rust\nfn main() { let s = \"<x>\"; }\n```\n");
        assert!(
            html.contains("<span style=\"color: #ff0000\">fn</span>"),
            "{html}"
        );
        assert!(
            html.contains("<span style=\"color: #00ff00\">&quot;&lt;x&gt;&quot;</span>"),
            "{html}"
        );
        let unknown = styled("```nosuchlang\nfn main() {}\n```\n");
        assert!(!unknown.contains("<span"), "{unknown}");
        assert!(unknown.contains(">fn main() {}</pre>"), "{unknown}");
        let late = render_code_after_deadline("```rust\nfn main() {}\n```\n");
        assert!(!late.contains("<span"), "out of time: plain: {late}");
    }

    fn render_code_after_deadline(md: &str) -> String {
        let style = Style::parse(SPEC);
        let mut r = Renderer::new(&style);
        r.deadline = Instant::now();
        for event in Parser::new(md) {
            r.event(event, false);
        }
        r.out
    }

    #[test]
    fn coloured_lines_keep_their_breaks_and_uncoloured_tail() {
        let html = styled("```sh\nif true; then\n  echo \"hi\"\nfi\n```\n");
        assert!(
            html.contains("<span style=\"color: #ff0000\">if</span> true; <span style=\"color: #ff0000\">then</span>\n  echo <span style=\"color: #00ff00\">&quot;hi&quot;</span>\n<span style=\"color: #ff0000\">fi</span></pre>"),
            "{html}"
        );
    }

    #[test]
    fn code_blocks_wrap_and_keep_their_lines() {
        let long = "x".repeat(300);
        for md in [
            format!("```\n{long}\n  next\n```\n"),
            format!("    {long}\n      next\n"),
        ] {
            let html = plain(&md);
            assert_eq!(html.matches("<pre").count(), 1, "{html}");
            assert!(html.contains(PRE), "a code block may wrap: {html}");
            assert!(html.contains(&format!("{long}\n  next</pre>")), "{html}");
        }
        assert!(plain("> ```\n> a\n> ```").contains(PRE), "quoted code wraps too");
    }

    #[test]
    fn lists_ordered_unordered_and_nested() {
        let html = styled("- a\n- b\n  - c\n\n3. x\n4. y\n");
        assert!(
            html.contains(
                "<ul style=\"margin-top: 0px; margin-bottom: 16px\"><li>a</li><li>b\
                 <ul style=\"margin-top: 0px; margin-bottom: 0px\"><li>c</li></ul></li></ul>"
            ),
            "{html}"
        );
        assert!(
            html.contains(
                "<ol start=\"3\" style=\"margin-top: 0px; margin-bottom: 16px\"><li>x</li><li>y</li></ol>"
            ),
            "{html}"
        );
        assert!(plain("1. one\n")
            .contains("<ol style=\"margin-top: 0px; margin-bottom: 12px\"><li>one</li></ol>"));
    }

    #[test]
    fn task_lists_are_lines_with_check_boxes() {
        let html = styled("- [x] done\n- [ ] todo\n\nafter");
        assert_eq!(
            html,
            "<p style=\"margin-top: 0px; margin-bottom: 4px; margin-left: 0px\"><font color=\"#222222\">\u{2611}</font>&nbsp;&nbsp;<span style=\"color: #333333; \">done</span></p>\
             <p style=\"margin-top: 0px; margin-bottom: 4px; margin-left: 0px\"><font color=\"#222222\">\u{2610}</font>&nbsp;&nbsp;todo</p>\
             <p style=\"margin-top: 0px; margin-bottom: 16px\">after</p>"
        );
        assert!(
            !html.contains("<ul") && !html.contains("<li"),
            "Qt would draw bullets: {html}"
        );
        assert!(plain("- [ ] x").contains("\u{2610}&nbsp;&nbsp;x</p>"));
    }

    #[test]
    fn loose_and_nested_task_lists_close_their_lines() {
        let html = styled("- [x] one\n\n  more\n- [ ] two\n  - [ ] inner\n  - plain\n");
        assert_eq!(html.matches("<p").count(), html.matches("</p>").count(), "{html}");
        assert_eq!(
            html.matches("<span").count(),
            html.matches("</span>").count(),
            "{html}"
        );
        assert!(
            html.contains("margin-left: 32px\"><font color=\"#222222\">\u{2610}</font>&nbsp;&nbsp;inner"),
            "{html}"
        );
        assert!(
            html.contains("one</span><br/>more</p>"),
            "paragraphs of one task stay on its line: {html}"
        );
        let mixed = styled("- [ ] task\n  ```\n  code\n  ```\n- next\n");
        assert!(
            mixed.contains("task</p><table"),
            "block ends the line first: {mixed}"
        );
        let quoted = styled("- [ ] t\n  > q\n  # h\n  | a |\n  |---|\n  | b |\n");
        assert!(quoted.contains("t</p><blockquote"), "{quoted}");
        assert_eq!(
            quoted.matches("<p").count(),
            quoted.matches("</p>").count(),
            "{quoted}"
        );
    }

    #[test]
    fn each_top_level_element_is_a_block_of_its_kind() {
        let blocks = render(
            "# Head\n\npara\n\n> quoted\n> > inner\n\n```\ncode\n```\n\n- a\n\n---\n\n| t |\n|---|\n| 1 |\n",
            &Style::parse(SPEC),
        );
        let kinds: Vec<_> = blocks.iter().map(|b| (b.kind.name(), b.joined)).collect();
        assert_eq!(
            kinds,
            [
                ("heading", false),
                ("text", false),
                ("quote", false),
                ("code", false),
                ("text", false),
                ("text", false),
                ("text", false)
            ]
        );
        assert_eq!(
            blocks[2].html,
            "<p style=\"margin-top: 0px; margin-bottom: 16px\">quoted</p>\
             <blockquote style=\"color: #333333; margin-top: 0px; margin-bottom: 16px; margin-left: 16px; margin-right: 0px\">\
             <p style=\"margin-top: 0px; margin-bottom: 16px\">inner</p></blockquote>",
            "top-level quote: viewer draws the bar; nested one indents"
        );
        assert_eq!(
            blocks[3].html,
            "<pre style=\"white-space: pre-wrap; margin: 0px; font-family: 'JetBrains Mono'; font-size: 20px\">code</pre>",
            "top-level code: viewer draws the panel"
        );
        assert_eq!(blocks[5].html, "<hr/>");
    }

    #[test]
    fn a_big_quote_or_code_block_comes_in_joined_parts() {
        let quote = format!("> {}", "word ".repeat(3000));
        let blocks = render(&quote, &Style::default());
        assert!(blocks.len() > 1);
        assert!(blocks.iter().all(|b| b.kind == BlockKind::Quote));
        let joins: Vec<_> = blocks.iter().map(|b| b.joined).collect();
        assert_eq!(joins.iter().filter(|j| !**j).count(), 1, "{joins:?}");
        assert!(!joins[joins.len() - 1], "last part ends the quote");
        let code = format!("```\n{}```\nafter", "line of code\n".repeat(2000));
        let blocks = render(&code, &Style::default());
        let (last, parts) = blocks.split_last().unwrap();
        assert_eq!(
            last.html,
            "<p style=\"margin-top: 0px; margin-bottom: 12px\">after</p>"
        );
        assert!(parts.len() > 2);
        let (end, run) = parts.split_last().unwrap();
        assert!(
            run.iter().all(|b| b.kind == BlockKind::Code && b.joined),
            "panels touch"
        );
        assert!(end.kind == BlockKind::Code && !end.joined);
        assert!(run
            .iter()
            .all(|b| b.html.starts_with(PRE) && b.html.ends_with("code</pre>")));
    }

    #[test]
    fn nested_code_keeps_a_tinted_table_panel() {
        let html = styled("- item\n\n  ```\n  x\n  ```\n");
        assert!(
            html.contains("<table width=\"100%\" cellspacing=\"0\" cellpadding=\"8\" bgcolor=\"#44444444\" style=\"margin-bottom: 16px\"><tr><td><pre"),
            "{html}"
        );
    }

    #[test]
    fn rule_and_breaks() {
        let html = plain("---\n\nline one  \nline two\nsoft");
        assert!(html.starts_with("<hr/>"));
        assert!(html.contains("line one<br/>line two soft"), "{html}");
    }

    #[test]
    fn images_become_alt_text_only() {
        let html = styled("![a *cat*](http://tracker.example/pixel.png \"t\") and ![](x.png)");
        assert!(
            !html.contains("<img") && !html.contains("tracker.example"),
            "{html}"
        );
        assert!(
            html.contains("<em style=\"color: #333333; \">[a cat]</em>"),
            "{html}"
        );
        assert!(html.contains("[image]</em>"), "{html}");
    }

    #[test]
    fn links_keep_safe_targets_only() {
        let html = plain("[ok](https://example.org/a?b=1&c=2) [mail](mailto:a@b.c) [rel](docs/x.md)");
        assert!(
            html.contains("<a href=\"https://example.org/a?b=1&amp;c=2\">ok</a>"),
            "{html}"
        );
        assert!(html.contains("<a href=\"mailto:a@b.c\">mail</a>"));
        assert!(html.contains("<a href=\"docs/x.md\">rel</a>"));
        assert!(
            styled("[a](https://e.org)").contains("<a href=\"https://e.org\" style=\"color: #222222\">a</a>")
        );
        for bad in [
            "javascript:alert(1)",
            "data:text/html,x",
            "file:///etc/passwd",
            "JaVaScRiPt:x",
        ] {
            let html = plain(&format!("[click]({bad})"));
            assert!(!html.contains("href") && !html.contains("</a>"), "{bad}: {html}");
            assert!(html.contains("click"), "{bad}: {html}");
        }
    }

    #[test]
    fn link_text_with_quotes_cannot_break_out_of_the_attribute() {
        let html = plain("[x](<http://a/\"onmouseover=\"y>)");
        assert!(!html.contains("\"onmouseover"), "{html}");
        assert!(html.contains("&quot;onmouseover=&quot;"), "{html}");
    }

    #[test]
    fn tables_have_a_header_rule_and_soft_row_rules() {
        let html = styled("| a | b |\n|:--|--:|\n| 1 | 2 |\n| 3 | 4 |\n");
        assert_eq!(
            html,
            "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"4\" style=\"margin-bottom: 16px\">\
             <tr><th align=\"left\" style=\"color: #222222; font-weight: normal\">a</th>\
             <th align=\"right\" style=\"color: #222222; font-weight: normal\">b</th></tr>\
             <tr style=\"font-size: 1px\"><td colspan=\"2\" bgcolor=\"#555555\"></td></tr>\
             <tr><td align=\"left\">1</td><td align=\"right\">2</td></tr>\
             <tr style=\"font-size: 1px\"><td colspan=\"2\" bgcolor=\"#44444444\"></td></tr>\
             <tr><td align=\"left\">3</td><td align=\"right\">4</td></tr></table>"
        );
        let bare = plain("| a |\n|---|\n| 1 |\n| 2 |\n");
        assert!(!bare.contains("font-size: 1px"), "no colours, no rules: {bare}");
        assert!(
            bare.contains("<th align=\"left\" style=\"font-weight: normal\">a</th>"),
            "headers sit left like cells: {bare}"
        );
        let second = styled("| a |\n|---|\n| 1 |\n\n| b |\n|---|\n| 2 |\n");
        assert_eq!(
            second.matches("#44444444").count(),
            0,
            "rows counted per table: {second}"
        );
    }

    #[test]
    fn html_entities_are_decoded_then_escaped() {
        assert_eq!(
            plain("a &lt;b&gt; &amp; c"),
            "<p style=\"margin-top: 0px; margin-bottom: 12px\">a &lt;b&gt; &amp; c</p>"
        );
    }

    #[test]
    fn nul_and_deep_nesting_do_not_panic() {
        assert!(plain("a\0b").contains('\u{FFFD}'));
        let deep = ">".repeat(5000) + " x";
        assert!(plain(&deep).contains('x'));
        let lists = "- ".repeat(2000) + "x";
        assert!(plain(&lists).contains('x'));
        let tasks = "- [ ] ".repeat(500) + "x";
        assert!(plain(&tasks).contains('x'));
        assert_eq!(plain(""), "");
    }

    #[test]
    fn prose_keeps_lines_and_escapes_everything() {
        assert_eq!(
            render_prose("# no heading\n\n  <i>a</i> & \0"),
            [Block {
                html: "<p style=\"white-space: pre-wrap; margin: 0px\"># no heading\n\n  &lt;i&gt;a&lt;/i&gt; &amp; \u{FFFD}</p>"
                    .to_owned(),
                kind: BlockKind::Text,
                joined: false,
            }]
        );
        assert!(render_prose("").is_empty());
    }

    #[test]
    fn long_prose_is_cut_at_line_breaks_into_blocks() {
        let line = "word ".repeat(30);
        let text = vec![line.as_str(); 400].join("\n");
        let blocks = render_prose(&text);
        assert!(blocks.len() > 4, "{}", blocks.len());
        let open = "<p style=\"white-space: pre-wrap; margin: 0px\">";
        let mut back = Vec::new();
        for (i, block) in blocks.iter().enumerate() {
            assert_eq!(block.joined, i + 1 < blocks.len(), "parts join, last ends");
            let block = &block.html;
            assert!(block.len() <= BLOCK_BYTES + open.len() + 4, "{}", block.len());
            let inner = block
                .strip_prefix(open)
                .and_then(|b| b.strip_suffix("</p>"))
                .unwrap();
            assert!(inner.ends_with("word "), "cut at a line break: {inner:?}");
            back.push(inner);
        }
        assert_eq!(back.join("\n"), text, "nothing lost or added");
        let one_line = "é".repeat(BLOCK_BYTES);
        let blocks = render_prose(&one_line);
        assert_eq!(blocks.len(), 2, "a line without breaks is cut too");
        assert!(blocks.iter().all(|b| b.html.len() <= BLOCK_BYTES + 64));
    }

    fn visible(html: &str) -> String {
        let mut out = String::new();
        let mut in_tag = false;
        for c in html.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag => out.push(c),
                _ => {}
            }
        }
        out
    }

    fn balanced(block: &str) {
        for (open, close) in [
            ("<table", "</table>"),
            ("<tr", "</tr>"),
            ("<p ", "</p>"),
            ("<ul", "</ul>"),
            ("<ol", "</ol>"),
            ("<li>", "</li>"),
            ("<pre", "</pre>"),
            ("<span", "</span>"),
            ("<em", "</em>"),
            ("<strong>", "</strong>"),
            ("<a ", "</a>"),
        ] {
            assert_eq!(
                block.matches(open).count(),
                block.matches(close).count(),
                "{open} in {block}"
            );
        }
    }

    fn crazy() -> String {
        let words = "ferry lake island harbour stove tent map battery route camp ";
        let mut md = String::new();
        for i in 0..60 {
            md += &format!(
                "## Section {i}\n\n{} **bold** *it* `c{i}` [l](https://e.org/{i})\n\n",
                words.repeat(6)
            );
            md += &(0..6)
                .map(|j| format!("- [{}] {words}\n", if j % 2 == 0 { 'x' } else { ' ' }))
                .collect::<String>();
            md += "\n";
            md += &(0..8)
                .map(|j| format!("{}- {words}\n", "  ".repeat(j % 4)))
                .collect::<String>();
            md += &format!("\n> {}\n> > nested {words}\n\n", words.repeat(4));
            md += &format!(
                "```rust\n{}```\n\n",
                "fn f(x: u32) -> String { format!(\"{}\", x) }\n".repeat(10)
            );
            md += &format!(
                "| a | b |\n|---|--:|\n{}\n",
                format!("| {words} | {i} |\n").repeat(8)
            );
        }
        md += &format!("{}\n\n", words.repeat(3000));
        md += &format!("```\n{}\n```\n\n", "x".repeat(40_000));
        md += &format!("```sh\n{}```\n\n", "echo \"hi\"\n".repeat(5000));
        md += &format!("> {}\n\n", format!("{words}\n> \n> ").repeat(400));
        md += &(1..=2000).map(|n| format!("{n}. {words}\n")).collect::<String>();
        md += &format!("\n| a |\n|---|\n{}", format!("| {words} |\n").repeat(2000));
        md
    }

    #[test]
    fn a_big_complex_document_comes_in_small_whole_blocks() {
        let md = crazy();
        let style = Style::parse(SPEC);
        let blocks = render(&md, &style);
        assert!(blocks.len() > 100, "{}", blocks.len());
        for block in &blocks {
            let b = &block.html;
            assert!(b.len() <= 3 * BLOCK_BYTES, "{} bytes: {}", b.len(), &b[..200]);
            balanced(b);
            assert!(!visible(b).trim().is_empty(), "no empty block: {b}");
        }
        let whole = render_in(&md, &style, usize::MAX);
        assert!(whole.len() < blocks.len() && whole.iter().all(|b| !b.joined));
        let text = |html: &str| visible(html).replace('\n', "");
        let (split, single) = (text(&html(&blocks)), text(&html(&whole)));
        let at = split
            .chars()
            .zip(single.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let near = |t: &str| t.chars().skip(at.saturating_sub(40)).take(80).collect::<String>();
        assert!(
            split == single,
            "differs at {at}: {:?} vs {:?}",
            near(&split),
            near(&single)
        );
        let numbered = blocks
            .iter()
            .filter(|b| b.html.starts_with("<ol start=\""))
            .count();
        assert!(numbered > 1, "a split numbered list counts on: {numbered}");
    }

    #[test]
    fn a_code_block_comes_in_whole_parts_that_end_with_it() {
        let long = "y".repeat(300);
        for n in 1..60 {
            let lines = "let x = 1;\n".repeat(n);
            for code in [
                lines.clone(),
                format!("a\n{long}\nb\n{lines}"),
                format!("{lines}{long}\n"),
            ] {
                let blocks = render_in(&format!("```\n{code}```\n\nafter\n"), &Style::default(), 128);
                let parts: Vec<&Block> = blocks.iter().filter(|b| b.kind == BlockKind::Code).collect();
                let (last, rest) = parts.split_last().unwrap();
                assert!(
                    rest.iter().all(|b| b.joined) && !last.joined,
                    "{n} lines: {parts:?}"
                );
                assert!(
                    parts.iter().all(|b| !visible(&b.html).is_empty()),
                    "no empty part: {parts:?}"
                );
                let text: String = parts.iter().map(|b| visible(&b.html).replace('\n', "")).collect();
                assert_eq!(text, code.replace('\n', ""), "{n} lines, in order");
            }
        }
    }

    #[test]
    fn nested_code_in_parts_keeps_its_gap_on_the_last_part() {
        let long = "y".repeat(300);
        for n in 1..60 {
            let lines = "let x = 1;\n".repeat(n);
            for code in [lines.clone(), format!("{lines}{long}\n")] {
                let md = format!(
                    "- item\n\n  ```\n{}  ```\n",
                    code.lines().map(|l| format!("  {l}\n")).collect::<String>()
                );
                let html = html(&render_in(&md, &Style::default(), 128));
                let margins: Vec<&str> = html
                    .match_indices("style=\"margin-bottom: ")
                    .map(|(i, _)| &html[i + 22..i + 25])
                    .collect();
                let (last, rest) = margins.split_last().unwrap();
                assert!(
                    *last == "12p" && rest.iter().all(|m| *m == "0px"),
                    "{n} lines: {margins:?} in {html}"
                );
            }
        }
        assert!(
            plain("```\n```\n").contains("Mono'\"></pre>"),
            "an empty code block still shows"
        );
    }

    #[test]
    fn pieces_cut_after_a_space_in_their_second_half_and_never_inside_a_char() {
        let cut = |t, max| pieces(t, max).collect::<Vec<_>>();
        assert_eq!(cut("aaaa bbbbbb", 8), ["aaaa ", "bbbbbb"]);
        assert_eq!(
            cut("a bbbbbbbbb", 8),
            ["a bbbbbb", "bbb"],
            "no space in second half"
        );
        assert_eq!(cut("a\u{e4}\u{20ac}b", 2), ["a\u{e4}", "\u{20ac}", "b"]);
        assert_eq!(cut("\u{20ac}\u{20ac}", 1), ["\u{20ac}", "\u{20ac}"]);
        let finnish = "\u{e4}\u{e4}\u{e4} ".repeat(3000);
        assert_eq!(
            cut(&finnish, BLOCK_BYTES).concat(),
            finnish,
            "long text of two-byte chars cuts cleanly"
        );
    }

    #[test]
    fn a_long_code_line_comes_in_parts_of_half_a_block() {
        let blocks = render_in(
            &format!("```\n{}\n```\n", "y".repeat(300)),
            &Style::default(),
            128,
        );
        let sizes: Vec<usize> = blocks.iter().map(|b| visible(&b.html).len()).collect();
        assert_eq!(sizes, [64, 64, 64, 64, 44]);
    }

    #[test]
    fn rules_are_blocks_of_their_own_at_top_level_only() {
        assert_eq!(render("a\n\n---\n\n***\n", &Style::default()).len(), 3);
        assert_eq!(
            render("- a\n\n  ***\n- b\n", &Style::default()).len(),
            1,
            "a rule in a list stays in it"
        );
    }

    #[test]
    fn a_list_under_the_block_size_stays_whole() {
        assert_eq!(render(&"- item\n".repeat(20), &Style::default()).len(), 1);
    }

    #[test]
    fn a_task_after_a_nested_plain_list_still_makes_a_task_list() {
        let html = styled("- plain\n  - nested\n- [ ] task\n");
        assert!(html.starts_with("<p") && html.contains("plain</p><ul"), "{html}");
    }

    #[test]
    fn a_paragraph_under_the_block_size_stays_whole() {
        let blocks = render(&"word ".repeat(1000), &Style::default());
        assert_eq!(blocks.len(), 1, "{} blocks", blocks.len());
    }

    #[test]
    fn a_split_numbered_list_continues_its_numbers() {
        let md = (1..=300).map(|n| format!("{n}. item\n")).collect::<String>();
        let blocks = render_in(&md, &Style::default(), 1024);
        assert!(blocks.len() > 2);
        let mut expected = 1;
        for block in &blocks {
            if expected > 1 {
                assert!(
                    block.html.starts_with(&format!("<ol start=\"{expected}\"")),
                    "{}",
                    block.html
                );
            }
            expected += block.html.matches("<li>").count();
        }
        assert_eq!(expected, 301);
    }

    #[test]
    fn splits_never_cut_headings_cells_task_lines_or_inline_markup() {
        let long = "word ".repeat(1000);
        for md in [
            format!("# {long}"),
            format!("| a |\n|---|\n| {long} |\n"),
            format!("- [ ] {long}\n"),
            format!("**{}**", long.trim_end()),
            format!("[{long}](https://e.org)"),
            format!("- [ ] a\n\n  {long}\n\n  b\n"),
        ] {
            let blocks = render_in(&md, &Style::default(), 256);
            assert!(
                blocks.iter().any(|b| b.html.contains(long.trim_end())),
                "{md:.20} cut: {blocks:?}"
            );
            for block in &blocks {
                balanced(&block.html);
            }
        }
        let blocks = render_in(&format!("{long}\n\nnext"), &Style::default(), 256);
        assert!(blocks.len() > 4, "plain paragraph text splits");
        for block in &blocks {
            balanced(&block.html);
        }
    }

    #[test]
    fn escape_html_covers_the_dangerous_five() {
        assert_eq!(
            escape_html("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }
}
