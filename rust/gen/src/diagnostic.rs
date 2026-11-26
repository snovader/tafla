use std::fmt::Write;

use crate::lexer::{BYTE_ORDER_MARK, garbles_text};

/// Tabs are rendered as this many spaces, as rustc renders them, so that the
/// carets line up under the text whatever a terminal's tab stops are
const TAB: &str = "    ";

/// A byte range in the schema source
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Whether a [`Diagnostic`] fails its schema
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Severity {
    /// The schema can't be compiled, or its tables would be misread
    #[default]
    Error,
    /// Something worth knowing that fails nothing, which only
    /// [`compatibility()`](crate::compatibility) reports
    Note,
}

impl Severity {
    /// The word that a rendered diagnostic starts with
    const fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Note => "note",
        }
    }
}

/// An error in a schema, or a note about it, at the span that caused it
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub help: Option<String>,
}

impl Diagnostic {
    /// An error
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span,
            help: None,
        }
    }

    pub fn note(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Note,
            ..Self::new(message, span)
        }
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Renders errors in a schema one after another, with a blank line
    /// between them, as compilers print them
    pub fn render_all(diagnostics: &[Self], file_name: &str, source: &str) -> String {
        let rendered: Vec<String> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.render(file_name, source))
            .collect();
        rendered.join("\n")
    }

    /// Renders the error or note with the line it points at, as compilers print them
    pub fn render(&self, file_name: &str, source: &str) -> String {
        // A byte-order mark at the start isn't part of the first line, which
        // it would shift the columns of, as the lexer skips it
        let skipped = if source.starts_with(BYTE_ORDER_MARK) {
            BYTE_ORDER_MARK.len_utf8()
        } else {
            0
        };
        let source = &source[skipped..];
        let start = self.span.start.saturating_sub(skipped);
        let end = self.span.end.saturating_sub(skipped).max(start);

        let (line, column) = line_and_column(source, start);
        let text = source.lines().nth(line - 1).unwrap_or("");
        let indent = columns(text.chars().take(column - 1));
        let width = columns(source[start..end].chars()).max(1);
        let gutter = " ".repeat(line.to_string().len());

        let mut rendered = String::new();
        let _ = writeln!(rendered, "{}: {}", self.severity.label(), self.message);
        let _ = writeln!(rendered, "{gutter}--> {file_name}:{line}:{column}");
        let _ = writeln!(rendered, "{gutter} |");
        let _ = writeln!(rendered, "{line} | {}", displayed(text));
        let _ = write!(
            rendered,
            "{gutter} | {}{}",
            " ".repeat(indent),
            "^".repeat(width)
        );
        if let Some(help) = &self.help {
            let _ = write!(rendered, " {help}");
        }
        rendered.push('\n');

        rendered
    }
}

/// A line as it's printed: with tabs as spaces, and replacement characters
/// for the characters that would garble the terminal, such as a lone carriage
/// return, and for a byte-order mark, which takes no column, so that the
/// carets line up and point at something
fn displayed(text: &str) -> String {
    let mut displayed = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\t' => displayed.push_str(TAB),
            character if character == BYTE_ORDER_MARK || garbles_text(character) => {
                displayed.push(char::REPLACEMENT_CHARACTER);
            }
            character => displayed.push(character),
        }
    }
    displayed
}

/// How many columns characters take in the rendered line
fn columns(characters: impl Iterator<Item = char>) -> usize {
    characters
        .map(|character| if character == '\t' { TAB.len() } else { 1 })
        .sum()
}

/// The 1-based line and column, in characters, of a byte offset
fn line_and_column(source: &str, offset: usize) -> (usize, usize) {
    let before = &source[..offset.min(source.len())];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = before[line_start..].chars().count() + 1;

    (line, column)
}
