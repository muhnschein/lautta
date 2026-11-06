// SPDX-License-Identifier: LGPL-2.1-or-later
//! Text viewer rows. One Text item of a megabyte is slow to lay out and draw: viewer shows a row
//! per line, and a line over [`ROW_LENGTH`] columns goes on over further rows. Columns are UTF-16
//! units as QML counts them, tabs expanded.

use super::highlight::columns;
use std::ops::Range;

pub const ROW_LENGTH: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row<'a> {
    /// Line number from 1 on a line's first row, 0 on the rows it goes on over.
    pub number: usize,
    /// Tabs expanded to spaces.
    pub text: &'a str,
    /// Line from 0.
    pub line: usize,
    /// Column of `text` in its line.
    pub start: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Span {
    line: usize,
    start: usize,
    bytes: Range<usize>,
}

/// Rows of a text split at `\n`; empty text is one empty line. Default: no text, no rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextRows {
    shown: String,
    rows: Vec<Span>,
    lines: usize,
}

impl TextRows {
    pub fn new(text: &str) -> TextRows {
        let mut out = TextRows {
            shown: String::with_capacity(text.len()),
            ..TextRows::default()
        };
        for (line, raw) in text.split('\n').enumerate() {
            out.push_line(line, raw);
            out.lines += 1;
        }
        out
    }

    fn push_line(&mut self, line: usize, raw: &str) {
        let begin = self.shown.len();
        let mut column = 0;
        for c in raw.chars() {
            let width = columns(column, c);
            if c == '\t' {
                self.shown.extend(std::iter::repeat_n(' ', width));
            } else {
                self.shown.push(c);
            }
            column += width;
        }
        let mut row = Span {
            line,
            start: 0,
            bytes: begin..begin,
        };
        let mut column = 0;
        for (at, c) in self.shown[begin..].char_indices() {
            // A char past the row's end (a surrogate pair across it too) starts the next row.
            if column + c.len_utf16() - row.start > ROW_LENGTH {
                let cut = begin + at;
                row.bytes.end = cut;
                let next = Span {
                    line,
                    start: column,
                    bytes: cut..cut,
                };
                self.rows.push(std::mem::replace(&mut row, next));
            }
            column += c.len_utf16();
        }
        row.bytes.end = self.shown.len();
        self.rows.push(row);
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn lines(&self) -> usize {
        self.lines
    }

    pub fn row(&self, index: usize) -> Option<Row<'_>> {
        let span = self.rows.get(index)?;
        Some(Row {
            number: if span.start == 0 { span.line + 1 } else { 0 },
            text: self.shown.get(span.bytes.clone())?,
            line: span.line,
            start: span.start,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(rows: &TextRows) -> String {
        (0..rows.len())
            .filter_map(|i| rows.row(i))
            .map(|r| format!("{}:{}", r.number, r.text))
            .collect::<Vec<_>>()
            .join("|")
    }

    fn lengths(rows: &TextRows) -> Vec<usize> {
        (0..rows.len())
            .filter_map(|i| rows.row(i))
            .map(|r| r.text.encode_utf16().count())
            .collect()
    }

    #[test]
    fn lines() {
        let two = TextRows::new("first\nsecond");
        assert_eq!((shown(&two).as_str(), two.lines()), ("1:first|2:second", 2));
        let blank = TextRows::new("");
        assert_eq!(
            (shown(&blank).as_str(), blank.lines()),
            ("1:", 1),
            "empty text: one empty line"
        );
        assert_eq!(TextRows::new("a\n\nb").len(), 3, "an empty line is a row");
        assert!(!two.is_empty());
        let none = TextRows::default();
        assert!(none.is_empty() && none.lines() == 0 && none.row(0).is_none());
        assert_eq!(two.row(1).map(|r| (r.line, r.start)), Some((1, 0)));
        assert!(two.row(2).is_none());
    }

    #[test]
    fn tabs() {
        for (raw, want) in [
            ("\tx", "    x"),
            ("ab\tc", "ab  c"),
            ("abcd\te", "abcd    e"),
            ("a\t\tb", "a       b"),
            ("abc\t", "abc "),
            ("no tabs", "no tabs"),
            ("\u{1F600}\tx", "\u{1F600}  x"),
        ] {
            assert_eq!(TextRows::new(raw).row(0).map(|r| r.text), Some(want), "{raw:?}");
        }
    }

    #[test]
    fn long_lines() {
        assert_eq!(lengths(&TextRows::new(&"a".repeat(1000))), [1000]);
        assert_eq!(lengths(&TextRows::new(&"a".repeat(1001))), [1000, 1]);
        let long = TextRows::new(&format!("x\n{}\ny", "c".repeat(2500)));
        let numbers: Vec<_> = (0..long.len())
            .filter_map(|i| long.row(i))
            .map(|r| r.number)
            .collect();
        assert_eq!(numbers, [1, 2, 0, 0, 3]);
        assert_eq!(lengths(&long), [1, 1000, 1000, 500, 1]);
        assert_eq!(long.lines(), 3);
        let starts: Vec<_> = (1..4)
            .filter_map(|i| long.row(i))
            .map(|r| (r.line, r.start))
            .collect();
        assert_eq!(starts, [(1, 0), (1, 1000), (1, 2000)]);
        let joined: String = (1..4).filter_map(|i| long.row(i)).map(|r| r.text).collect();
        assert_eq!(joined, "c".repeat(2500), "nothing is lost");
    }

    #[test]
    fn pair_not_cut() {
        let emoji = TextRows::new(&format!("{}\u{1F600}e", "d".repeat(999)));
        assert_eq!(lengths(&emoji), [999, 3]);
        assert_eq!(emoji.row(1).map(|r| (r.text, r.start)), Some(("\u{1F600}e", 999)));
        let fits = TextRows::new(&format!("{}\u{1F600}e", "d".repeat(998)));
        assert_eq!(lengths(&fits), [1000, 1]);
    }

    #[test]
    fn tab_across_row_end_is_cut_as_spaces() {
        // Pair pushed to row 2 moves its end off a tab stop.
        let rows = TextRows::new(&format!("{}\u{1F600}{}\tx", "d".repeat(999), "d".repeat(997)));
        assert_eq!(lengths(&rows), [999, 1000, 2]);
        assert_eq!(rows.row(2).map(|r| (r.text, r.start)), Some((" x", 1999)));
    }
}
