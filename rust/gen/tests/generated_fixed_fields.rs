//! The code generated from `schemas/fixed_fields.tafla`, whose fields' sizes
//! the schema fixes, reads back what it writes in every offset type and
//! header, and fails to write a message whose fixed fields alone are too long
//! for its offset type, as `TableBuilder` fails. The Go tests run the same
//! schema, whose generated code lays these fields out itself.

// The schema's structs aren't in arrays, so their arrays' helpers go unused
#[allow(dead_code)]
#[path = "golden/fixed_fields/rust/fixed_fields.rs"]
mod fixed_fields;

use fixed_fields::{
    Block, Box as BoxMessage, BoxFields, FrameFields, Header, HeaderFields, Label, LabelFields,
    Overflow, OverflowFields, Pixel, PixelFields, Quad, RangeFields, Span, SpanFields, Strip,
    StripFields, Tagged, TaggedFields, Tiny, TinyFields, Track, TrackFields, Vec2,
};
use tafla::{BuildError, DecodeError, OffsetType};

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
