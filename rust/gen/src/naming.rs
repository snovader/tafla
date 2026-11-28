//! The cases that targets write names in, which the checker checks too, so
//! that names distinct in a schema stay distinct in each target

/// A `snake_case` name in `camelCase`, as TypeScript, JavaScript and Java
/// write fields. It drops the underscores, so names that differ only in an
/// underscore before a digit are the same: `a_1` and `a1` are both `a1`.
pub fn camel_case(snake: &str) -> String {
    let pascal = pascal_case(snake);
    let mut characters = pascal.chars();
    characters
        .next()
        .map(|first| first.to_ascii_lowercase().to_string() + characters.as_str())
        .unwrap_or_default()
}

/// A `snake_case` name in `PascalCase`, as Go exports names and C# names
/// members. It drops the underscores, as [`camel_case()`] does.
pub fn pascal_case(snake: &str) -> String {
    snake
        .split('_')
        .map(|word| {
            let mut characters = word.chars();
            characters
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + characters.as_str())
                .unwrap_or_default()
        })
        .collect()
}
