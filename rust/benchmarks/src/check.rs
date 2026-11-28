//! Checks that a codec reads back exactly what it wrote.
//!
//! Floats are compared by their bits, so that no benchmark times a codec that
//! skips or corrupts part of the work. The benchmarks run the checks before
//! timing, and the tests run them for every codec.

use crate::codec::{DocumentWorkload, Error, Reused, Workload};
use crate::workloads::{Document, Shape, sum_column};

/// Checks that `codec` reads `input` back from the bytes it wrote, and wrote
/// no more than the shape's capacity, which codecs whose API takes one are
/// given
///
/// # Errors
///
/// The codec's error, or an error that names the codec and the workload when
/// it reads back other values or writes more than the capacity.
pub fn round_trip<S: Shape, C: Workload<S>>(codec: C, input: &S::Value<'_>) -> Result<(), Error> {
    let bytes = codec.serialize(input)?;
    let length = bytes.as_ref().len();
    let capacity = S::capacity(input);
    if length > capacity {
        let (codec, shape) = (C::NAME, S::NAME);
        return Err(format!("{codec} wrote {length} bytes of the {shape}, past {capacity}").into());
    }
    let output = codec.deserialize(bytes.as_ref())?;

    if S::same(&output, input) {
        Ok(())
    } else {
        Err(format!("{} didn't read back the {}", C::NAME, S::NAME).into())
    }
}

/// Checks that `codec` sums column `a` of the bytes it wrote to the sum of
/// `input`'s, with the same bits
///
/// # Errors
///
/// The codec's error, or an error that names the codec when the sums differ.
pub fn sum_a<C: DocumentWorkload>(codec: C, input: &Document<'_>) -> Result<(), Error> {
    let bytes = codec.serialize(input)?;
    let sum = codec.sum_a(bytes.as_ref())?;
    let expected = sum_column(input.a.iter().copied());

    if sum.to_bits() == expected.to_bits() {
        Ok(())
    } else {
        Err(format!("{} summed column a to {sum}, not {expected}", C::NAME).into())
    }
}

/// Checks that `codec`, writing `input` twice into the same state, writes
/// the bytes that it writes into new ones each time, and reads `input` back
/// from them
///
/// # Errors
///
/// The codec's error, or an error that names the codec and the workload when
/// it writes other bytes or reads back other values.
pub fn reused<S: Shape, C: Reused<S>>(codec: C, input: &S::Value<'_>) -> Result<(), Error> {
    let fresh = codec.serialize(input)?;
    let mut state = C::State::default();
    for _ in 0..2 {
        let bytes = codec.serialize_reused(input, &mut state)?;
        if bytes != fresh.as_ref() {
            let (codec, shape) = (C::NAME, S::NAME);
            return Err(format!("{codec} wrote other bytes of the {shape} into its state").into());
        }
        let output = codec.deserialize(bytes)?;
        if !S::same(&output, input) {
            let (codec, shape) = (C::NAME, S::NAME);
            return Err(format!("{codec} didn't read back the {shape} from its state").into());
        }
    }

    Ok(())
}
