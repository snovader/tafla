//! The code generated from `schemas/layouts.tafla`, which has a message of
//! the same fields in every layout, writes the tables that `TableBuilder`
//! writes by hand, or fails as it fails, reads back what it writes, reads the
//! version of tables of any version and rejects those of other versions, and
//! reads corrupted and arbitrary bytes as values or decode errors without
//! panicking. tafla-gen decodes every table that it writes as JSON, which
//! encodes back into the same bytes.

#[path = "golden/layouts/rust/layouts.rs"]
mod layouts;
mod tables;

use layouts::{
    Uint8Offsets, Uint8OffsetsFields, Uint8OffsetsSized, Uint8OffsetsSizedFields,
    Uint8OffsetsSizedVersioned, Uint8OffsetsSizedVersionedFields, Uint8OffsetsVersioned,
    Uint8OffsetsVersionedFields, Uint16Offsets, Uint16OffsetsFields, Uint16OffsetsSized,
    Uint16OffsetsSizedFields, Uint16OffsetsSizedVersioned, Uint16OffsetsSizedVersionedFields,
    Uint16OffsetsVersioned, Uint16OffsetsVersionedFields, Uint32Offsets, Uint32OffsetsFields,
    Uint32OffsetsSized, Uint32OffsetsSizedFields, Uint32OffsetsSizedVersioned,
    Uint32OffsetsSizedVersionedFields, Uint32OffsetsVersioned, Uint32OffsetsVersionedFields,
};
use proptest::collection::vec;
use proptest::prelude::*;
use tables::{
    CASES, Fields, Header, Value, arbitrary_bytes, build, check_json_round_trip, check_truncated,
    expected, integers, overwrites, placements, strings_up_to, to_vec, truncations,
};
use tafla::{BuildError, DecodeError, OffsetType, TableLayout};

/// The values of the fields that every message of the schema holds, which
/// each message's generated fields borrow
#[derive(Clone, Debug)]
struct Owned {
    id: u32,
    name: String,
    values: Vec<u16>,
}

/// A message of the schema: its header, as its attributes set it, and its
/// generated code
struct Message {
    name: &'static str,
    header: Header,
    serialize: fn(&Owned) -> Result<Vec<u8>, BuildError>,
    /// Reads every field, after checking the version when the message is
    /// versioned
    read: fn(&[u8]) -> Result<Fields, DecodeError>,
    /// The generated `version_of()` of a versioned message
    version_of: Option<VersionOf>,
}

/// A generated `version_of()`
type VersionOf = fn(&[u8]) -> Result<u32, DecodeError>;

/// Reads every field of a message, including those after one that fails
macro_rules! read_fields {
    ($message:expr) => {{
        let message = $message;
        vec![
            message.id().map(Value::U32),
            message.name().map(Value::string),
            message.values().map(to_vec).map(Value::U16Array),
        ]
    }};
}

/// The `Message` of a generated message and its fields, whose `new()` checks
/// the version, and returns a `Result`, when it's `versioned`
macro_rules! message {
    ($message:ident, $fields:ident, $header:expr $(, $versioned:ident)?) => {
        Message {
            name: stringify!($message),
            header: $header,
            serialize: |owned| {
                let fields = $fields {
                    id: owned.id,
                    name: &owned.name,
                    values: &owned.values,
                };
                fields.serialize()
            },
            read: message!(@read $message $(, $versioned)?),
            version_of: message!(@version_of $message $(, $versioned)?),
        }
    };
    (@read $message:ident) => {
        |bytes| Ok(read_fields!($message::new(bytes)))
    };
    (@read $message:ident, versioned) => {
        |bytes| $message::new(bytes).map(|message| read_fields!(message))
    };
    (@version_of $message:ident) => {
        None
    };
    (@version_of $message:ident, versioned) => {
        Some($message::version_of)
    };
}

const UINT8_OFFSETS: TableLayout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint8);
const UINT16_OFFSETS: TableLayout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint16);
const UINT32_OFFSETS: TableLayout = TableLayout::DEFAULT;

/// Every message, with its layout and version from its attributes
fn messages() -> [Message; 12] {
    [
        message!(Uint8Offsets, Uint8OffsetsFields, Header::new(UINT8_OFFSETS)),
        message!(
            Uint8OffsetsSized,
            Uint8OffsetsSizedFields,
            Header::new(UINT8_OFFSETS.sized())
        ),
        message!(
            Uint8OffsetsVersioned,
            Uint8OffsetsVersionedFields,
            Header::versioned(UINT8_OFFSETS, 255),
            versioned
        ),
        message!(
            Uint8OffsetsSizedVersioned,
            Uint8OffsetsSizedVersionedFields,
            Header::versioned(UINT8_OFFSETS.sized(), 0),
            versioned
        ),
        message!(
            Uint16Offsets,
            Uint16OffsetsFields,
            Header::new(UINT16_OFFSETS)
        ),
        message!(
            Uint16OffsetsSized,
            Uint16OffsetsSizedFields,
            Header::new(UINT16_OFFSETS.sized())
        ),
        message!(
            Uint16OffsetsVersioned,
            Uint16OffsetsVersionedFields,
            Header::versioned(UINT16_OFFSETS, 65_535),
            versioned
        ),
        message!(
            Uint16OffsetsSizedVersioned,
            Uint16OffsetsSizedVersionedFields,
            Header::versioned(UINT16_OFFSETS.sized(), 1),
            versioned
        ),
        message!(
            Uint32Offsets,
            Uint32OffsetsFields,
            Header::new(UINT32_OFFSETS)
        ),
        message!(
            Uint32OffsetsSized,
            Uint32OffsetsSizedFields,
            Header::new(UINT32_OFFSETS.sized())
        ),
        message!(
            Uint32OffsetsVersioned,
            Uint32OffsetsVersionedFields,
            Header::versioned(UINT32_OFFSETS, 4_294_967_295),
            versioned
        ),
        message!(
            Uint32OffsetsSizedVersioned,
            Uint32OffsetsSizedVersionedFields,
            Header::versioned(UINT32_OFFSETS.sized(), 2),
            versioned
        ),
    ]
}

fn describe(owned: &Owned) -> Vec<Value> {
    vec![
        Value::U32(owned.id),
        Value::string(&owned.name),
        Value::U16Array(owned.values.clone()),
    ]
}

/// The most characters of a name and elements of `values`, in tables of up
/// to about 600 bytes, so that those of `uint8` offsets are too long for their
/// offsets about half the time
const MAX_LENGTH: usize = 96;

const SCHEMA: &str = include_str!("../../../schemas/layouts.tafla");

fn owned() -> impl Strategy<Value = Owned> {
    let values = vec(integers(0, u16::MAX), 0..=MAX_LENGTH);

    (integers(0, u32::MAX), strings_up_to(MAX_LENGTH), values)
        .prop_map(|(id, name, values)| Owned { id, name, values })
}

/// A version other than the message's own, which fits in its offsets, from
/// any `version`
fn other_version(header: Header, version: u32) -> Option<u32> {
    let own = header.version?;
    let fitted = version & header.layout.offset_type().max_value();

    Some(if fitted == own { fitted ^ 1 } else { fitted })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn every_layout_reads_back_its_fields_wherever_its_bytes_lie(
        owned in owned(),
        placement in placements(),
    ) {
        for message in messages() {
            let bytes = match (message.serialize)(&owned) {
                Ok(bytes) => placement.place(&bytes),
                // Too long for `uint8` offsets, as the next property checks
                Err(error) => {
                    let too_long = matches!(error, BuildError::TooLong { .. });
                    prop_assert!(too_long, "{}: {}", message.name, error);
                    continue;
                }
            };

            let read = (message.read)(bytes.bytes());
            prop_assert_eq!(read, Ok(expected(describe(&owned))), "{}", message.name);
        }
    }

    #[test]
    fn every_layout_is_written_as_table_builder_writes_it_or_fails_as_it_fails(owned in owned()) {
        for message in messages() {
            let by_hand = build(message.header, &describe(&owned));
            prop_assert_eq!((message.serialize)(&owned), by_hand, "{}", message.name);
            if let Ok(bytes) = (message.serialize)(&owned) {
                check_json_round_trip(SCHEMA, message.name, &bytes)?;
            }
        }
    }

    #[test]
    fn every_versioned_layout_rejects_other_versions_with_unsupported_version(
        owned in owned(),
        version in integers(0, u32::MAX),
    ) {
        for message in messages() {
            let Some(other) = other_version(message.header, version) else {
                continue;
            };
            let Ok(bytes) = build(message.header.with_version(other), &describe(&owned)) else {
                continue;
            };

            let error = (message.read)(&bytes).err();
            let supported = message.header.version.unwrap();
            prop_assert_eq!(
                error,
                Some(DecodeError::UnsupportedVersion { version: other, supported }),
                "{}",
                message.name
            );
        }
    }

    #[test]
    fn every_versioned_layout_reads_the_version_of_its_bytes_of_any_version(
        owned in owned(),
        version in integers(0, u32::MAX),
    ) {
        for message in messages() {
            let Some(version_of) = message.version_of else {
                continue;
            };
            let version = version & message.header.layout.offset_type().max_value();
            let Ok(bytes) = build(message.header.with_version(version), &describe(&owned)) else {
                continue;
            };

            prop_assert_eq!(version_of(&bytes), Ok(version), "{}", message.name);
        }
    }

    #[test]
    fn corrupted_tables_of_every_layout_read_as_values_or_decode_errors(
        owned in owned(),
        overwrite in overwrites(),
    ) {
        let written = describe(&owned);

        for message in messages() {
            let Ok(bytes) = (message.serialize)(&owned) else {
                continue;
            };

            for truncated in truncations(&bytes) {
                check_truncated((message.read)(truncated), &written)?;
            }

            // Any values can be read from overwritten bytes, as long as no
            // getter panics: as they are, and with their version restored, so
            // that the getters of a versioned message read them
            let overwritten = overwrite.apply(&bytes);
            let _ = (message.read)(&overwritten);
            let _ = (message.read)(&message.header.write_version(&overwritten));
        }
    }

    #[test]
    fn arbitrary_bytes_read_as_every_layout_or_decode_errors(bytes in arbitrary_bytes()) {
        for message in messages() {
            // As they are, and with the message's version, so that its
            // getters read them
            let _ = (message.read)(&bytes);
            let _ = (message.read)(&message.header.write_version(&bytes));
        }
    }
}

/// Names of every length from 64 bytes short of the limit of `uint8` and
/// `uint16` offsets to past it, so that each message's longest table that
/// fits is written as `TableBuilder` writes it, and those longer fail as it
/// fails.
/// Each byte of the name makes the table a byte longer, or two with the
/// padding of the array after it, so the longest is at the limit or a byte
/// short of it.
#[test]
fn every_layout_of_short_offsets_is_written_up_to_their_limit_and_fails_past_it() {
    for message in messages() {
        let max = message.header.layout.offset_type().max_value() as usize;
        if max > usize::from(u16::MAX) {
            continue;
        }

        let mut longest = 0;
        let mut rejected = false;
        for length in max - 64..=max {
            let owned = Owned {
                id: u32::MAX,
                name: "a".repeat(length),
                values: vec![1, u16::MAX],
            };

            let written = (message.serialize)(&owned);
            let by_hand = build(message.header, &describe(&owned));
            assert_eq!(written, by_hand, "{}", message.name);
            match written {
                Ok(bytes) => longest = longest.max(bytes.len()),
                Err(_) => rejected = true,
            }
        }

        assert!(max - 1 <= longest, "{}: {longest} bytes", message.name);
        assert!(rejected, "{}", message.name);
    }
}
