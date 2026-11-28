use crate::workloads::{DocumentShape, ListShape, RecordShape, SampleShape, Shape, StructsShape};

/// A library's error, of either direction. Errors only describe failures,
/// which the timed calls don't have, so boxing them costs nothing there.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// A library that encodes some of the workloads
pub trait Codec: Copy {
    /// The library's name, which names its benchmarks
    const NAME: &'static str;
}

/// One library's encoding of one of the workloads' shapes
pub trait Workload<S: Shape>: Codec {
    /// The serialized bytes, in what the library writes them into
    type Bytes: AsRef<[u8]>;

    /// Writes the value, as the library's documentation shows
    ///
    /// # Errors
    ///
    /// The library's error, if it can't write the value.
    fn serialize(self, input: &S::Value<'_>) -> Result<Self::Bytes, Error>;

    /// Reads every field, with strings and columns borrowed from the bytes
    /// where the library reads them in place, and copied otherwise
    ///
    /// # Errors
    ///
    /// The library's error, if the bytes aren't the shape's.
    fn deserialize(self, bytes: &[u8]) -> Result<S::Value<'_>, Error>;
}

/// The documents' workload, which also reads a column one element at a time
pub trait DocumentWorkload: Workload<DocumentShape> {
    /// Sums column `a` through the library's own per-element accessor, with
    /// [`crate::workloads::sum_column()`]
    ///
    /// # Errors
    ///
    /// The library's error, if the bytes aren't a document.
    fn sum_a(self, bytes: &[u8]) -> Result<f64, Error>;
}

/// A library that encodes every workload, as all but the fixed-size record's
/// alternatives do
pub trait Suite:
    Workload<RecordShape>
    + DocumentWorkload
    + Workload<ListShape>
    + Workload<StructsShape>
    + Workload<SampleShape>
{
}

impl<C> Suite for C where
    C: Workload<RecordShape>
        + DocumentWorkload
        + Workload<ListShape>
        + Workload<StructsShape>
        + Workload<SampleShape>
{
}
