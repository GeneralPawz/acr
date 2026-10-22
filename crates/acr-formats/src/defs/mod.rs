//! Game object definitions: units and buildings (`.MD`), nations and tech trees (`.NDS`,
//! `NATIONS.LST`), sounds (`SOUNDLIST.TXT`) and map pieces (`.SMP`, `.SMP.LNK`).
//!
//! The text formats share one lexical structure, mirroring the original `sscanf`-based
//! readers: tokens are separated by ASCII whitespace (line breaks included, so argument
//! lists may continue on the next lines) and a token starting with `/` comments out the
//! rest of its line; `.MD` commands also skip whatever follows their arguments on the
//! same line. See `docs/formats/definitions.md` for the grammars.

pub mod lnk;
pub mod md;
pub mod nds;
pub mod nres;
pub mod orders;
pub mod smp;
pub mod sounds;
pub mod sprites;

pub use lnk::PieceLinks;
pub use md::{AnimFrames, Animation, Command, CommandKind, SpriteSet, UnitDef};
pub use nds::{NationDef, NationEntry, NationList, NdsFile, Section, Upgrade};
pub use orders::{OrderGroups, OrderList, OrderShape};
pub use smp::Piece;
pub use sounds::SoundList;
pub use sprites::SpriteResources;

use std::fmt;

use crate::text::CodePage;

/// A problem found while parsing; parsing continues after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// 1-based line, or 0 for binary formats.
    pub line: u32,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// One argument of a text command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i32),
    Word(String),
}

impl Value {
    pub fn as_int(&self) -> Option<i32> {
        match self {
            Value::Int(v) => Some(*v),
            Value::Word(_) => None,
        }
    }

    pub fn as_word(&self) -> Option<&str> {
        match self {
            Value::Word(w) => Some(w),
            Value::Int(_) => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Word(w) => f.write_str(w),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token<'a> {
    pub bytes: &'a [u8],
    pub line: u32,
}

impl Token<'_> {
    /// Parses a decimal integer the way `%d` does for well-formed input.
    pub fn int(&self) -> Option<i32> {
        std::str::from_utf8(self.bytes).ok()?.parse().ok()
    }

    pub fn is(&self, s: &str) -> bool {
        self.bytes == s.as_bytes()
    }

    pub fn text(&self, cp: CodePage) -> String {
        cp.decode(self.bytes)
    }
}

/// Splits text into whitespace-separated tokens, skipping `/` comments.
#[derive(Debug, Clone)]
pub(crate) struct Tokenizer<'a> {
    data: &'a [u8],
    pos: usize,
    line: u32,
}

fn is_space(b: u8) -> bool {
    // NUL and other control bytes appear as padding at the end of a few files.
    b <= b' '
}

impl<'a> Tokenizer<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, line: 1 }
    }

    /// Line of the next unread byte.
    pub fn line(&self) -> u32 {
        self.line
    }

    fn skip_space(&mut self) {
        while let Some(&b) = self.data.get(self.pos) {
            if !is_space(b) {
                break;
            }
            if b == b'\n' {
                self.line += 1;
            }
            self.pos += 1;
        }
    }

    fn skip_line(&mut self) {
        while let Some(&b) = self.data.get(self.pos) {
            if b == b'\n' {
                break;
            }
            self.pos += 1;
        }
    }

    pub fn next(&mut self) -> Option<Token<'a>> {
        loop {
            self.skip_space();
            let start = self.pos;
            if start >= self.data.len() {
                return None;
            }
            if self.data[start] == b'/' {
                self.skip_line();
                continue;
            }
            while self.data.get(self.pos).is_some_and(|&b| !is_space(b)) {
                self.pos += 1;
            }
            return Some(Token { bytes: &self.data[start..self.pos], line: self.line });
        }
    }

    pub fn peek(&self) -> Option<Token<'a>> {
        self.clone().next()
    }

    /// Returns the remaining tokens of the current line.
    pub fn rest_of_line(&mut self) -> Vec<Token<'a>> {
        let line = self.line;
        let mut out = Vec::new();
        while let Some(t) = self.peek() {
            if t.line != line {
                break;
            }
            out.push(t);
            self.next();
        }
        out
    }
}

/// Reads a plain list of names, one token each (`NMLIST.TXT`, `ICONLIST.TXT`).
pub fn name_list(data: &[u8]) -> Vec<String> {
    let mut tk = Tokenizer::new(data);
    std::iter::from_fn(|| tk.next()).map(|t| t.text(CodePage::Cp1252)).collect()
}

/// Splits a file into lines of tokens, dropping comments and empty lines.
pub(crate) fn token_lines(data: &[u8]) -> Vec<Vec<Token<'_>>> {
    let mut lines: Vec<Vec<Token>> = Vec::new();
    let mut tk = Tokenizer::new(data);
    while let Some(t) = tk.next() {
        match lines.last_mut() {
            Some(l) if l[0].line == t.line => l.push(t),
            _ => lines.push(vec![t]),
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_skips_comments_and_counts_lines() {
        let mut tk = Tokenizer::new(b"A 1 //x y\r\n/13 0 0\n\tB\x00 -2\n");
        let toks: Vec<_> = std::iter::from_fn(|| tk.next()).map(|t| (t.bytes, t.line)).collect();
        assert_eq!(toks, [(&b"A"[..], 1), (b"1", 1), (b"B", 3), (b"-2", 3)]);
    }

    #[test]
    fn rest_of_line_stops_at_newline() {
        let mut tk = Tokenizer::new(b"K a b\nL c");
        tk.next();
        assert_eq!(tk.rest_of_line().len(), 2);
        assert!(tk.next().unwrap().is("L"));
    }
}
