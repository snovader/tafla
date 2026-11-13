//! The English of generated documentation, which every target shares

/// A declaration's name in code, after its indefinite article: "a `Point`" or
/// "an `Item`". Rust's documentation links the name instead.
#[cfg(any(feature = "typescript", feature = "python", feature = "java"))]
pub fn article(name: &str) -> String {
    format!("{} `{name}`", indefinite_article(name))
}

/// The indefinite article before a declaration's name in documentation, by
/// how the name sounds: "an" before a vowel, but "a" before a `U` read as
/// "you".
///
/// That's a `U` spelled out, alone or in an acronym such as `URL` or `U8`, and
/// one before another vowel, as in `Uint8Offsets`, before `ni`, as in `Unit`,
/// or before `s`, `t`, `r` or `k` and then a vowel, as in `User` or `Utility`.
/// Any other takes "an", such as the prefix `Un` in `Unaligned`, `Unused` and
/// `Uninitialized`, and `Under`, `Upper` and `Update`. A name of a single
/// letter is read as the letter's name: "an `S`", "an `F`" and "a `T`".
pub fn indefinite_article(name: &str) -> &'static str {
    let mut letters = name.chars();
    let first = letters.next().map(|letter| letter.to_ascii_lowercase());

    // A single letter is read by its name, as "an `S`" is "an ess"
    if let Some(letter) = first
        && letters.as_str().is_empty()
    {
        return if "aefhilmnorsx".contains(letter) {
            "an"
        } else {
            "a"
        };
    }

    match first {
        Some('u') if reads_as_you(letters.as_str()) => "a",
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

/// Whether a leading `U` is read as "you" before the rest of its name
fn reads_as_you(rest: &str) -> bool {
    let mut letters = rest.chars();
    let Some(second) = letters.next() else {
        // A `U` alone is spelled out
        return true;
    };
    if second.is_ascii_uppercase() || second.is_ascii_digit() {
        // So is one in an acronym or before a number
        return true;
    }

    let mut letters = letters.map(|letter| letter.to_ascii_lowercase());
    match (second, letters.next(), letters.next()) {
        (second, _, _) if is_vowel(second) => true,
        // The prefix `Un` before `in` or `im`, as in `Uninitialized`
        ('n', Some('i'), Some('n' | 'm')) => false,
        ('n', Some('i'), _) => true,
        ('s' | 't' | 'r' | 'k', Some(third), _) => is_vowel(third),
        _ => false,
    }
}

const fn is_vowel(letter: char) -> bool {
    matches!(letter, 'a' | 'e' | 'i' | 'o' | 'u')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indefinite_article() {
        let cases = [
            ("Point", "a"),
            ("Array", "an"),
            ("Event", "an"),
            ("Item", "an"),
            ("Order", "an"),
            // U read as "you"
            ("Uint8Offsets", "a"),
            ("Uuid", "a"),
            ("Unit", "a"),
            ("Union", "a"),
            ("Uniform", "a"),
            ("User", "a"),
            ("Usage", "a"),
            ("Utility", "a"),
            ("Uranium", "a"),
            ("Ukulele", "a"),
            ("U", "a"),
            ("URL", "a"),
            ("UInt8", "a"),
            ("U8", "a"),
            // U read otherwise
            ("Unaligned", "an"),
            ("Unordered", "an"),
            ("Unused", "an"),
            ("Unexpected", "an"),
            ("Uninitialized", "an"),
            ("Unimplemented", "an"),
            ("Unsigned", "an"),
            ("Under", "an"),
            ("Upper", "an"),
            ("Update", "an"),
            ("Upend", "an"),
            ("Umbrella", "an"),
            ("Ux", "an"),
            // A single letter, read by its name
            ("S", "an"),
            ("F", "an"),
            ("H", "an"),
            ("A", "an"),
            ("T", "a"),
            ("B", "a"),
            ("Y", "a"),
        ];

        for (name, article) in cases {
            assert_eq!(indefinite_article(name), article, "{name}");
        }
    }

    #[cfg(any(feature = "typescript", feature = "python", feature = "java"))]
    #[test]
    fn test_article() {
        assert_eq!(article("Point"), "a `Point`");
        assert_eq!(article("Item"), "an `Item`");
        assert_eq!(article("Uint8Offsets"), "a `Uint8Offsets`");
        assert_eq!(article("S"), "an `S`");
    }
}
