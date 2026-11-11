//! Indented output, and line breaking for calls that's close enough to each
//! language's formatter that the formatter leaves generated code as it is.

pub struct CodeWriter {
    output: String,
    depth: usize,
    indent: &'static str,
}

impl CodeWriter {
    pub const fn new(indent: &'static str) -> Self {
        Self {
            output: String::new(),
            depth: 0,
            indent,
        }
    }

    pub fn line(&mut self, text: impl AsRef<str>) {
        let text = text.as_ref();
        if !text.is_empty() {
            for _ in 0..self.depth {
                self.output.push_str(self.indent);
            }
            self.output.push_str(text);
        }
        self.output.push('\n');
    }

    /// An empty line, unless the last line is empty or opened a block
    pub fn blank(&mut self) {
        self.blanks(1);
    }

    /// Ends the output with `count` empty lines, unless it's empty or the last
    /// line opened a block
    pub fn blanks(&mut self, count: usize) {
        if self.output.is_empty() || self.output.ends_with("{\n") || self.output.ends_with(":\n") {
            return;
        }
        while self.output.ends_with("\n\n") {
            self.output.pop();
        }
        for _ in 0..count {
            self.output.push('\n');
        }
    }

    pub const fn indent(&mut self) {
        self.depth += 1;
    }

    pub const fn dedent(&mut self) {
        self.depth -= 1;
    }

    pub fn open(&mut self, text: impl AsRef<str>) {
        self.line(text);
        self.depth += 1;
    }

    pub fn close(&mut self, text: impl AsRef<str>) {
        self.depth -= 1;
        self.line(text);
    }

    pub fn lines(&mut self, lines: &[String]) {
        for line in lines {
            self.line(line);
        }
    }

    /// The width of the current depth's indentation
    #[cfg(feature = "cpp")]
    pub const fn indentation(&self) -> usize {
        self.depth * self.indent.len()
    }

    /// Whether a line of `text` fits within the style's width at the current
    /// depth
    pub const fn fits(&self, style: &Style, text: &str) -> bool {
        self.depth * self.indent.len() + text.len() <= style.width
    }

    /// Writes `prefix` + `expression` + `suffix`, broken over lines when it
    /// doesn't fit
    pub fn statement(&mut self, style: &Style, prefix: &str, expression: &Expr, suffix: &str) {
        let width = style.width - self.depth * self.indent.len();
        let lines = style.render(prefix, expression, suffix, width);
        self.lines(&lines);
    }

    pub fn finish(mut self) -> String {
        while self.output.ends_with("\n\n") {
            self.output.pop();
        }
        self.output
    }
}

/// An expression that may need breaking over lines
#[derive(Clone, Debug)]
pub enum Expr {
    Text(String),
    Call(String, Vec<Expr>),
    /// An object literal of `key: value` entries, which Biome hugs
    #[cfg(feature = "typescript")]
    Object(Vec<(String, String)>),
}

pub fn text(text: impl Into<String>) -> Expr {
    Expr::Text(text.into())
}

pub fn call(callee: impl Into<String>, arguments: Vec<Expr>) -> Expr {
    Expr::Call(callee.into(), arguments)
}

impl Expr {
    pub fn flat(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Call(callee, arguments) => {
                let arguments: Vec<String> = arguments.iter().map(Self::flat).collect();
                format!("{callee}({})", arguments.join(", "))
            }
            #[cfg(feature = "typescript")]
            Self::Object(entries) => {
                let entries: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| format!("{key}: {value}"))
                    .collect();
                format!("{{ {} }}", entries.join(", "))
            }
        }
    }
}

/// How a language's formatter breaks calls that don't fit
pub struct Style {
    pub width: usize,
    pub indent: &'static str,
    /// Whether broken arguments end with a comma
    pub trailing_comma: bool,
    /// Whether arguments that fit on one line together get that line before
    /// they get one line each, as Black and Ruff do
    pub hug_arguments: bool,
    /// The widest that a call's arguments can be on one line, as rustfmt's
    /// `fn_call_width`
    pub max_arguments_width: Option<usize>,
}

impl Style {
    fn render(&self, prefix: &str, expression: &Expr, suffix: &str, width: usize) -> Vec<String> {
        let flat = expression.flat();
        let arguments_fit = match (expression, self.max_arguments_width) {
            (Expr::Call(callee, _), Some(max)) => flat.len() - callee.len() - 2 <= max,
            _ => true,
        };
        if prefix.len() + flat.len() + suffix.len() <= width && arguments_fit {
            return vec![format!("{prefix}{flat}{suffix}")];
        }

        let inner = width.saturating_sub(self.indent.len());
        match expression {
            Expr::Text(_) => vec![format!("{prefix}{flat}{suffix}")],
            #[cfg(feature = "typescript")]
            Expr::Object(entries) => {
                let mut lines = vec![format!("{prefix}{{")];
                for (index, (key, value)) in entries.iter().enumerate() {
                    let comma = if index + 1 < entries.len() || self.trailing_comma {
                        ","
                    } else {
                        ""
                    };
                    lines.push(format!("{}{key}: {value}{comma}", self.indent));
                }
                lines.push(format!("}}{suffix}"));
                lines
            }
            Expr::Call(callee, arguments) => {
                // Hug a lone object, as Prettier and Biome do
                #[cfg(feature = "typescript")]
                if let [Expr::Object(_)] = arguments.as_slice() {
                    return self.render(
                        &format!("{prefix}{callee}("),
                        &arguments[0],
                        &format!("){suffix}"),
                        width,
                    );
                }

                let mut lines = vec![format!("{prefix}{callee}(")];
                let joined: Vec<String> = arguments.iter().map(Expr::flat).collect();
                let joined = joined.join(", ");

                if self.hug_arguments && (arguments.len() == 1 || joined.len() <= inner) {
                    if arguments.len() == 1 {
                        lines.extend(self.indented(&arguments[0], "", inner));
                    } else {
                        lines.push(format!("{}{joined}", self.indent));
                    }
                } else {
                    for (index, argument) in arguments.iter().enumerate() {
                        let comma = if index + 1 < arguments.len() || self.trailing_comma {
                            ","
                        } else {
                            ""
                        };
                        lines.extend(self.indented(argument, comma, inner));
                    }
                }

                lines.push(format!("){suffix}"));
                lines
            }
        }
    }

    fn indented(&self, expression: &Expr, suffix: &str, width: usize) -> Vec<String> {
        self.render("", expression, suffix, width)
            .into_iter()
            .map(|line| format!("{}{line}", self.indent))
            .collect()
    }
}

/// A `/** ... */` comment, as `JSDoc` and Javadoc write them, on one line when
/// it's one line of text
#[cfg(any(feature = "typescript", feature = "java"))]
pub fn doc_comment(docs: &[String]) -> Vec<String> {
    match docs {
        [] => Vec::new(),
        [line] => vec![format!("/** {} */", comment_text(line))],
        lines => {
            let mut comment = vec!["/**".to_owned()];
            comment.extend(
                lines
                    .iter()
                    .map(|line| format!(" * {}", comment_text(line)).trim_end().to_owned()),
            );
            comment.push(" */".to_owned());
            comment
        }
    }
}

/// A line of a schema's documentation with its text outside Markdown's code
/// spans escaped, and its code spans as they are. A code span starts at
/// every other backtick, and ends at the next or at the end of the line, as
/// Java's `{@code ...}` does.
#[cfg(any(feature = "typescript", feature = "java", feature = "cpp"))]
pub fn escape_outside_code_spans(line: &str, escape: impl Fn(&str) -> String) -> String {
    let mut escaped = String::with_capacity(line.len());
    for (index, part) in line.split('`').enumerate() {
        if index > 0 {
            escaped.push('`');
        }
        if index % 2 == 0 {
            escaped.push_str(&escape(part));
        } else {
            escaped.push_str(part);
        }
    }
    escaped
}

/// Text for a `/* ... */` comment, with `*/` written as an HTML entity, which
/// `JSDoc` and Javadoc render as the characters, so that it doesn't end the
/// comment
#[cfg(any(feature = "typescript", feature = "java", feature = "cpp"))]
pub fn comment_text(text: &str) -> String {
    text.replace("*/", "*&#47;")
}
