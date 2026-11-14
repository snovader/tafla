//! The code generated from `schemas/fixed_fields.tafla`, whose fields' sizes
//! the schema fixes, writes the tables that `TableBuilder` writes by hand in
//! every offset type and header, reads back what it writes, and fails to
//! write a message whose fixed fields alone, or with a string after them,
//! are too long for its offset type, as `TableBuilder` fails. The generated
//! code writes these messages and nested tables whole, as the Go and C++
//! code does, whose tests run the same schema.

// The schema's structs aren't in arrays, so their arrays' helpers go unused
#[allow(dead_code)]
#[path = "golden/fixed_fields/rust/fixed_fields.rs"]
mod fixed_fields;
mod tables;

use fixed_fields::{
    Block, Box as BoxMessage, BoxFields, FrameFields, Header, HeaderFields, Label, LabelFields,
    Overflow, OverflowFields, Pixel, PixelFields, Quad, RangeFields, Span, SpanFields, Strip,
    StripFields, Tagged, TaggedFields, Tiny, TinyFields, Track, TrackFields, Vec2,
};
use proptest::collection::vec;
use proptest::prelude::*;
use tables::{CASES, Value, arrays, build, float32s, integers, strings, strings_up_to};
use tafla::{BuildError, DecodeError, OffsetType, TableLayout};

// The headers of the messages, from their attributes in the schema
const TINY: tables::Header = tables::Header::new(uint8().sized());
const TAGGED: tables::Header = tables::Header::versioned(uint8().sized(), 3);
const HEADER: tables::Header = tables::Header::new(uint8());
const PIXEL: tables::Header = tables::Header::versioned(uint16().sized(), 2);
const STRIP: tables::Header = tables::Header::new(uint16());
const BOX: tables::Header = tables::Header::versioned(TableLayout::DEFAULT, 1);
const DEFAULT: tables::Header = tables::Header::new(TableLayout::DEFAULT);

const fn uint8() -> TableLayout {
    TableLayout::DEFAULT.with_offset_type(OffsetType::Uint8)
}

const fn uint16() -> TableLayout {
    TableLayout::DEFAULT.with_offset_type(OffsetType::Uint16)
}

const ORIGIN: Vec2 = Vec2 { x: 1.5, y: -0.0 };

fn frame() -> FrameFields {
    FrameFields {
        origin: ORIGIN,
        range: RangeFields { start: 3, end: 7 },
        flags: 0xa5a5,
    }
}

#[test]
fn whole_messages_of_uint8_offsets_read_back_their_fields() -> Result<(), DecodeError> {
    let tiny = TinyFields { id: 513, kind: 9 }.serialize().unwrap();
    assert_eq!(tiny.len(), 6);
    let read = Tiny::new(&tiny);
    assert_eq!((read.id()?, read.kind()?), (513, 9));

    let tagged = TaggedFields {
        id: u32::MAX,
        position: ORIGIN,
        label: "a 😀",
    }
    .serialize()
    .unwrap();
    let read = Tagged::new(&tagged)?;
    assert_eq!(Tagged::version_of(&tagged)?, 3);
    assert_eq!(read.id()?, u32::MAX);
    assert_eq!(read.position()?.y.to_bits(), (-0.0_f32).to_bits());
    assert_eq!(read.label()?, "a 😀");
    Ok(())
}

#[test]
fn runs_of_fixed_fields_read_back_with_the_fields_after_them() -> Result<(), DecodeError> {
    let header = HeaderFields {
        kind: 1,
        length: 300,
        values: &[1, 2, 3],
    }
    .serialize()
    .unwrap();
    let read = Header::new(&header);
    assert_eq!(
        (read.kind()?, read.length()?, read.values()?),
        (1, 300, &[1, 2, 3][..])
    );

    let strip = StripFields {
        id: 7,
        origin: ORIGIN,
        values: &[1, u16::MAX],
    }
    .serialize()
    .unwrap();
    let read = Strip::new(&strip);
    assert_eq!((read.id()?, read.origin()?), (7, ORIGIN));
    assert_eq!(read.values()?.to_vec(), [1, u16::MAX]);

    let track = TrackFields {
        id: 8,
        origin: ORIGIN,
        frame: frame(),
        points: &[0.5, 1.5],
        name: "track",
    }
    .serialize()
    .unwrap();
    let read = Track::new(&track);
    let frame = read.frame()?;
    assert_eq!((read.id()?, read.origin()?), (8, ORIGIN));
    assert_eq!((frame.origin()?, frame.flags()?), (ORIGIN, 0xa5a5));
    assert_eq!((frame.range()?.start()?, frame.range()?.end()?), (3, 7));
    assert_eq!(read.points()?.to_vec(), [0.5, 1.5]);
    assert_eq!(read.name()?, "track");
    Ok(())
}

#[test]
fn whole_messages_of_uint16_and_uint32_offsets_read_back_their_fields() -> Result<(), DecodeError> {
    let pixel = PixelFields {
        position: ORIGIN,
        color: 0x0102_0304,
    }
    .serialize()
    .unwrap();
    let read = Pixel::new(&pixel)?;
    assert_eq!(Pixel::version_of(&pixel)?, 2);
    assert_eq!((read.position()?, read.color()?), (ORIGIN, 0x0102_0304));

    let corner = Vec2 { x: 2.0, y: 3.0 };
    let written = BoxFields {
        min: ORIGIN,
        max: corner,
        range: RangeFields { start: 1, end: 2 },
    }
    .serialize()
    .unwrap();
    let read = BoxMessage::new(&written)?;
    assert_eq!(BoxMessage::version_of(&written)?, 1);
    assert_eq!((read.min()?, read.max()?), (ORIGIN, corner));
    assert_eq!((read.range()?.start()?, read.range()?.end()?), (1, 2));

    let label = LabelFields { text: "" }.serialize().unwrap();
    assert_eq!(Label::new(&label).text()?, "");
    Ok(())
}

#[test]
fn fixed_tables_after_other_fields_read_back_alone_and_in_a_vector() -> Result<(), DecodeError> {
    let ranges = [
        RangeFields { start: 1, end: 2 },
        RangeFields { start: 3, end: 4 },
    ];
    let span = SpanFields {
        name: "span",
        range: RangeFields { start: 5, end: 6 },
        frame: frame(),
        ranges: &ranges,
    }
    .serialize()
    .unwrap();

    let read = Span::new(&span);
    assert_eq!(read.name()?, "span");
    assert_eq!((read.range()?.start()?, read.range()?.end()?), (5, 6));
    assert_eq!(read.frame()?.flags()?, 0xa5a5);
    let mut starts = Vec::new();
    for range in read.ranges()?.iter() {
        let range = range?;
        starts.push((range.start()?, range.end()?));
    }
    assert_eq!(starts, [(1, 2), (3, 4)]);

    let view = read.ranges()?;
    assert_eq!(view.len(), 2);
    let last = view.get(1).expect("an item")?;
    assert_eq!((last.start()?, last.end()?), (3, 4));
    Ok(())
}

#[test]
fn a_message_whose_fixed_fields_are_too_long_for_its_offsets_fails_to_write() {
    let quad = Quad {
        a: 1.0,
        b: 2.0,
        c: 3.0,
        d: 4.0,
        e: 5.0,
        f: 6.0,
        g: 7.0,
        h: 8.0,
    };
    let fields = OverflowFields {
        id: 1,
        block: Block {
            first: quad,
            second: quad,
            third: quad,
            fourth: quad,
        },
    };

    // The offsets, the id and then the block's 256 bytes
    assert_eq!(
        fields.serialize().unwrap_err(),
        BuildError::TooLong {
            index: 1,
            length: 259,
            offset_type: OffsetType::Uint8,
        }
    );

    // A table that something else wrote reads as values or decode errors
    let read = Overflow::new(&[2, 3, 1]);
    assert_eq!(read.id(), Ok(1));
    assert!(read.block().is_err());
}

#[test]
fn a_whole_message_whose_string_makes_it_too_long_fails_to_write() {
    // The header, offsets, fields and length take 18 of the 255 bytes
    let fits = "a".repeat(237);
    let tagged = TaggedFields {
        id: 1,
        position: ORIGIN,
        label: &fits,
    };
    assert_eq!(tagged.serialize().map(|bytes| bytes.len()), Ok(255));

    let too_long = "a".repeat(238);
    let tagged = TaggedFields {
        label: &too_long,
        ..tagged
    };
    assert_eq!(
        tagged.serialize(),
        Err(BuildError::TooLong {
            index: 2,
            length: 256,
            offset_type: OffsetType::Uint8,
        })
    );
}

// Writing every message as `TableBuilder` writes it

fn vec2s() -> impl Strategy<Value = Vec2> {
    (float32s(), float32s()).prop_map(|(x, y)| Vec2 { x, y })
}

fn ranges() -> impl Strategy<Value = RangeFields> {
    let values = || integers(u32::MIN, u32::MAX);
    (values(), values()).prop_map(|(start, end)| RangeFields { start, end })
}

fn frames() -> impl Strategy<Value = FrameFields> {
    (vec2s(), ranges(), integers(u16::MIN, u16::MAX)).prop_map(|(origin, range, flags)| {
        FrameFields {
            origin,
            range,
            flags,
        }
    })
}

fn describe_vec2(vec2: Vec2) -> Value {
    Value::Struct(vec![Value::f32(vec2.x), Value::f32(vec2.y)])
}

fn describe_range(range: &RangeFields) -> Vec<Value> {
    vec![Value::U32(range.start), Value::U32(range.end)]
}

fn describe_frame(frame: &FrameFields) -> Vec<Value> {
    vec![
        describe_vec2(frame.origin),
        Value::Table(describe_range(&frame.range)),
        Value::U16(frame.flags),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn tiny_is_written_whole_as_table_builder_writes_it(
        id in integers(u16::MIN, u16::MAX),
        kind in integers(u8::MIN, u8::MAX),
    ) {
        let fields = TinyFields { id, kind };

        prop_assert_eq!(fields.serialize(), build(TINY, &[Value::U16(id), Value::U8(kind)]));
    }

    /// Labels of up to 320 bytes, of which 237 fit after the fixed fields
    /// in `uint8` offsets, so the longer ones fail as `TableBuilder` fails
    #[test]
    fn tagged_is_written_whole_or_fails_as_table_builder_writes_it_or_fails(
        id in integers(u32::MIN, u32::MAX),
        position in vec2s(),
        label in strings_up_to(80),
    ) {
        let fields = TaggedFields { id, position, label: &label };
        let values = [Value::U32(id), describe_vec2(position), Value::string(&label)];

        prop_assert_eq!(fields.serialize(), build(TAGGED, &values));
    }

    #[test]
    fn pixel_and_box_are_written_whole_as_table_builder_writes_them(
        position in vec2s(),
        color in integers(u32::MIN, u32::MAX),
        max in vec2s(),
        range in ranges(),
    ) {
        let pixel = PixelFields { position, color };
        let values = [describe_vec2(position), Value::U32(color)];
        prop_assert_eq!(pixel.serialize(), build(PIXEL, &values));

        let values = [describe_vec2(position), describe_vec2(max), Value::Table(describe_range(&range))];
        let written = BoxFields { min: position, max, range };
        prop_assert_eq!(written.serialize(), build(BOX, &values));
    }

    #[test]
    fn header_and_strip_are_written_as_table_builder_writes_them(
        kind in integers(u8::MIN, u8::MAX),
        length in integers(u16::MIN, u16::MAX),
        bytes in arrays(any::<u8>()),
        origin in vec2s(),
        values in arrays(any::<u16>()),
    ) {
        let header = HeaderFields { kind, length, values: &bytes };
        let described = [Value::U8(kind), Value::U16(length), Value::U8Array(bytes.clone())];
        prop_assert_eq!(header.serialize(), build(HEADER, &described));

        let strip = StripFields { id: u32::from(length), origin, values: &values };
        let described = [Value::U32(u32::from(length)), describe_vec2(origin), Value::U16Array(values.clone())];
        prop_assert_eq!(strip.serialize(), build(STRIP, &described));
    }

    #[test]
    fn tables_written_whole_are_nested_as_table_builder_nests_them(
        id in integers(u32::MIN, u32::MAX),
        origin in vec2s(),
        frame in frames(),
        points in arrays(float32s()),
        name in strings(),
        range in ranges(),
        items in vec(ranges(), 0..=8),
    ) {
        let track = TrackFields { id, origin, frame: frame.clone(), points: &points, name: &name };
        let values = [
            Value::U32(id),
            describe_vec2(origin),
            Value::Table(describe_frame(&frame)),
            Value::f32_array(points.iter().copied()),
            Value::string(&name),
        ];
        prop_assert_eq!(track.serialize(), build(DEFAULT, &values));

        let values = [
            Value::string(&name),
            Value::Table(describe_range(&range)),
            Value::Table(describe_frame(&frame)),
            Value::Vector(items.iter().map(describe_range).collect()),
        ];
        let span = SpanFields { name: &name, range, frame, ranges: &items };
        prop_assert_eq!(span.serialize(), build(DEFAULT, &values));
    }

    #[test]
    fn label_is_written_whole_as_table_builder_writes_it(text in strings()) {
        let label = LabelFields { text: &text };

        prop_assert_eq!(label.serialize(), build(DEFAULT, &[Value::string(&text)]));
    }
}
