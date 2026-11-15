//! Times main's `TableBuilder` against the rewrite, and its variants, on
//! the comparison's rows that build tables: the record, the documents of
//! 100 and 100,000 rows, the vector of 100 nested tables, in place and from
//! builders, the array of 100 structs, and a table of 32 medium values.
//!
//! Every builder writes each row in a loop of its own, after a check that it
//! writes the tafla codec's bytes, which the codec reads back, or main's
//! builder's bytes of the medium values. Rounds run
//! the builders of each row in rotating order, reversed every other round,
//! after warm-up rounds, and the fastest round's time per call is reported,
//! with the median. Each builder runs twice, as an A/A pair.
//!
//! Options: `--rounds <n>` (30), `--warm-up <n>` (5) and `--sample-ms <n>`
//! (10), the time that each builder's loop takes in each round.

use std::hint::black_box;
use std::process::Command;
use std::time::{Duration, Instant};

use tafla_benchmarks::codec::{Error, Workload};
use tafla_benchmarks::codecs::Tafla;
use tafla_benchmarks::builders::{self, Builder, Main, Medium, Visit, for_each_builder};
use tafla_benchmarks::workloads::{
    self, Document, DocumentShape, LIST_ITEMS, List, ListShape, RECORD, Record, RecordShape, Shape,
    StructsShape,
};

const ROWS: [&str; 7] = [
    "record: serialize",
    "document of 100 rows: serialize",
    "document of 100,000 rows: serialize",
    "vector of 100 nested tables: serialize",
    "array of 100 structs: serialize",
    "vector of 100 nested tables from builders: serialize",
    "table of 32 medium values: serialize",
];

/// The workloads' inputs, which every timing loop borrows
struct Inputs {
    record: Record<'static>,
    small: Document<'static>,
    large: Document<'static>,
    list: List,
    medium: Medium,
}

/// A loop that calls a builder's function on a row's input, as many times
/// as it's given, and returns how long that took
type Loop = Box<dyn FnMut(u64) -> Duration>;

/// A builder's loop of one row, and its times
struct Timer {
    builder: &'static str,
    row: usize,
    run: Loop,
    iterations: u64,
    /// Nanoseconds per call in each timed round
    times: Vec<f64>,
}

struct Options {
    rounds: usize,
    warm_up: usize,
    sample: Duration,
}

fn main() -> Result<(), Error> {
    let options = options()?;
    let inputs: &'static Inputs = Box::leak(Box::new(Inputs {
        record: RECORD,
        small: workloads::document(100),
        large: workloads::document(100_000),
        list: workloads::list(LIST_ITEMS),
        medium: builders::medium(),
    }));

    println!("{}", load_average());
    let mut timers = Timers {
        inputs,
        added: Vec::new(),
        error: None,
    };
    for_each_builder(&mut timers);
    if let Some(error) = timers.error {
        return Err(error);
    }
    let mut timers = timers.added;

    for timer in &mut timers {
        timer.iterations = calibrate(&mut timer.run, options.sample);
    }

    for round in 0..options.warm_up + options.rounds {
        for row in 0..ROWS.len() {
            let mut order: Vec<usize> = (0..timers.len())
                .filter(|&index| timers[index].row == row)
                .collect();
            let shift = round % order.len();
            order.rotate_left(shift);
            if round % 2 == 1 {
                order.reverse();
            }

            for index in order {
                let timer = &mut timers[index];
                let elapsed = (timer.run)(timer.iterations);
                if round >= options.warm_up {
                    timer.times.push(per_call(elapsed, timer.iterations) * 1e9);
                }
            }
        }
    }

    println!("{}", load_average());
    print_report(&mut timers);

    Ok(())
}

/// Adds each builder's loops, after checking that it writes each row as
/// `TableBuilder` does
struct Timers {
    inputs: &'static Inputs,
    added: Vec<Timer>,
    error: Option<Error>,
}

impl Visit for Timers {
    fn visit<T: Builder>(&mut self, name: &'static str) {
        if let Err(error) = check::<T>(self.inputs) {
            self.error
                .get_or_insert_with(|| format!("{name}: {error}").into());
            return;
        }

        let inputs = self.inputs;
        let loops: [Loop; 7] = [
            timing_loop(&inputs.record, T::record),
            timing_loop(&inputs.small, T::document),
            timing_loop(&inputs.large, T::document),
            timing_loop(&inputs.list, T::list),
            timing_loop(&inputs.list, T::structs),
            timing_loop(&inputs.list, T::builders),
            timing_loop(&inputs.medium, T::medium),
        ];
        for (row, run) in loops.into_iter().enumerate() {
            self.added.push(Timer {
                builder: name,
                row,
                run,
                iterations: 0,
                times: Vec::new(),
            });
        }
    }
}

/// A loop of calls of `serialize`, compiled for it alone, which keeps each
/// result until it's dropped
fn timing_loop<I: 'static, F>(input: &'static I, serialize: F) -> Loop
where
    F: Fn(&I) -> Result<Vec<u8>, Error> + 'static,
{
    Box::new(move |iterations| {
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(serialize(black_box(input)).ok());
        }
        start.elapsed()
    })
}

/// Checks that the builder writes the tafla codec's bytes of every row, and
/// that the codec reads them back
fn check<T: Builder>(inputs: &Inputs) -> Result<(), Error> {
    let record: Vec<u8> = Workload::<RecordShape>::serialize(Tafla, &inputs.record)?;
    same::<RecordShape>(&T::record(&inputs.record)?, &record)?;
    for document in [&inputs.small, &inputs.large] {
        let bytes = T::document(document)?;
        let expected: Vec<u8> = Workload::<DocumentShape>::serialize(Tafla, document)?;
        same::<DocumentShape>(&bytes, &expected)?;
    }
    let list: Vec<u8> = Workload::<ListShape>::serialize(Tafla, &inputs.list)?;
    same::<ListShape>(&T::list(&inputs.list)?, &list)?;
    same::<ListShape>(&T::builders(&inputs.list)?, &list)?;
    same::<StructsShape>(
        &T::structs(&inputs.list)?,
        &Workload::<StructsShape>::serialize(Tafla, &inputs.list)?,
    )?;

    if T::medium(&inputs.medium)? != Main::medium(&inputs.medium)? {
        return Err("wrote other bytes of the medium values".into());
    }
    Ok(())
}

fn same<S: Shape>(bytes: &[u8], expected: &[u8]) -> Result<(), Error>
where
    Tafla: Workload<S>,
{
    if bytes != expected {
        return Err(format!("wrote other bytes of the {}", S::NAME).into());
    }

    Tafla.deserialize(bytes)?;
    Ok(())
}

/// Seconds per call of `iterations` calls that took `elapsed`
#[allow(clippy::cast_precision_loss)]
fn per_call(elapsed: Duration, iterations: u64) -> f64 {
    elapsed.as_secs_f64() / iterations as f64
}

/// The iterations that take about `sample`, at least one
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn calibrate(run: &mut Loop, sample: Duration) -> u64 {
    let mut iterations = 1;
    loop {
        let elapsed = run(iterations);
        if elapsed >= sample / 4 {
            let fitted = sample.as_secs_f64() / per_call(elapsed, iterations);
            return (fitted as u64).max(1);
        }
        iterations *= 2;
    }
}

fn print_report(timers: &mut [Timer]) {
    for (row, name) in ROWS.iter().enumerate() {
        println!("\n{name}: ns per call, fastest and median of each builder's rounds");

        let reference = fastest_of(timers, row, "main");
        for timer in timers.iter_mut().filter(|timer| timer.row == row) {
            timer.times.sort_by(f64::total_cmp);
            let fastest = timer.times.first().copied().unwrap_or(f64::NAN);
            let median = timer.times.get(timer.times.len() / 2).copied();
            let median = median.unwrap_or(f64::NAN);
            println!(
                "  {:22} {:>12.1} {:>12.1} {:>8.3}×",
                timer.builder,
                fastest,
                median,
                fastest / reference
            );
        }
    }
}

fn fastest_of(timers: &[Timer], row: usize, builder: &str) -> f64 {
    timers
        .iter()
        .filter(|timer| timer.row == row && timer.builder == builder)
        .flat_map(|timer| timer.times.iter().copied())
        .fold(f64::INFINITY, f64::min)
}

/// The machine's load averages, as `sysctl` prints them, or `uptime` on
/// other systems
fn load_average() -> String {
    let output = Command::new("sysctl")
        .args(["-n", "vm.loadavg"])
        .output()
        .or_else(|_| Command::new("uptime").output());
    let load = output.map_or_else(
        |error| error.to_string(),
        |output| String::from_utf8_lossy(&output.stdout).trim().to_owned(),
    );

    format!("load averages: {load}")
}

fn options() -> Result<Options, Error> {
    let mut options = Options {
        rounds: 30,
        warm_up: 5,
        sample: Duration::from_millis(10),
    };

    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{argument} takes a value"))?;
        match argument.as_str() {
            "--rounds" => options.rounds = value.parse()?,
            "--warm-up" => options.warm_up = value.parse()?,
            "--sample-ms" => options.sample = Duration::from_millis(value.parse()?),
            _ => return Err(format!("unknown option {argument}").into()),
        }
    }

    Ok(options)
}
