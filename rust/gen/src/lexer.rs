use crate::diagnostic::{Diagnostic, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Identifier,
    Integer,
    /// A `///` comment, which documents the declaration after it
    Doc,
    LeftBrace,
    RightBrace,
    LeftParenthesis,
    RightParenthesis,
    LeftBracket,
    RightBracket,
    Colon,
    Semicolon,
    Comma,
    Equals,
    Minus,
    End,
}

#[derive(Clone, Copy, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// The byte-order mark that editors on Windows write at the start of UTF-8
/// files, which a schema may start with
pub const BYTE_ORDER_MARK: char = '\u{FEFF}';

/// Splits a schema into tokens, dropping a byte-order mark at its start,
/// whitespace and `//` comments
pub fn tokenize(source: &str) -> Result<Vec<Token>, Diagnostic> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = if source.starts_with(BYTE_ORDER_MARK) {
        BYTE_ORDER_MARK.len_utf8()
    } else {
        0
    };

    while index < bytes.len() {
        let start = index;
        let byte = bytes[index];

        let kind = match byte {
            b' ' | b'\t' | b'\r' | b'\n' => {
                index += 1;
                continue;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                let is_doc =
                    bytes.get(index + 2) == Some(&b'/') && bytes.get(index + 3) != Some(&b'/');
                index = source[index..]
                    .find('\n')
                    .map_or(bytes.len(), |end| index + end);

                if !is_doc {
                    continue;
                }
                check_doc(source, start, index)?;
                TokenKind::Doc
            }
            b'{' => single(&mut index, TokenKind::LeftBrace),
            b'}' => single(&mut index, TokenKind::RightBrace),
            b'(' => single(&mut index, TokenKind::LeftParenthesis),
            b')' => single(&mut index, TokenKind::RightParenthesis),
            b'[' => single(&mut index, TokenKind::LeftBracket),
            b']' => single(&mut index, TokenKind::RightBracket),
            b':' => single(&mut index, TokenKind::Colon),
            b';' => single(&mut index, TokenKind::Semicolon),
            b',' => single(&mut index, TokenKind::Comma),
            b'=' => single(&mut index, TokenKind::Equals),
            b'-' => single(&mut index, TokenKind::Minus),
            b'0'..=b'9' => {
                index += count_while(&bytes[index..], u8::is_ascii_digit);
                TokenKind::Integer
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                index += count_while(&bytes[index..], |byte| {
                    byte.is_ascii_alphanumeric() || *byte == b'_'
                });
                TokenKind::Identifier
            }
            _ => {
                let character = source[index..].chars().next().unwrap_or_default();
                return Err(Diagnostic::new(
                    format!("unexpected character {}", describe(character)),
                    character_span(start, character),
                ));
            }
        };

        tokens.push(Token {
            kind,
            span: Span { start, end: index },
        });
    }

    let end = Span {
        start: bytes.len(),
        end: bytes.len(),
    };
    tokens.push(Token {
        kind: TokenKind::End,
        span: end,
    });

    Ok(tokens)
}

/// Fails on a character in the doc comment `source[start..end]` that the
/// generated code couldn't carry in its comments and docstrings: a control
/// character other than a tab, such as a lone carriage return, which rustc
/// rejects in doc comments, or a null, which Python rejects in source; a line
/// or paragraph separator, which ends a line in JavaScript; or a control of
/// the direction of text, which rustc rejects in comments, since it can make
/// code read as other code. The carriage return of a line that ends with
/// `\r\n` ends the comment, as a newline does.
fn check_doc(source: &str, start: usize, end: usize) -> Result<(), Diagnostic> {
    let text = &source[start..end];
    let text = text.strip_suffix('\r').unwrap_or(text);

    let Some((offset, character)) = text
        .char_indices()
        .find(|&(_, character)| garbles_text(character))
    else {
        return Ok(());
    };
    Err(Diagnostic::new(
        format!("a doc comment can't hold {}", describe(character)),
        character_span(start + offset, character),
    )
    .with_help("the generated code would carry it in a comment, which it would end or garble"))
}

/// Whether a character ends a line or garbles text where it's printed, as in
/// a comment: a control character other than a tab, a line or paragraph
/// separator, or a control of the direction of text
pub fn garbles_text(character: char) -> bool {
    let is_control = character.is_control() && character != '\t';
    let is_separator = matches!(character, '\u{2028}' | '\u{2029}');
    let is_direction = matches!(character, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
    is_control || is_separator || is_direction
}

/// A character in an error: in backticks if it's visible, or as its code
/// point if it isn't, such as a control character or a byte-order mark,
/// which Rust's `Debug` escapes
fn describe(character: char) -> String {
    let is_visible =
        matches!(character, '\\' | '\'' | '"') || character.escape_debug().nth(1).is_none();
    if is_visible {
        format!("`{character}`")
    } else {
        format!("U+{:04X}", u32::from(character))
    }
}

const fn character_span(start: usize, character: char) -> Span {
    Span {
        start,
        end: start + character.len_utf8(),
    }
}

fn single(index: &mut usize, kind: TokenKind) -> TokenKind {
    *index += 1;
    kind
}

fn count_while(bytes: &[u8], predicate: impl Fn(&u8) -> bool) -> usize {
    bytes.iter().take_while(|byte| predicate(byte)).count()
}
