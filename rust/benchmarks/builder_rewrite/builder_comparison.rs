//! Times main's `TableBuilder` against the rewrite, and its variants, on
//! the rows of `builders::for_each_row()`: the comparison's rows that build
//! tables, the vector of 100 nested tables from builders, tables of 32
//! medium and 32 short values, and the shapes of the second review.
//!
//! Every builder writes each row in a loop of its own, after a check that it
//! writes main's builder's bytes, and of the comparison's rows, the tafla
//! codec's bytes, which the codec reads back. Rounds run the builders of
//! each row in rotating order, reversed every other round, after warm-up
//! rounds, and the fastest round's time per call is reported, with the
//! median. Main's builder and the rewrite run twice, as A/A pairs.
//!
//! Options: `--rounds <n>` (30), `--warm-up <n>` (5) and `--sample-ms <n>`
//! (10), the time that each builder's loop takes in each round.

use std::hint::black_box;
use std::process::Command;
use std::time::{Duration, Instant};

use tafla_benchmarks::builders::{Builder, Inputs, Rows, Visit, for_each_builder, for_each_row};
use tafla_benchmarks::codec::{Error, Workload};
use tafla_benchmarks::codecs::Tafla;
use tafla_benchmarks::workloads::{DocumentShape, ListShape, RecordShape, Shape, StructsShape};

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
    let inputs: &'static Inputs = Box::leak(Box::new(Inputs::new()));

    println!("{}", load_average());
    let mut timers = Timers {
        inputs,
        rows: Vec::new(),
        expected: Vec::new(),
        added: Vec::new(),
        error: None,
    };
    for_each_builder(&mut timers);
    if let Some(error) = timers.error {
        return Err(error);
    }
    let rows = timers.rows;
    drop(timers.expected);
    let mut timers = timers.added;

    for timer in &mut timers {
        timer.iterations = calibrate(&mut timer.run, options.sample);
    }

    for round in 0..options.warm_up + options.rounds {
        for row in 0..rows.len() {
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
    print_report(&rows, &mut timers);

    Ok(())
}

/// Adds each builder's loops, after checking that it writes each row as
/// main's builder does, which it visits first
struct Timers {
    inputs: &'static Inputs,
    rows: Vec<&'static str>,
    /// Main's bytes of each row
    expected: Vec<Vec<u8>>,
    added: Vec<Timer>,
    error: Option<Error>,
}

impl Visit for Timers {
    fn visit<T: Builder>(&mut self, name: &'static str) {
        let inputs = self.inputs;
        if let Err(error) = check::<T>(inputs) {
            self.error
                .get_or_insert_with(|| format!("{name}: {error}").into());
            return;
        }

        let mut adder = Adder {
            builder: name,
            row: 0,
            timers: self,
        };
        for_each_row::<T>(inputs, &mut adder);
    }
}

/// Adds a builder's loop of each row
struct Adder<'t> {
    builder: &'static str,
    row: usize,
    timers: &'t mut Timers,
}

impl Rows for Adder<'_> {
    fn row<I, F>(&mut self, name: &'static str, input: &'static I, write: F)
    where
        I: 'static,
        F: Fn(&I) -> Result<Vec<u8>, Error> + Copy + 'static,
    {
        let timers = &mut *self.timers;
        match write(input) {
            Ok(bytes) if self.row == timers.expected.len() => {
                timers.rows.push(name);
                timers.expected.push(bytes);
            }
            Ok(bytes) if bytes == timers.expected[self.row] => {}
            Ok(_) => {
                let error = format!("{}: wrote other bytes of the {name}", self.builder);
                timers.error.get_or_insert_with(|| error.into());
            }
            Err(error) => {
                let error = format!("{}: {name}: {error}", self.builder);
                timers.error.get_or_insert_with(|| error.into());
            }
        }

        timers.added.push(Timer {
            builder: self.builder,
            row: self.row,
            run: timing_loop(input, write),
            iterations: 0,
            times: Vec::new(),
        });
        self.row += 1;
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

/// Checks that the builder writes the tafla codec's bytes of the
/// comparison's rows, and that the codec reads them back
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
    )
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

fn print_report(rows: &[&str], timers: &mut [Timer]) {
    for (row, name) in rows.iter().enumerate() {
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
