//! Times how tafla's runtime and the comparison's other crates write and read
//! strings, as README.md's Strings section describes: tables of 16 distinct
//! strings of one shape and nothing else, so that a call spends most of its
//! time on their UTF-8. `cargo run --bin report` then prints each
//! benchmark's fastest sample.
//!
//! Each group times every codec, and then tafla's codec again, as an A/A pair
//! of the same code. `STRINGS_REVERSE=1` runs each group's codecs in
//! reverse order, so that rounds saved with `--save-baseline` alternate it.

use std::env;
use std::hint::black_box;
use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, criterion_group, criterion_main};
use tafla_benchmarks::check;
use tafla_benchmarks::codec::{Codec, Error, Workload};
use tafla_benchmarks::codecs::{Tafla, TaflaGen};
use tafla_benchmarks::for_each_codec;
use tafla_benchmarks::string_workloads::{self, Strings, StringsShape};

type Group<'a> = BenchmarkGroup<'a, WallTime>;

/// A codec's benchmark, which it adds to a group when it's called
type Benchmark<'a> = Box<dyn FnOnce(&mut Group<'_>) + 'a>;

criterion_group! {
    name = strings;
    // The comparison's configuration, as `benches/comparison.rs` explains it
    config = Criterion::default()
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(1))
        .sample_size(50)
        .nresamples(1000);
    targets = shapes
}

criterion_main!(strings);

/// Each codec's benchmark of `$time`, `serialize` or `deserialize`, in the
/// order that results list them, and tafla's A/A pair last
macro_rules! benchmarks {
    ($time:ident, $input:expr) => {{
        let input: &Strings<'_> = $input;
        let mut benchmarks: Vec<Benchmark<'_>> = Vec::new();

        for_each_codec!(
            |codec| benchmarks.push(Box::new(move |group: &mut Group<'_>| {
                $time(group, codec, input);
            }))
        );
        benchmarks.push(Box::new(move |group: &mut Group<'_>| {
            $time(group, TaflaAa, input);
        }));

        benchmarks
    }};
}

fn shapes(criterion: &mut Criterion) {
    let reverse = env::var_os("STRINGS_REVERSE").is_some_and(|value| value == "1");

    for workload in string_workloads::string_workloads() {
        let input = workload.input();
        checked(check_same_tables(&input));

        let name = format!("16 × {}", workload.name);
        group(
            criterion,
            &format!("{name}: serialize"),
            reverse,
            benchmarks!(serialize, &input),
        );
        group(
            criterion,
            &format!("{name}: deserialize"),
            reverse,
            benchmarks!(deserialize, &input),
        );
    }
}

/// tafla's codec, timed again: an A/A pair that shows the harness's noise
/// between two timings of the same code. It delegates to `Tafla`, so the
/// compiler is free to merge its loops with tafla's, as rustc 1.100 merges
/// the writes', and the pair doesn't show what a loop's address adds to a
/// time.
#[derive(Clone, Copy, Debug)]
struct TaflaAa;

impl Codec for TaflaAa {
    const NAME: &'static str = "tafla A/A";
}

impl Workload<StringsShape> for TaflaAa {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &Strings<'_>) -> Result<Vec<u8>, Error> {
        <Tafla as Workload<StringsShape>>::serialize(Tafla, input)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Strings<'_>, Error> {
        <Tafla as Workload<StringsShape>>::deserialize(Tafla, bytes)
    }
}

/// Checks that tafla-gen's code writes the table that tafla's builder
/// writes, as the round-trip test checks too
fn check_same_tables(input: &Strings<'_>) -> Result<(), Error> {
    let built = <Tafla as Workload<StringsShape>>::serialize(Tafla, input)?;
    let generated = <TaflaGen as Workload<StringsShape>>::serialize(TaflaGen, input)?;

    if generated == built {
        Ok(())
    } else {
        Err("tafla-gen's code wrote another table of the strings than tafla's builder".into())
    }
}

/// Runs a group of benchmarks, one after another, in reverse when `reverse`
fn group(criterion: &mut Criterion, name: &str, reverse: bool, mut benchmarks: Vec<Benchmark<'_>>) {
    if reverse {
        benchmarks.reverse();
    }

    let mut group = criterion.benchmark_group(name);
    for benchmark in benchmarks {
        benchmark(&mut group);
    }
    group.finish();
}

/// Times writing `input`, after checking that the codec reads it back. The
/// bytes are dropped within the timed loop, as a caller frees them.
fn serialize<C: Workload<StringsShape>>(group: &mut Group<'_>, codec: C, input: &Strings<'_>) {
    checked(check::round_trip::<StringsShape, C>(codec, input));

    group.bench_function(C::NAME, |bencher| {
        bencher.iter(|| codec.serialize(black_box(input)));
    });
}

/// Times reading every string of the bytes that the codec wrote of `input`
fn deserialize<C: Workload<StringsShape>>(group: &mut Group<'_>, codec: C, input: &Strings<'_>) {
    checked(check::round_trip::<StringsShape, C>(codec, input));
    let bytes = checked(codec.serialize(input));

    group.bench_function(C::NAME, |bencher| {
        bencher.iter(|| codec.deserialize(black_box(bytes.as_ref())));
    });
}

/// Stops the benchmarks before timing a codec that failed its check
fn checked<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|error| panic!("{error}"))
}
