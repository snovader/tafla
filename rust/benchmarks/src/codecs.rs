//! Each library's encoding of the workloads

mod flatbuffers;
mod le_bytes;
mod msgpack;
mod protobuf;
mod rkyv;
mod tafla;
mod tafla_gen;

pub use self::flatbuffers::{Finished, FlatBuffers};
pub use self::le_bytes::LeBytes;
pub use self::msgpack::RmpSerde;
pub use self::protobuf::{Prost, messages};
pub use self::rkyv::Rkyv;
pub use self::tafla::Tafla;
pub use self::tafla_gen::TaflaGen;

/// Expands `$body` once for each codec that encodes every workload.
///
/// `$codec` is bound to each in the order that results list them. It's a
/// macro rather than a loop, since each codec is a type of its own, which its
/// benchmarks are compiled for.
///
/// [`LeBytes`] encodes only the fixed-size record, so callers add it there.
#[macro_export]
macro_rules! for_each_codec {
    (|$codec:ident| $body:expr) => {{
        {
            let $codec = $crate::codecs::Tafla;
            $body;
        }
        {
            let $codec = $crate::codecs::TaflaGen;
            $body;
        }
        {
            let $codec = $crate::codecs::FlatBuffers;
            $body;
        }
        {
            let $codec = $crate::codecs::Prost;
            $body;
        }
        {
            let $codec = $crate::codecs::RmpSerde;
            $body;
        }
        {
            let $codec = $crate::codecs::Rkyv;
            $body;
        }
    }};
}
