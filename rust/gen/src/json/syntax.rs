//! JSON's syntax: the tokens of a text, each with its span, for a parser
//! that knows what it expects from a schema, and the strings of a text that's
//! written. It knows nothing of tables.
//!
//! The text isn't read into a tree first, so that an array of a million
//! numbers takes the memory of its elements rather than of a node each.

use std::fmt::Write;

use crate::lexer::garbles_text;
use crate::{Diagnostic, Span};

/// A token of a JSON text. A number is its span of the text, which its
/// reader parses as the type it expects, so that no digit is lost.
#[derive(Debug, PartialEq, Eq)]
pub enum Token {
    BeginObject,
    EndObject,
    BeginArray,
    EndArray,
    Colon,
    Comma,
    String(String),
    Number,
    True,
    False,
    Null,
    /// The end of the text
    End,
}

impl Token {
    /// What the token starts, as an error names it
    pub const fn describe(&self) -> &'static str {
        match self {
            Self::BeginObject => "an object",
            Self::BeginArray => "an array",
            Self::String(_) => "a string",
            Self::Number => "a number",
            Self::True | Self::False => "a bool",
            Self::Null => "null",
            Self::EndObject => "`}`",
            Self::EndArray => "`]`",
            Self::Colon => "`:`",
            Self::Comma => "`,`",
            Self::End => "the end of the text",
        }
    }
}

/// Reads a JSON text token by token
pub struct Tokens<'a> {
    source: &'a str,
    position: usize,
}

impl<'a> Tokens<'a> {
    pub const fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
        }
    }

    /// The text of a span, such as a number's
    pub fn text(&self, span: Span) -> &'a str {
        &self.source[span.start..span.end]
    }

    /// The next token and its span
    ///
    /// # Errors
    ///
    /// A diagnostic at a character that starts no token, or within a string
    /// or a number that isn't JSON's.
    pub fn next(&mut self) -> Result<(Token, Span), Diagnostic> {
        self.skip_whitespace();

        let start = self.position;
        let Some(character) = self.source[start..].chars().next() else {
            return Ok((Token::End, self.span_from(start)));
        };

        let token = match character {
            '{' => self.punctuation(Token::BeginObject),
            '}' => self.punctuation(Token::EndObject),
            '[' => self.punctuation(Token::BeginArray),
            ']' => self.punctuation(Token::EndArray),
            ':' => self.punctuation(Token::Colon),
            ',' => self.punctuation(Token::Comma),
            '"' => Token::String(self.string()?),
            '-' | '0'..='9' => self.number()?,
            't' => self.word("true", Token::True)?,
            'f' => self.word("false", Token::False)?,
            'n' => self.word("null", Token::Null)?,
            _ => {
                let span = Span {
                    start,
                    end: start + character.len_utf8(),
                };
                return Err(Diagnostic::new("this isn't JSON", span)
                    .with_help("expected a value or punctuation"));
            }
        };

        Ok((token, self.span_from(start)))
    }

    /// The next token without reading it
    ///
    /// # Errors
    ///
    /// As [`Tokens::next()`].
    pub fn peek(&mut self) -> Result<(Token, Span), Diagnostic> {
        let position = self.position;
        let next = self.next();
        self.position = position;
        next
    }

    /// Reads the token that must follow, such as a member's colon
    ///
    /// # Errors
    ///
    /// A diagnostic at any other token.
    pub fn expect(&mut self, expected: &Token) -> Result<Span, Diagnostic> {
        let (token, span) = self.next()?;
        if token == *expected {
            Ok(span)
        } else {
            Err(unexpected(&token, span, expected.describe()))
        }
    }

    /// After an object's `{` or one of its members: the next member's key
    /// and the key's span, past its colon, or `None` and the span of the `}`
    /// that ends the object. `first` is whether no member came before.
    ///
    /// # Errors
    ///
    /// A diagnostic at a token that isn't the object's next.
    pub fn member(&mut self, first: bool) -> Result<(Option<String>, Span), Diagnostic> {
        let (mut token, mut span) = self.next()?;
        if token == Token::EndObject {
            return Ok((None, span));
        }
        if !first {
            if token != Token::Comma {
                return Err(unexpected(&token, span, "`,` or `}`"));
            }
            (token, span) = self.next()?;
        }

        let Token::String(key) = token else {
            return Err(unexpected(&token, span, "a field's name, a string"));
        };
        self.expect(&Token::Colon)?;
        Ok((Some(key), span))
    }

    /// After an array's `[` or one of its elements: whether an element
    /// follows, past the comma before it, or the `]` that ends the array is
    /// read. `first` is whether no element came before.
    ///
    /// # Errors
    ///
    /// A diagnostic at a token that isn't the array's next.
    pub fn element(&mut self, first: bool) -> Result<bool, Diagnostic> {
        let (token, span) = self.peek()?;
        if token == Token::EndArray {
            self.next()?;
            return Ok(false);
        }
        if first {
            return Ok(true);
        }

        if token != Token::Comma {
            return Err(unexpected(&token, span, "`,` or `]`"));
        }
        self.next()?;
        Ok(true)
    }

    /// Reads past the value that starts with `token`, which was just read,
    /// without a call per level, so that no nesting overflows the stack. It
    /// counts brackets rather than checking what lies between them, since
    /// its caller already has an error of the value.
    ///
    /// # Errors
    ///
    /// A diagnostic at a token that's malformed, or at the end of the text
    /// within the value.
    pub fn skip(&mut self, token: &Token) -> Result<(), Diagnostic> {
        let mut depth = usize::from(matches!(token, Token::BeginObject | Token::BeginArray));

        while depth > 0 {
            let (token, span) = self.next()?;
            match token {
                Token::BeginObject | Token::BeginArray => depth += 1,
                Token::EndObject | Token::EndArray => depth -= 1,
                Token::End => return Err(unexpected(&token, span, "`]` or `}`")),
                _ => {}
            }
        }
        Ok(())
    }

    fn skip_whitespace(&mut self) {
        let rest = &self.source[self.position..];
        let trimmed = rest.trim_start_matches([' ', '\t', '\n', '\r']);
        self.position += rest.len() - trimmed.len();
    }

    const fn span_from(&self, start: usize) -> Span {
        Span {
            start,
            end: self.position,
        }
    }

    const fn punctuation(&mut self, token: Token) -> Token {
        self.position += 1;
        token
    }

    fn word(&mut self, word: &str, token: Token) -> Result<Token, Diagnostic> {
        let start = self.position;
        let rest = &self.source[start..];
        let length = rest
            .find(|character: char| !character.is_ascii_alphanumeric())
            .unwrap_or(rest.len());

        if &rest[..length] != word {
            let span = Span {
                start,
                end: start + length,
            };
            return Err(Diagnostic::new("this isn't JSON", span)
                .with_help(format!("did you mean `{word}`?")));
        }
        self.position += length;
        Ok(token)
    }

    /// A number as JSON writes it: `-`? digits, without a leading zero, a
    /// fraction and an exponent
    fn number(&mut self) -> Result<Token, Diagnostic> {
        let start = self.position;
        let bytes = self.source.as_bytes();
        let mut end = start;
        let digits = |mut end: usize| {
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            end
        };
        let malformed = |end: usize, help: &str| {
            let span = Span {
                start,
                end: end.max(start + 1),
            };
            Diagnostic::new("this isn't a JSON number", span).with_help(help)
        };

        if bytes[end] == b'-' {
            end += 1;
        }
        let integer = digits(end);
        if integer == end {
            return Err(malformed(end, "a digit follows the sign"));
        }
        if bytes[end] == b'0' && integer > end + 1 {
            return Err(malformed(integer, "a number has no leading zeros"));
        }
        end = integer;

        if bytes.get(end) == Some(&b'.') {
            let fraction = digits(end + 1);
            if fraction == end + 1 {
                return Err(malformed(fraction, "a digit follows the point"));
            }
            end = fraction;
        }

        if matches!(bytes.get(end), Some(b'e' | b'E')) {
            let mut exponent = end + 1;
            if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
                exponent += 1;
            }
            let exponent_end = digits(exponent);
            if exponent_end == exponent {
                return Err(malformed(exponent_end, "a digit follows the exponent"));
            }
            end = exponent_end;
        }

        self.position = end;
        Ok(Token::Number)
    }

    /// A string's characters, with its escapes read
    fn string(&mut self) -> Result<String, Diagnostic> {
        let start = self.position;
        self.position += 1;
        let mut string = String::new();

        loop {
            let rest = &self.source[self.position..];
            let plain = rest
                .find(|character: char| matches!(character, '"' | '\\') || character < ' ')
                .unwrap_or(rest.len());
            string.push_str(&rest[..plain]);
            self.position += plain;

            match rest[plain..].chars().next() {
                Some('"') => {
                    self.position += 1;
                    return Ok(string);
                }
                Some('\\') => string.push(self.escape()?),
                Some(_) => {
                    let span = Span {
                        start: self.position,
                        end: self.position + 1,
                    };
                    return Err(
                        Diagnostic::new("a string can't hold a control character", span)
                            .with_help("write it as an escape, such as `\\n` or `\\u0000`"),
                    );
                }
                None => {
                    let span = Span {
                        start,
                        end: start + 1,
                    };
                    return Err(
                        Diagnostic::new("the string doesn't end", span).with_help("it starts here")
                    );
                }
            }
        }
    }

    /// The character of the escape at the position, which is a `\`
    fn escape(&mut self) -> Result<char, Diagnostic> {
        let start = self.position;
        let escaped = self.source[start + 1..].chars().next();
        self.position += 1 + escaped.map_or(0, char::len_utf8);

        let character = match escaped {
            Some('"') => '"',
            Some('\\') => '\\',
            Some('/') => '/',
            Some('b') => '\u{8}',
            Some('f') => '\u{c}',
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some('u') => return self.unicode_escape(start),
            _ => {
                return Err(
                    Diagnostic::new("this isn't an escape", self.span_from(start))
                        .with_help("a `\\` in a string is written `\\\\`"),
                );
            }
        };
        Ok(character)
    }

    /// The character of a `\u` escape that starts at `start`, whose `\u` is
    /// read: a code unit of UTF-16, or two for a character outside the Basic
    /// Multilingual Plane
    fn unicode_escape(&mut self, start: usize) -> Result<char, Diagnostic> {
        let lone = |tokens: &Self| {
            Diagnostic::new(
                "a string can't hold half of a surrogate pair",
                tokens.span_from(start),
            )
            .with_help("a table's strings are UTF-8, which has no lone surrogates")
        };

        let unit = self.code_unit(start)?;
        if let Some(character) = char::from_u32(unit) {
            return Ok(character);
        }

        // A high surrogate, which the low one follows
        if !(0xd800..0xdc00).contains(&unit) || !self.source[self.position..].starts_with("\\u") {
            return Err(lone(self));
        }
        self.position += 2;
        let low = self.code_unit(start)?;
        if !(0xdc00..0xe000).contains(&low) {
            return Err(lone(self));
        }

        let code_point = 0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00);
        char::from_u32(code_point).ok_or_else(|| lone(self))
    }

    /// The four hexadecimal digits at the position
    fn code_unit(&mut self, start: usize) -> Result<u32, Diagnostic> {
        let digits = self.source.as_bytes().get(self.position..self.position + 4);
        let unit = digits
            .filter(|digits| digits.iter().all(u8::is_ascii_hexdigit))
            .and_then(|digits| std::str::from_utf8(digits).ok())
            .and_then(|digits| u32::from_str_radix(digits, 16).ok());

        let Some(unit) = unit else {
            return Err(
                Diagnostic::new("this isn't an escape", self.span_from(start))
                    .with_help("`\\u` takes four hexadecimal digits"),
            );
        };
        self.position += 4;
        Ok(unit)
    }
}

/// The error of a token where another was expected
pub fn unexpected(token: &Token, span: Span, expected: &str) -> Diagnostic {
    // The end of the text has no character to point at but its last
    let span = if *token == Token::End {
        Span {
            start: span.start.saturating_sub(1),
            end: span.end,
        }
    } else {
        span
    };
    Diagnostic::new(format!("expected {expected}"), span)
        .with_help(format!("this is {}", token.describe()))
}

/// Appends a string as JSON writes it: in quotes, with escapes of the
/// quote, the backslash and the characters that would garble a terminal or
/// that a parser may drop, such as a control character or a byte-order mark,
/// and of every character outside ASCII when `ascii` is set. A character
/// outside the Basic Multilingual Plane is then a surrogate pair.
pub fn push_string(json: &mut String, string: &str, ascii: bool) {
    json.push('"');
    for character in string.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            character if is_escaped(character, ascii) => {
                let mut units = [0; 2];
                for unit in character.encode_utf16(&mut units) {
                    let _ = write!(json, "\\u{unit:04x}");
                }
            }
            character => json.push(character),
        }
    }
    json.push('"');
}

fn is_escaped(character: char, ascii: bool) -> bool {
    let is_invisible = character == '\u{feff}' || character == '\u{7f}';
    garbles_text(character) || is_invisible || (ascii && !character.is_ascii())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(source: &str) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Tokens::new(source);
        let mut read = Vec::new();
        loop {
            let (token, _) = tokens.next()?;
            if token == Token::End {
                return Ok(read);
            }
            read.push(token);
        }
    }

    #[test]
    fn reads_every_token() {
        let read = tokens(" {\"a\": [1, -2.5e+3, true, false, null]}\r\n\t").unwrap();

        assert_eq!(
            read,
            [
                Token::BeginObject,
                Token::String("a".to_owned()),
                Token::Colon,
                Token::BeginArray,
                Token::Number,
                Token::Comma,
                Token::Number,
                Token::Comma,
                Token::True,
                Token::Comma,
                Token::False,
                Token::Comma,
                Token::Null,
                Token::EndArray,
                Token::EndObject,
            ]
        );
    }

    #[test]
    fn reads_a_strings_escapes() {
        let read = tokens(r#""a\"\\\/\b\f\n\r\t\u00e5\u4e16\ud83e\udd80\ufeff""#).unwrap();

        let expected = "a\"\\/\u{8}\u{c}\n\r\t\u{e5}\u{4e16}\u{1f980}\u{feff}";
        assert_eq!(read, [Token::String(expected.to_owned())]);
    }

    #[test]
    fn rejects_what_json_has_no_token_for() {
        for source in [
            "tru",
            "nulls",
            "'a'",
            "01",
            "-",
            "1.",
            "1e",
            "+1",
            ".5",
            "NaN",
            "\"a",
            "\"\\x\"",
            "\"\\u12\"",
            "\"\\ud83e\"",
            "\"\\ud83e\\u0041\"",
            "\"\\udd80\"",
            "\"a\nb\"",
        ] {
            let error = tokens(source).expect_err(source);

            assert!(error.span.start <= error.span.end && error.span.end <= source.len());
            assert!(error.render("table.json", source).starts_with("error: "));
        }
    }

    #[test]
    fn skips_a_value_of_any_depth() {
        let nested = format!("{}1{} 2", "[{\"a\":".repeat(10_000), "}]".repeat(10_000));
        let mut tokens = Tokens::new(&nested);

        let (token, _) = tokens.next().unwrap();
        tokens.skip(&token).unwrap();

        assert_eq!(tokens.next().unwrap().0, Token::Number);
        assert_eq!(tokens.next().unwrap().0, Token::End);
    }

    #[test]
    fn writes_a_string_with_its_escapes() {
        let string = "a\"\\\n\r\t\u{0}\u{7f}\u{feff}\u{2028}\u{202e}\u{e5}\u{1f980}";

        let mut json = String::new();
        push_string(&mut json, string, false);
        assert_eq!(
            json,
            "\"a\\\"\\\\\\n\\r\\t\\u0000\\u007f\\ufeff\\u2028\\u202e\u{e5}\u{1f980}\""
        );
        assert_eq!(tokens(&json).unwrap(), [Token::String(string.to_owned())]);

        let mut ascii = String::new();
        push_string(&mut ascii, string, true);
        assert!(ascii.is_ascii() && ascii.ends_with("\\u00e5\\ud83e\\udd80\""));
        assert_eq!(tokens(&ascii).unwrap(), [Token::String(string.to_owned())]);
    }
}
