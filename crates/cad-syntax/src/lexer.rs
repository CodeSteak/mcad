//! Tokenizer. Comments and whitespace are skipped; every token keeps its byte span.

use crate::{Diagnostic, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Number(f64),
    Str(String),
    Ident(String),
    /// `<path>` after `include` / `use`.
    Path(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Semi,
    Comma,
    Colon,
    Dot,
    Question,
    Assign,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Not,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Hash,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

pub fn lex(src: &str, diags: &mut Vec<Diagnostic>) -> Vec<Token> {
    let b = src.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    // After `include`/`use`, a `<...>` is a path.
    let mut expect_path = false;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            let start = i;
            i += 2;
            loop {
                if i + 1 >= b.len() {
                    diags.push(Diagnostic::error(Span::new(start, b.len()), "Kommentar nicht geschlossen"));
                    i = b.len();
                    break;
                }
                if b[i] == b'*' && b[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }
        let start = i;
        if expect_path && c == b'<' {
            i += 1;
            while i < b.len() && b[i] != b'>' && b[i] != b'\n' {
                i += 1;
            }
            let path = src[start + 1..i].to_string();
            if i < b.len() && b[i] == b'>' {
                i += 1;
            } else {
                diags.push(Diagnostic::error(Span::new(start, i), "Pfad nicht geschlossen, `>` erwartet"));
            }
            out.push(Token { tok: Tok::Path(path), span: Span::new(start, i) });
            expect_path = false;
            continue;
        }
        expect_path = false;
        if c.is_ascii_digit() || (c == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i < b.len() && b[i] == b'.' && !(i + 1 < b.len() && b[i + 1] == b'.') {
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                let mut j = i + 1;
                if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                    j += 1;
                }
                if j < b.len() && b[j].is_ascii_digit() {
                    i = j;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            // As in OpenSCAD (longest match), a word starting with digits is an
            // identifier when it is longer than the number: `2d`, `3x`, `0x1F`.
            let mut w = start;
            while w < b.len() && (b[w].is_ascii_alphanumeric() || b[w] == b'_') {
                w += 1;
            }
            if c != b'.' && w > i {
                out.push(Token { tok: Tok::Ident(src[start..w].to_string()), span: Span::new(start, w) });
                i = w;
                continue;
            }
            let text = &src[start..i];
            let v = text.parse::<f64>().unwrap_or_else(|_| {
                diags.push(Diagnostic::error(Span::new(start, i), "ungültige Zahl"));
                0.0
            });
            out.push(Token { tok: Tok::Number(v), span: Span::new(start, i) });
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = &src[start..i];
            if word == "include" || word == "use" {
                expect_path = true;
            }
            out.push(Token { tok: Tok::Ident(word.to_string()), span: Span::new(start, i) });
            continue;
        }
        if c == b'"' {
            i += 1;
            let mut s = String::new();
            let mut closed = false;
            while i < b.len() {
                let ch = src[i..].chars().next().unwrap();
                if ch == '"' {
                    i += 1;
                    closed = true;
                    break;
                }
                if ch == '\\' && i + 1 < b.len() {
                    let e = b[i + 1];
                    i += 2;
                    match e {
                        b'n' => s.push('\n'),
                        b't' => s.push('\t'),
                        b'r' => s.push('\r'),
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'x' if i + 2 <= b.len() => {
                            if let Ok(v) = u8::from_str_radix(&src[i..i + 2], 16) {
                                s.push(v as char);
                            }
                            i += 2;
                        }
                        b'u' if i + 4 <= b.len() => {
                            if let Some(ch) =
                                u32::from_str_radix(&src[i..i + 4], 16).ok().and_then(char::from_u32)
                            {
                                s.push(ch);
                            }
                            i += 4;
                        }
                        _ => s.push(e as char),
                    }
                    continue;
                }
                s.push(ch);
                i += ch.len_utf8();
            }
            if !closed {
                diags.push(Diagnostic::error(Span::new(start, i), "String nicht geschlossen"));
            }
            out.push(Token { tok: Tok::Str(s), span: Span::new(start, i) });
            continue;
        }
        let two = if i + 1 < b.len() { Some((c, b[i + 1])) } else { None };
        let (tok, len) = match two {
            Some((b'=', b'=')) => (Tok::Eq, 2),
            Some((b'!', b'=')) => (Tok::Ne, 2),
            Some((b'<', b'=')) => (Tok::Le, 2),
            Some((b'>', b'=')) => (Tok::Ge, 2),
            Some((b'&', b'&')) => (Tok::AndAnd, 2),
            Some((b'|', b'|')) => (Tok::OrOr, 2),
            _ => match c {
                b'(' => (Tok::LParen, 1),
                b')' => (Tok::RParen, 1),
                b'[' => (Tok::LBracket, 1),
                b']' => (Tok::RBracket, 1),
                b'{' => (Tok::LBrace, 1),
                b'}' => (Tok::RBrace, 1),
                b';' => (Tok::Semi, 1),
                b',' => (Tok::Comma, 1),
                b':' => (Tok::Colon, 1),
                b'.' => (Tok::Dot, 1),
                b'?' => (Tok::Question, 1),
                b'=' => (Tok::Assign, 1),
                b'<' => (Tok::Lt, 1),
                b'>' => (Tok::Gt, 1),
                b'!' => (Tok::Not, 1),
                b'+' => (Tok::Plus, 1),
                b'-' => (Tok::Minus, 1),
                b'*' => (Tok::Star, 1),
                b'/' => (Tok::Slash, 1),
                b'%' => (Tok::Percent, 1),
                b'^' => (Tok::Caret, 1),
                b'#' => (Tok::Hash, 1),
                _ => {
                    let ch = src[i..].chars().next().unwrap();
                    diags.push(Diagnostic::error(
                        Span::new(i, i + ch.len_utf8()),
                        format!("unerwartetes Zeichen `{ch}`"),
                    ));
                    i += ch.len_utf8();
                    continue;
                }
            },
        };
        i += len;
        out.push(Token { tok, span: Span::new(start, i) });
    }
    out.push(Token { tok: Tok::Eof, span: Span::new(b.len(), b.len()) });
    out
}
