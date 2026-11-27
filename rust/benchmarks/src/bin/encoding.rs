//! Prints what the timings don't show: each codec's encoded size of every
//! workload, and what it does with a document cut short, as the TypeScript
//! comparison prints after its timings. `--strings` prints each codec's
//! encoded size of each shape of the strings comparison instead.

use std::panic::{self, AssertUnwindSafe};

use rkyv::util::AlignedVec;
use tafla_benchmarks::codec::{Codec, Error, Suite, Workload};
use tafla_benchmarks::codecs::LeBytes;
use tafla_benchmarks::for_each_codec;
use tafla_benchmarks::string_workloads::{self, StringWorkload, StringsShape};
use tafla_benchmarks::workloads::{
    self, DOCUMENT_ROWS, Document, DocumentShape, LIST_ITEMS, List, ListShape, RECORD, RecordShape,
    SAMPLE, SampleShape, Shape, StructsShape,
};

const NAME_WIDTH: usize = 16;
const COLUMN_WIDTH: usize = 12;

/// The width of the strings' shapes' names
const SHAPE_WIDTH: usize = 30;

fn main() -> Result<(), Error> {
    if std::env::args()
        .skip(1)
        .any(|argument| argument == "--strings")
    {
        return print_string_sizes(&string_workloads::string_workloads());
    }

    let documents = DOCUMENT_ROWS.map(workloads::document);
    let list = workloads::list(LIST_ITEMS);

    print_sizes(&documents, &list)?;
    println!();
    print_truncation(&documents[documents.len() - 1])
}

fn print_sizes(documents: &[Document<'_>], list: &List) -> Result<(), Error> {
    let mut columns = vec!["record".to_string()];
    columns.extend(DOCUMENT_ROWS.map(|rows| format!("doc {rows}")));
    columns.extend(["vector", "structs", "fixed"].map(String::from));

    println!("Encoded size (bytes)");
    print_row("", &columns);

    for_each_codec!(|codec| print_row(codec_name(codec), &sizes(codec, documents, list)?));

    // to_le_bytes encodes only the fixed-size record
    let mut cells = vec![String::new(); columns.len() - 1];
    cells.push(size::<SampleShape, _>(LeBytes, &SAMPLE)?);
    print_row(LeBytes::NAME, &cells);

    Ok(())
}

fn sizes<C: Suite>(
    codec: C,
    documents: &[Document<'_>],
    list: &List,
) -> Result<Vec<String>, Error> {
    let mut sizes = vec![size::<RecordShape, C>(codec, &RECORD)?];
    for document in documents {
        sizes.push(size::<DocumentShape, C>(codec, document)?);
    }
    sizes.push(size::<ListShape, C>(codec, list)?);
    sizes.push(size::<StructsShape, C>(codec, list)?);
    sizes.push(size::<SampleShape, C>(codec, &SAMPLE)?);

    Ok(sizes)
}

fn size<S: Shape, C: Workload<S>>(codec: C, input: &S::Value<'_>) -> Result<String, Error> {
    Ok(codec.serialize(input)?.as_ref().len().to_string())
}

/// Prints a row of each codec's encoded sizes for each shape of strings
fn print_string_sizes(workloads: &[StringWorkload]) -> Result<(), Error> {
    println!("Encoded size of 16 strings (bytes)");
    print!("  {:SHAPE_WIDTH$}{:>COLUMN_WIDTH$}", "", "each string");
    for_each_codec!(|codec| print!("{:>COLUMN_WIDTH$}", codec_name(codec)));
    println!();

    for workload in workloads {
        let input = workload.input();

        print!(
            "  {:SHAPE_WIDTH$}{:>COLUMN_WIDTH$}",
            workload.name,
            workload.bytes()
        );
        for_each_codec!(|codec| print!(
            "{:>COLUMN_WIDTH$}",
            size::<StringsShape, _>(codec, &input)?
        ));
        println!();
    }

    Ok(())
}

fn print_row(name: &str, cells: &[String]) {
    print!("  {name:NAME_WIDTH$}");
    for cell in cells {
        print!("{cell:>COLUMN_WIDTH$}");
    }
    println!();
}

/// Deserializes copies of a document cut short, as a dropped connection would
/// leave it
fn print_truncation(document: &Document<'_>) -> Result<(), Error> {
    println!(
        "Truncated document of {} rows: deserialize()",
        document.a.len()
    );

    for_each_codec!(|codec| print_truncated(codec, document)?);

    Ok(())
}

fn print_truncated<C: Workload<DocumentShape>>(
    codec: C,
    document: &Document<'_>,
) -> Result<(), Error> {
    let bytes = codec.serialize(document)?;
    let bytes = bytes.as_ref();

    for length in [bytes.len() / 2, 24] {
        // A copy, since a view could read on into the full bytes, aligned as
        // rkyv needs
        let mut truncated = AlignedVec::<16>::with_capacity(length);
        truncated.extend_from_slice(&bytes[..length]);

        let outcome = describe_outcome(codec, &truncated, document.a.len());
        println!("  {:NAME_WIDTH$}{length:>8} B  {outcome}", C::NAME);
    }

    Ok(())
}

fn describe_outcome<C: Workload<DocumentShape>>(codec: C, bytes: &[u8], rows: usize) -> String {
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        codec.deserialize(bytes).map(|document| document.a.len())
    }));

    match outcome {
        Ok(Ok(length)) => format!("returned {length} of {rows} values"),
        Ok(Err(error)) => format!("returned an error: {}", first_line(&error.to_string(), 60)),
        Err(_) => "panicked".to_string(),
    }
}

/// The start of a message's first line, as the other languages print them
fn first_line(message: &str, length: usize) -> String {
    let line = message.lines().next().unwrap_or_default();
    line.chars()
        .take(length)
        .collect::<String>()
        .trim_end()
        .to_string()
}

const fn codec_name<C: Codec>(_: C) -> &'static str {
    C::NAME
}
