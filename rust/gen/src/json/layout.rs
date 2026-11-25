//! The annotated layout of a table's bytes, which `tafla-gen decode
//! --layout` prints: each header slot, offset, count, value and run of
//! padding, with its position, its length, its bytes and the path of its
//! field, in the order of the bytes.

use std::fmt::Write;

/// The bytes of a table that one read took, as the walk that reads its
/// values records them
pub struct Part {
    pub start: usize,
    pub length: usize,
    /// The path of the field that the part belongs to, empty for the
    /// root's header
    pub path: String,
    /// What the bytes hold, such as `offset 16` or `float64 1.5`
    pub description: String,
}

/// The most bytes of a part that a line shows
const SHOWN_BYTES: usize = 8;

/// The most bytes of padding that writers write, before an array of 8-byte
/// elements whose count is a byte
const MAX_PADDING: usize = 7;

/// The layout of `bytes`, from the parts read of them: a line of each part
/// in the order of their positions, and of each run of bytes that no part
/// read: padding when it's at most 7 zeros before a part, as writers pad,
/// and unread otherwise. Parts that overlap, as corrupted offsets lead to,
/// are each shown.
pub fn render(mut parts: Vec<Part>, bytes: &[u8]) -> String {
    parts.sort_by_key(|part| part.start);

    let mut lines = Vec::with_capacity(parts.len() * 2);
    let mut covered = 0;
    for part in parts {
        if part.start > covered && covered < bytes.len() {
            let before_part = part.start <= bytes.len();
            lines.push(gap(
                covered,
                part.start.min(bytes.len()),
                bytes,
                before_part,
            ));
        }
        covered = covered.max(part.start.saturating_add(part.length));
        lines.push(part);
    }
    if bytes.len() > covered {
        lines.push(gap(covered, bytes.len(), bytes, false));
    }

    let digits = bytes.len().max(1).to_string().len().max("position".len());
    // Each byte and a space, and `…` after them
    let bytes_width = SHOWN_BYTES * 3 + 1;
    let path_width = lines
        .iter()
        .map(|line| line.path.chars().count())
        .max()
        .unwrap_or(0)
        .max("field".len());

    let mut text = String::new();
    let _ = writeln!(
        text,
        "{:>digits$}  {:>digits$}  {:bytes_width$}  {:path_width$}  value",
        "position", "length", "bytes", "field"
    );
    for line in &lines {
        let shown = shown_bytes(bytes, line.start, line.length);
        let row = format!(
            "{:>digits$}  {:>digits$}  {shown:bytes_width$}  {:path_width$}  {}",
            line.start, line.length, line.path, line.description
        );
        text.push_str(row.trim_end());
        text.push('\n');
    }
    text
}

/// A run of bytes that no part read, which is `before_part` or at the end
fn gap(start: usize, end: usize, bytes: &[u8], before_part: bool) -> Part {
    let is_padding = before_part
        && end - start <= MAX_PADDING
        && bytes[start..end].iter().all(|&byte| byte == 0);

    Part {
        start,
        length: end - start,
        path: String::new(),
        description: if is_padding { "padding" } else { "unread" }.to_owned(),
    }
}

/// The first bytes of a part in hexadecimal, and `…` after them when there
/// are more, of those that lie within the bytes
fn shown_bytes(bytes: &[u8], start: usize, length: usize) -> String {
    let within = bytes.get(start..).unwrap_or_default();
    let within = &within[..length.min(within.len())];

    let mut shown = String::new();
    for byte in within.iter().take(SHOWN_BYTES) {
        let _ = write!(shown, "{byte:02x} ");
    }
    if within.len() > SHOWN_BYTES {
        shown.push('…');
    }
    shown.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(start: usize, length: usize, path: &str, description: &str) -> Part {
        Part {
            start,
            length,
            path: path.to_owned(),
            description: description.to_owned(),
        }
    }

    #[test]
    fn shows_the_parts_and_the_bytes_between_them_in_order() {
        let bytes = [4, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 7];
        let parts = vec![
            part(8, 9, "values", "9 uint8s"),
            part(0, 4, "values", "offset 4"),
            part(4, 4, "values", "count 9"),
        ];

        let layout = render(parts, &bytes);

        assert_eq!(
            layout,
            "\
position    length  bytes                      field   value
       0         4  04 00 00 00                values  offset 4
       4         4  00 00 00 00                values  count 9
       8         9  01 02 03 04 05 06 07 08 …  values  9 uint8s
      17         1  07                                 unread
"
        );
    }

    #[test]
    fn shows_padding_and_parts_past_the_bytes() {
        let bytes = [9, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let parts = vec![
            part(0, 1, "x", "offset 9"),
            part(3, 1, "x", "uint8 1"),
            part(12, 1, "x", "uint8 0"),
            part(20, 4, "x", "past"),
        ];

        let layout = render(parts, &bytes);

        let lines: Vec<&str> = layout.lines().collect();
        assert_eq!(
            lines[2..],
            [
                "       1         2  00 00                             padding",
                "       3         1  01                         x      uint8 1",
                "       4         8  00 00 00 00 00 00 00 00           unread",
                "      12         1  00                         x      uint8 0",
                "      13         4  00 00 00 00                       unread",
                "      20         4                             x      past",
            ]
        );
    }
}
