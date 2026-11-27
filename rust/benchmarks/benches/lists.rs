//! Times how tafla-gen's code writes and reads lists of 10, 100 and 10,000
//! strings, as a vector of strings and as a vector of tables of one string
//! each, as README.md describes. Its results go to `criterion-lists` in the
//! target directory, apart from the comparison's, and
//! `cargo run --bin report -- target/criterion-lists` prints each
//! benchmark's fastest sample.

use std::env;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, criterion_group, criterion_main};
use tafla_benchmarks::codec::Error;
use tafla_benchmarks::for_each_list_codec;
use tafla_benchmarks::lists::{self, ListCodec, list_shapes};

type Group<'a> = BenchmarkGroup<'a, WallTime>;

criterion_group! {
    name = benches;
    // As the comparison's: 50 samples over a second, which grow linearly
    // from about a millisecond, after a warm-up of half a second
    config = Criterion::default()
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(1))
        .sample_size(50)
        .nresamples(1000)
        .output_directory(&output_directory());
    targets = lists
}

criterion_main!(benches);

/// Writes each shape, reads every string, and reads the last string, with
/// each codec, after checking that it reads back what it wrote
fn lists(criterion: &mut Criterion) {
    for shape in list_shapes() {
        let strings = shape.strs();
        for_each_list_codec!(|codec, _name| checked(lists::check(codec, &shape)));

        group(criterion, &format!("{}: serialize", shape.name), |group| {
            for_each_list_codec!(|codec, name| {
                group.bench_function(name, |bencher| {
                    bencher.iter(|| codec.serialize(black_box(&strings)));
                });
            });
        });
        group(
            criterion,
            &format!("{}: deserialize", shape.name),
            |group| {
                for_each_list_codec!(|codec, name| {
                    let bytes = checked(codec.serialize(&strings));
                    group.bench_function(name, |bencher| {
                        bencher.iter(|| codec.deserialize(black_box(&bytes)));
                    });
                });
            },
        );
        group(
            criterion,
            &format!("{}: read the last string", shape.name),
            |group| {
                for_each_list_codec!(|codec, name| {
                    let bytes = checked(codec.serialize(&strings));
                    group.bench_function(name, |bencher| {
                        bencher.iter(|| codec.read_last(black_box(&bytes)));
                    });
                });
            },
        );
    }
}

/// Runs a group of benchmarks, which `add` adds a codec at a time, so that
/// the codecs run one after another
fn group(criterion: &mut Criterion, name: &str, add: impl FnOnce(&mut Group<'_>)) {
    let mut group = criterion.benchmark_group(name);
    add(&mut group);
    group.finish();
}

/// Where Criterion writes the suite's results: `$CRITERION_HOME`, as it
/// writes the comparison's, or else `criterion-lists` in the target
/// directory, so that the report reads them apart from the comparison's
fn output_directory() -> PathBuf {
    if let Some(home) = env::var_os("CRITERION_HOME") {
        return PathBuf::from(home);
    }

    env::var_os("CARGO_TARGET_DIR")
        .map_or_else(
            || Path::new(env!("CARGO_MANIFEST_DIR")).join("../target"),
            PathBuf::from,
        )
        .join("criterion-lists")
}

/// Stops the benchmarks before timing a codec that failed its check
fn checked<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|error| panic!("{error}"))
}
