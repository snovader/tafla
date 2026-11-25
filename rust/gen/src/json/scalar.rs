//! A scalar's value, and how JSON writes it, which README.md's "Tables as
//! JSON" describes: the part of the mapping that could lose a value, were it
//! left to JSON's numbers.

use std::fmt::Write;

use super::syntax::Token;
use crate::model::Scalar;

/// The bits of the NaN that `"NaN"` names: the quiet NaN without a payload,
/// which JavaScript writes
const FLOAT32_NAN: u32 = 0x7fc0_0000;
const FLOAT64_NAN: u64 = 0x7ff8_0000_0000_0000;

/// The value of a scalar field, of a struct's scalar or of an array's element
#[derive(Clone, Copy, Debug)]
pub enum ScalarValue {
    Bool(bool),
    Int8(i8),
    Uint8(u8),
    Int16(i16),
    Uint16(u16),
    Int32(i32),
    Uint32(u32),
    Int64(i64),
    Uint64(u64),
    Float32(f32),
    Float64(f64),
}

/// The chunk of `N` bytes that a scalar's bytes start with, which hold it
fn chunk<const N: usize>(bytes: &[u8]) -> [u8; N] {
    *bytes.first_chunk().expect("the bytes hold the scalar")
}

impl ScalarValue {
    /// The scalar that `bytes` start with, little-endian, as a struct holds
    /// it. A bool is true when its byte isn't 0.
    ///
    /// # Panics
    ///
    /// If the bytes are shorter than the scalar.
    pub fn from_le_bytes(scalar: Scalar, bytes: &[u8]) -> Self {
        match scalar {
            Scalar::Bool => Self::Bool(bytes[0] != 0),
            Scalar::Int8 => Self::Int8(i8::from_le_bytes(chunk(bytes))),
            Scalar::Uint8 => Self::Uint8(bytes[0]),
            Scalar::Int16 => Self::Int16(i16::from_le_bytes(chunk(bytes))),
            Scalar::Uint16 => Self::Uint16(u16::from_le_bytes(chunk(bytes))),
            Scalar::Int32 => Self::Int32(i32::from_le_bytes(chunk(bytes))),
            Scalar::Uint32 => Self::Uint32(u32::from_le_bytes(chunk(bytes))),
            Scalar::Int64 => Self::Int64(i64::from_le_bytes(chunk(bytes))),
            Scalar::Uint64 => Self::Uint64(u64::from_le_bytes(chunk(bytes))),
            Scalar::Float32 => Self::Float32(f32::from_le_bytes(chunk(bytes))),
            Scalar::Float64 => Self::Float64(f64::from_le_bytes(chunk(bytes))),
        }
    }

    /// Writes the scalar at the start of `bytes`, little-endian, as a struct
    /// holds it, with a bool as 0 or 1
    ///
    /// # Panics
    ///
    /// If the bytes are shorter than the scalar.
    pub fn write_le(self, bytes: &mut [u8]) {
        match self {
            Self::Bool(value) => bytes[0] = u8::from(value),
            Self::Int8(value) => bytes[..1].copy_from_slice(&value.to_le_bytes()),
            Self::Uint8(value) => bytes[0] = value,
            Self::Int16(value) => bytes[..2].copy_from_slice(&value.to_le_bytes()),
            Self::Uint16(value) => bytes[..2].copy_from_slice(&value.to_le_bytes()),
            Self::Int32(value) => bytes[..4].copy_from_slice(&value.to_le_bytes()),
            Self::Uint32(value) => bytes[..4].copy_from_slice(&value.to_le_bytes()),
            Self::Int64(value) => bytes[..8].copy_from_slice(&value.to_le_bytes()),
            Self::Uint64(value) => bytes[..8].copy_from_slice(&value.to_le_bytes()),
            Self::Float32(value) => bytes[..4].copy_from_slice(&value.to_le_bytes()),
            Self::Float64(value) => bytes[..8].copy_from_slice(&value.to_le_bytes()),
        }
    }

    /// Appends the scalar as JSON writes it: a bool as `true` or `false`, an
    /// integer of up to 32 bits as a number, one of 64 bits as a string of
    /// its digits, and a float as a number or its name
    pub fn push_json(self, json: &mut String) {
        let _ = match self {
            Self::Bool(value) => write!(json, "{value}"),
            Self::Int8(value) => write!(json, "{value}"),
            Self::Uint8(value) => write!(json, "{value}"),
            Self::Int16(value) => write!(json, "{value}"),
            Self::Uint16(value) => write!(json, "{value}"),
            Self::Int32(value) => write!(json, "{value}"),
            Self::Uint32(value) => write!(json, "{value}"),
            Self::Int64(value) => write!(json, "\"{value}\""),
            Self::Uint64(value) => write!(json, "\"{value}\""),
            Self::Float32(value) if value.is_nan() && value.to_bits() != FLOAT32_NAN => {
                write!(json, "\"NaN:0x{:08x}\"", value.to_bits())
            }
            Self::Float64(value) if value.is_nan() && value.to_bits() != FLOAT64_NAN => {
                write!(json, "\"NaN:0x{:016x}\"", value.to_bits())
            }
            // The float64 that equals it, which parses and narrows to it
            Self::Float32(value) => Ok(push_float(json, f64::from(value))),
            Self::Float64(value) => Ok(push_float(json, value)),
        };
    }

    /// The scalar that a JSON token holds, read as `scalar`, or `None` if
    /// it holds none: another kind of token, or a number that the type
    /// doesn't hold. `text` is the token's text, whose digits a number is
    /// parsed from.
    pub fn from_json(scalar: Scalar, token: &Token, text: &str) -> Option<Self> {
        match (scalar, token) {
            (Scalar::Bool, Token::True) => Some(Self::Bool(true)),
            (Scalar::Bool, Token::False) => Some(Self::Bool(false)),
            (Scalar::Int8, Token::Number) => integer(text).map(Self::Int8),
            (Scalar::Uint8, Token::Number) => integer(text).map(Self::Uint8),
            (Scalar::Int16, Token::Number) => integer(text).map(Self::Int16),
            (Scalar::Uint16, Token::Number) => integer(text).map(Self::Uint16),
            (Scalar::Int32, Token::Number) => integer(text).map(Self::Int32),
            (Scalar::Uint32, Token::Number) => integer(text).map(Self::Uint32),
            (Scalar::Int64, Token::Number) => integer(text).map(Self::Int64),
            (Scalar::Uint64, Token::Number) => integer(text).map(Self::Uint64),
            (Scalar::Int64, Token::String(digits)) => integer(digits).map(Self::Int64),
            (Scalar::Uint64, Token::String(digits)) => integer(digits).map(Self::Uint64),
            (Scalar::Float32, Token::Number) => finite(text).map(Self::Float32),
            (Scalar::Float64, Token::Number) => finite(text).map(Self::Float64),
            (Scalar::Float32, Token::String(name)) => {
                let bits = named_float(name, FLOAT32_NAN, 8, |bits| f32::from_bits(bits).is_nan());
                bits.map(|bits| Self::Float32(f32::from_bits(bits)))
            }
            (Scalar::Float64, Token::String(name)) => {
                let bits = named_float(name, FLOAT64_NAN, 16, |bits| f64::from_bits(bits).is_nan());
                bits.map(|bits| Self::Float64(f64::from_bits(bits)))
            }
            _ => None,
        }
    }

    /// What JSON writes a scalar type as, for the help of a value that
    /// isn't one
    pub fn expectation(scalar: Scalar) -> String {
        let name = scalar.name();
        let article = article(name);

        let (min, max) = match scalar {
            Scalar::Bool => return "a bool is `true` or `false`".to_owned(),
            Scalar::Float32 | Scalar::Float64 => {
                return format!(
                    "a {name} is a number within its range, or \"NaN\", \"Infinity\", \"-Infinity\" or \"-0\""
                );
            }
            Scalar::Int8 => ("-128", "127"),
            Scalar::Uint8 => ("0", "255"),
            Scalar::Int16 => ("-32768", "32767"),
            Scalar::Uint16 => ("0", "65535"),
            Scalar::Int32 => ("-2147483648", "2147483647"),
            Scalar::Uint32 => ("0", "4294967295"),
            Scalar::Int64 => ("-9223372036854775808", "9223372036854775807"),
            Scalar::Uint64 => ("0", "18446744073709551615"),
        };
        let forms = if scalar.size() == 8 {
            ", as a number or as a string of its digits"
        } else {
            ""
        };
        format!("{article} {name} is an integer from {min} to {max}{forms}")
    }
}

/// The article of a type's name: `an int8`, and `a` before every other
pub fn article(name: &str) -> &'static str {
    if name.starts_with('i') { "an" } else { "a" }
}

/// Appends a float as JSON writes it: the shortest digits that parse to it,
/// with an exponent where JavaScript writes one, so that the text is the
/// conformance suite's, or the name of a value that JSON has no number for.
/// `-0` is named too, since parsers read the number as an integer's 0.
fn push_float(json: &mut String, value: f64) {
    if value.is_nan() {
        json.push_str("\"NaN\"");
    } else if value.is_infinite() {
        json.push_str(if value > 0.0 {
            "\"Infinity\""
        } else {
            "\"-Infinity\""
        });
    } else if value == 0.0 && value.is_sign_negative() {
        json.push_str("\"-0\"");
    } else {
        push_finite(json, value);
    }
}

fn push_finite(json: &mut String, value: f64) {
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("a float in scientific notation has an exponent");
    let exponent: i32 = exponent.parse().expect("an exponent is an integer");

    let _ = if (-6..21).contains(&exponent) {
        write!(json, "{value}")
    } else {
        write!(json, "{mantissa}e{exponent:+}")
    };
}

/// An integer from its decimal digits, after a `-` if it's negative, or
/// `None` if the text is anything else, or the type doesn't hold it
fn integer<T: std::str::FromStr>(text: &str) -> Option<T> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|digit| digit.is_ascii_digit()) {
        return None;
    }

    // `-0` is 0, which an unsigned type's parser doesn't take with its sign
    let is_zero = digits.bytes().all(|digit| digit == b'0');
    if is_zero { digits } else { text }.parse().ok()
}

/// The float nearest a JSON number, or `None` if it's beyond the type's
/// range, which parses as an infinity
fn finite<T: std::str::FromStr + Into<f64> + Copy>(text: &str) -> Option<T> {
    text.parse()
        .ok()
        .filter(|&value: &T| value.into().is_finite())
}

/// The bits of a float that a string names: `NaN`, an infinity, `-0`, or
/// `NaN:0x` and every bit of another NaN, in `digits` hexadecimal digits.
/// The sign is the top bit of the `digits * 4`.
fn named_float<T>(name: &str, nan: T, digits: usize, is_nan: impl Fn(T) -> bool) -> Option<T>
where
    T: Copy + TryFrom<u64>,
{
    let width = digits * 4;
    let sign = 1_u64 << (width - 1);
    // The exponent's bits, all set: 8 of a float32 and 11 of a float64
    let exponent_bits = if digits == 8 { 8 } else { 11 };
    let infinity = ((1_u64 << exponent_bits) - 1) << (width - 1 - exponent_bits);

    let bits = match name {
        "NaN" => return Some(nan),
        "Infinity" => infinity,
        "-Infinity" => sign | infinity,
        "-0" => sign,
        _ => {
            let hexadecimal = name.strip_prefix("NaN:0x")?;
            let is_whole = hexadecimal.len() == digits
                && hexadecimal.bytes().all(|digit| digit.is_ascii_hexdigit());
            let bits = u64::from_str_radix(hexadecimal, 16)
                .ok()
                .filter(|_| is_whole)?;
            return T::try_from(bits).ok().filter(|&bits| is_nan(bits));
        }
    };
    T::try_from(bits).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(value: ScalarValue) -> String {
        let mut json = String::new();
        value.push_json(&mut json);
        json
    }

    /// The scalar that the JSON of a value reads as, which is one token
    fn read(scalar: Scalar, json: &str) -> Option<ScalarValue> {
        let mut tokens = super::super::syntax::Tokens::new(json);
        let (token, span) = tokens.next().unwrap();
        ScalarValue::from_json(scalar, &token, tokens.text(span))
    }

    #[test]
    fn writes_floats_as_the_conformance_suite_does() {
        for (value, expected) in [
            (0.0, "0"),
            (1.0, "1"),
            (-2.25, "-2.25"),
            (5e-324, "5e-324"),
            (1.797_693_134_862_315_7e308, "1.7976931348623157e+308"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (1e-6, "0.000001"),
            (1e-7, "1e-7"),
            (-0.0, "\"-0\""),
            (f64::INFINITY, "\"Infinity\""),
            (f64::NEG_INFINITY, "\"-Infinity\""),
            (f64::from_bits(FLOAT64_NAN), "\"NaN\""),
            (
                f64::from_bits(0xfff8_0000_0000_0001),
                "\"NaN:0xfff8000000000001\"",
            ),
        ] {
            assert_eq!(json(ScalarValue::Float64(value)), expected);
        }

        for (value, expected) in [
            (0.1, "0.10000000149011612"),
            (f32::MAX, "3.4028234663852886e+38"),
            (f32::from_bits(1), "1.401298464324817e-45"),
            (-0.0, "\"-0\""),
            (f32::from_bits(FLOAT32_NAN), "\"NaN\""),
            (f32::from_bits(0x7f80_0001), "\"NaN:0x7f800001\""),
        ] {
            assert_eq!(json(ScalarValue::Float32(value)), expected);
        }
    }

    #[test]
    fn reads_back_the_bits_of_every_float_it_writes() {
        let float64s = [
            0.0,
            -0.0,
            0.1,
            5e-324,
            f64::MAX,
            f64::MIN_POSITIVE,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::from_bits(FLOAT64_NAN),
            f64::from_bits(0x7ff0_0000_0000_0001),
            f64::from_bits(0xffff_ffff_ffff_ffff),
        ];
        for value in float64s {
            let Some(ScalarValue::Float64(read)) =
                read(Scalar::Float64, &json(ScalarValue::Float64(value)))
            else {
                panic!("{value} reads as a float64");
            };
            assert_eq!(read.to_bits(), value.to_bits());
        }

        let float32s = [
            0.0,
            -0.0,
            0.1,
            f32::from_bits(1),
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::from_bits(FLOAT32_NAN),
            f32::from_bits(0xffc0_0000),
            f32::from_bits(0x7f80_0001),
        ];
        for value in float32s {
            let Some(ScalarValue::Float32(read)) =
                read(Scalar::Float32, &json(ScalarValue::Float32(value)))
            else {
                panic!("{value} reads as a float32");
            };
            assert_eq!(read.to_bits(), value.to_bits());
        }
    }

    #[test]
    fn reads_a_float_from_any_json_number() {
        let bits = |json: &str| match read(Scalar::Float64, json) {
            Some(ScalarValue::Float64(value)) => Some(value.to_bits()),
            _ => None,
        };

        assert_eq!(bits("1"), Some(1.0_f64.to_bits()));
        assert_eq!(bits("-0"), Some((-0.0_f64).to_bits()));
        assert_eq!(bits("-0.0"), Some((-0.0_f64).to_bits()));
        assert_eq!(bits("1E3"), Some(1000.0_f64.to_bits()));
        assert_eq!(bits("1e-999"), Some(0.0_f64.to_bits()));
        assert_eq!(bits("1e999"), None);
        assert_eq!(bits("true"), None);
        assert_eq!(bits("\"nan\""), None);
        assert_eq!(bits("\"NaN:0x7ff0000000000000\""), None);
        assert_eq!(bits("\"NaN:0x7ff8\""), None);

        assert!(read(Scalar::Float32, "3.5e38").is_none());
        assert!(read(Scalar::Float32, "\"NaN:0x7ff8000000000001\"").is_none());
    }

    #[test]
    fn reads_integers_within_their_types_range_only() {
        let fits = |scalar, json: &str| read(scalar, json).is_some();

        for (scalar, min, max, below, above) in [
            (Scalar::Int8, "-128", "127", "-129", "128"),
            (Scalar::Uint8, "0", "255", "-1", "256"),
            (Scalar::Int16, "-32768", "32767", "-32769", "32768"),
            (Scalar::Uint16, "0", "65535", "-1", "65536"),
            (
                Scalar::Int32,
                "-2147483648",
                "2147483647",
                "-2147483649",
                "2147483648",
            ),
            (Scalar::Uint32, "0", "4294967295", "-1", "4294967296"),
            (
                Scalar::Int64,
                "-9223372036854775808",
                "9223372036854775807",
                "-9223372036854775809",
                "9223372036854775808",
            ),
            (
                Scalar::Uint64,
                "0",
                "18446744073709551615",
                "-1",
                "18446744073709551616",
            ),
        ] {
            assert!(fits(scalar, min) && fits(scalar, max), "{scalar:?}");
            assert!(!fits(scalar, below) && !fits(scalar, above), "{scalar:?}");
            assert!(fits(scalar, "-0") && !fits(scalar, "1.0") && !fits(scalar, "1e2"));
            assert!(!fits(scalar, "true") && !fits(scalar, "null"));
            assert!(ScalarValue::expectation(scalar).contains(&format!("from {min} to {max}")));
        }

        assert!(fits(Scalar::Uint64, "\"18446744073709551615\""));
        assert!(fits(Scalar::Int64, "\"-9223372036854775808\""));
        assert!(!fits(Scalar::Int64, "\"+1\"") && !fits(Scalar::Int64, "\"1 \""));
        assert!(!fits(Scalar::Uint32, "\"1\""));
    }

    #[test]
    fn reads_and_writes_a_structs_scalars_little_endian() {
        let mut bytes = [0xff; 8];

        ScalarValue::Uint16(0x0102).write_le(&mut bytes);
        ScalarValue::Bool(true).write_le(&mut bytes[2..]);
        assert_eq!(bytes[..3], [2, 1, 1]);

        let read = ScalarValue::from_le_bytes(Scalar::Uint16, &bytes);
        assert!(matches!(read, ScalarValue::Uint16(0x0102)));
        let read = ScalarValue::from_le_bytes(Scalar::Bool, &[0x80]);
        assert!(matches!(read, ScalarValue::Bool(true)));
    }
}
