// SPDX-License-Identifier: LGPL-2.1-or-later
//! Markdown to the Qt rich-text subset (SPEC PRV-4).
//!
//! The output uses only tags Qt's `Text.RichText` understands: `h1`-`h6`,
//! `p`, `em`, `strong`, `s`, `code`, `pre`, `ul`/`ol`/`li`, `blockquote`, `a`,
//! `hr`, `br` and a simple `table`. Nothing from the source is trusted:
//! every text event is escaped, raw HTML (block or inline) is shown as
//! escaped text, images become their alt text (no resource is ever
//! requested), and link targets are limited to a few safe schemes.

use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag};

/// Schemes a rendered link may carry; other targets are shown as plain text.
const SAFE_SCHEMES: [&str; 4] = ["http", "https", "mailto", "ftp"];

/// Escapes text for element content and double-quoted attributes.
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

/// A link target is safe when it has no scheme (relative) or an allowed one.
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

#[derive(Default)]
struct Renderer {
    out: String,
    /// Alignments of the table being rendered and the current column.
    aligns: Vec<Alignment>,
    column: usize,
    in_head: bool,
    /// Alt text being collected for an image, if inside one.
    image_alt: Option<String>,
    /// One flag per open link: its target was refused, so there is no `</a>`.
    refused_links: Vec<bool>,
}

impl Renderer {
    fn text(&mut self, t: &str) {
        match &mut self.image_alt {
            Some(alt) => alt.push_str(t),
            None => push_escaped(&mut self.out, t),
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.out.push_str("<p>"),
            Tag::Heading(level, _, _) => self.out.push_str(&format!("<h{}>", heading_number(level))),
            Tag::BlockQuote => self.out.push_str("<blockquote>"),
            Tag::CodeBlock(_) => self.out.push_str("<pre>"),
            Tag::List(None) => self.out.push_str("<ul>"),
            Tag::List(Some(1)) => self.out.push_str("<ol>"),
            Tag::List(Some(n)) => self.out.push_str(&format!("<ol start=\"{n}\">")),
            Tag::Item => self.out.push_str("<li>"),
            Tag::Emphasis => self.out.push_str("<em>"),
            Tag::Strong => self.out.push_str("<strong>"),
            Tag::Strikethrough => self.out.push_str("<s>"),
            Tag::Link(_, url, _) => self.start_link(&url),
            Tag::Image(..) => self.image_alt = Some(String::new()),
            Tag::Table(aligns) => self.start_table(aligns),
            Tag::TableHead => {
                self.in_head = true;
                self.column = 0;
                self.out.push_str("<tr>");
            }
            Tag::TableRow => {
                self.column = 0;
                self.out.push_str("<tr>");
            }
            Tag::TableCell => self.start_cell(),
            Tag::FootnoteDefinition(_) => self.out.push_str("<p>"),
        }
    }

    fn start_link(&mut self, url: &str) {
        let ok = safe_href(url);
        self.refused_links.push(!ok);
        if ok {
            self.out.push_str("<a href=\"");
            push_escaped(&mut self.out, url.trim());
            self.out.push_str("\">");
        }
    }

    fn start_table(&mut self, aligns: Vec<Alignment>) {
        self.aligns = aligns;
        self.out
            .push_str("<table border=\"1\" cellspacing=\"0\" cellpadding=\"4\">");
    }

    fn start_cell(&mut self) {
        let align = self.aligns.get(self.column).copied().unwrap_or(Alignment::None);
        self.column += 1;
        let tag = if self.in_head { "th" } else { "td" };
        self.out.push_str(&format!("<{tag}{}>", align_attr(align)));
    }

    fn end(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph | Tag::FootnoteDefinition(_) => self.out.push_str("</p>"),
            Tag::Heading(level, _, _) => self.out.push_str(&format!("</h{}>", heading_number(level))),
            Tag::BlockQuote => self.out.push_str("</blockquote>"),
            Tag::CodeBlock(_) => self.out.push_str("</pre>"),
            Tag::List(None) => self.out.push_str("</ul>"),
            Tag::List(Some(_)) => self.out.push_str("</ol>"),
            Tag::Item => self.out.push_str("</li>"),
            Tag::Emphasis => self.out.push_str("</em>"),
            Tag::Strong => self.out.push_str("</strong>"),
            Tag::Strikethrough => self.out.push_str("</s>"),
            Tag::Link(..) => {
                if self.refused_links.pop() == Some(false) {
                    self.out.push_str("</a>");
                }
            }
            Tag::Image(..) => self.end_image(),
            Tag::Table(_) => self.out.push_str("</table>"),
            Tag::TableHead => {
                self.in_head = false;
                self.out.push_str("</tr>");
            }
            Tag::TableRow => self.out.push_str("</tr>"),
            Tag::TableCell => self.out.push_str(if self.in_head { "</th>" } else { "</td>" }),
        }
    }

    /// Images are never loaded: the alt text stands in for them.
    fn end_image(&mut self) {
        let alt = self.image_alt.take().unwrap_or_default();
        let label = if alt.trim().is_empty() {
            "image"
        } else {
            alt.trim()
        };
        self.out.push_str("<em>[");
        push_escaped(&mut self.out, label);
        self.out.push_str("]</em>");
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.text(&t),
            Event::Code(c) => {
                self.out.push_str("<code>");
                push_escaped(&mut self.out, &c);
                self.out.push_str("</code>");
            }
            // Raw HTML is never passed through, it is displayed as text.
            Event::Html(h) => self.text(&h),
            Event::FootnoteReference(name) => self.text(&format!("[{name}]")),
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.out.push_str("<br/>"),
            Event::Rule => self.out.push_str("<hr/>"),
            Event::TaskListMarker(done) => self.text(if done { "[x] " } else { "[ ] " }),
        }
    }
}

/// Renders `md` to rich-text HTML. Total: any input yields a string and no
/// source text ever reaches the output unescaped.
pub fn render(md: &str) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut r = Renderer::default();
    for event in Parser::new_ext(md, options) {
        r.event(event);
    }
    r.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_script_is_escaped_not_passed_through() {
        let html = render("<script>alert(1)</script>\n\nhello <b onclick=\"x\">there</b>");
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<b "), "{html}");
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"), "{html}");
        assert!(html.contains("&lt;b onclick=&quot;x&quot;&gt;"), "{html}");
    }

    #[test]
    fn raw_html_blocks_are_shown_as_text() {
        let html = render("<div>\n<img src=\"http://evil/x.png\">\n</div>\n");
        assert!(!html.contains("<img") && !html.contains("<div"), "{html}");
        assert!(
            html.contains("&lt;img src=&quot;http://evil/x.png&quot;&gt;"),
            "{html}"
        );
    }

    #[test]
    fn headings_emphasis_and_code() {
        let html = render("# Title\n\n###### Six\n\nsome *em* and **strong** and `a<b` and ~~gone~~");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<h6>Six</h6>"));
        assert!(html.contains("<em>em</em>"));
        assert!(html.contains("<strong>strong</strong>"));
        assert!(html.contains("<code>a&lt;b</code>"));
        assert!(html.contains("<s>gone</s>"));
    }

    #[test]
    fn code_blocks_keep_text_escaped() {
        let html = render("```rust\nfn main() { if a < b && c > d {} }\n```\n");
        assert_eq!(
            html,
            "<pre>fn main() { if a &lt; b &amp;&amp; c &gt; d {} }\n</pre>"
        );
    }

    #[test]
    fn lists_ordered_unordered_and_nested() {
        let html = render("- a\n- b\n  - c\n\n3. x\n4. y\n");
        assert!(
            html.contains("<ul><li>a</li><li>b<ul><li>c</li></ul></li></ul>"),
            "{html}"
        );
        assert!(
            html.contains("<ol start=\"3\"><li>x</li><li>y</li></ol>"),
            "{html}"
        );
        assert!(render("1. one\n").contains("<ol><li>one</li></ol>"));
    }

    #[test]
    fn blockquote_rule_and_breaks() {
        let html = render("> quoted\n\n---\n\nline one  \nline two\nsoft");
        assert!(html.contains("<blockquote><p>quoted</p></blockquote>"));
        assert!(html.contains("<hr/>"));
        assert!(html.contains("line one<br/>line two soft"), "{html}");
    }

    #[test]
    fn images_become_alt_text_only() {
        let html = render("![a *cat*](http://tracker.example/pixel.png \"t\") and ![](x.png)");
        assert!(
            !html.contains("<img") && !html.contains("tracker.example"),
            "{html}"
        );
        assert!(html.contains("<em>[a cat]</em>"), "{html}");
        assert!(html.contains("<em>[image]</em>"), "{html}");
    }

    #[test]
    fn links_keep_safe_targets_only() {
        let html = render("[ok](https://example.org/a?b=1&c=2) [mail](mailto:a@b.c) [rel](docs/x.md)");
        assert!(
            html.contains("<a href=\"https://example.org/a?b=1&amp;c=2\">ok</a>"),
            "{html}"
        );
        assert!(html.contains("<a href=\"mailto:a@b.c\">mail</a>"));
        assert!(html.contains("<a href=\"docs/x.md\">rel</a>"));
        for bad in [
            "javascript:alert(1)",
            "data:text/html,x",
            "file:///etc/passwd",
            "JaVaScRiPt:x",
        ] {
            let html = render(&format!("[click]({bad})"));
            assert!(!html.contains("href"), "{bad}: {html}");
            assert!(html.contains("click"), "{bad}: {html}");
        }
    }

    #[test]
    fn link_text_with_quotes_cannot_break_out_of_the_attribute() {
        let html = render("[x](<http://a/\"onmouseover=\"y>)");
        assert!(!html.contains("\"onmouseover"), "{html}");
        assert!(html.contains("&quot;onmouseover=&quot;"), "{html}");
    }

    #[test]
    fn tables_render_with_alignment() {
        let html = render("| a | b |\n|:--|--:|\n| 1 | 2 |\n");
        assert!(html.starts_with("<table border=\"1\""), "{html}");
        assert!(
            html.contains("<tr><th align=\"left\">a</th><th align=\"right\">b</th></tr>"),
            "{html}"
        );
        assert!(
            html.contains("<tr><td align=\"left\">1</td><td align=\"right\">2</td></tr>"),
            "{html}"
        );
        assert!(html.ends_with("</table>"));
    }

    #[test]
    fn task_lists_and_footnote_like_text() {
        let html = render("- [x] done\n- [ ] todo\n");
        assert!(html.contains("[x] done") && html.contains("[ ] todo"), "{html}");
    }

    #[test]
    fn html_entities_are_decoded_then_escaped() {
        assert_eq!(render("a &lt;b&gt; &amp; c"), "<p>a &lt;b&gt; &amp; c</p>");
    }

    #[test]
    fn nul_and_deep_nesting_do_not_panic() {
        assert!(render("a\0b").contains('\u{FFFD}'));
        let deep = ">".repeat(5000) + " x";
        assert!(render(&deep).contains('x'));
        let lists = "- ".repeat(2000) + "x";
        assert!(render(&lists).contains('x'));
        assert_eq!(render(""), "");
    }

    #[test]
    fn escape_html_covers_the_dangerous_five() {
        assert_eq!(
            escape_html("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }
}
