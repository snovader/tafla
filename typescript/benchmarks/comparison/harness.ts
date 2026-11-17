import { parseArgs } from 'node:util';
import { getHeapStatistics, GCProfiler, type GCProfilerResult } from 'node:v8';

export interface Variant {
    name: string;
    run: () => unknown;
}

/** Variants that `measure()` times together, which a script prints as one */
export interface Group {
    name: string;
    variants: Variant[];
}

export interface Measurement {
    name: string;
    /** The fastest round's time per call, in microseconds */
    fastest: number;
    /**
     * The median round's time per call, in microseconds, which load from
     * other processes moves away from the fastest. Of an even count of
     * rounds, it's the slower of the middle two, as in Go's and C++'s
     * harnesses.
     */
    median: number;
    /**
     * The fewest bytes that a call allocated on V8's heap over a few samples
     * after the rounds. V8 allocates the contents of an `ArrayBuffer` of more
     * than 64 bytes outside its heap, so they're left out.
     */
    heapBytes: number;
}

export interface MeasureOptions {
    rounds: number;
    warmupRounds: number;
    /** The time that one round of one variant should take, in milliseconds */
    sampleTime: number;
}

/** The options of a script that measures groups of variants */
export interface RunOptions extends MeasureOptions {
    /** Measures only the groups whose name matches */
    filter: RegExp;
}

type Timer = (run: () => unknown, repetitions: number) => number;

let timerId = 0;

/** Samples of each variant's heap bytes, which run after its timed rounds */
const HEAP_SAMPLES = 5;

/** What a sample of heap bytes allocates itself */
const SAMPLE_BYTES = sampleBytes();

/**
 * Reads a script's options from its command-line arguments: `--rounds`,
 * 31 by default, `--sample-ms`, the time of one round of one variant in
 * milliseconds, 1 by default, and `--filter`, a regular expression that
 * selects groups by name. Every run has 10 warm-up rounds.
 *
 * @throws {TypeError} If an argument isn't one of the options, or an option
 * has no value.
 * @throws {Error} If the rounds aren't a positive integer, or the time of a
 * round isn't a positive number.
 */
export function runOptions(args: readonly string[]): RunOptions {
    const { values } = parseArgs({
        args,
        options: {
            rounds: { type: 'string', default: '31' },
            'sample-ms': { type: 'string', default: '1' },
            filter: { type: 'string', default: '' }
        }
    });
    const rounds = Number(values.rounds);
    const sampleTime = Number(values['sample-ms']);

    if (!Number.isInteger(rounds) || rounds < 1) {
        throw new Error('Expected --rounds to be a positive integer');
    }
    if (!(sampleTime > 0)) {
        throw new Error('Expected --sample-ms to be a positive number');
    }

    return {
        rounds,
        warmupRounds: 10,
        sampleTime,
        filter: new RegExp(values.filter)
    };
}

/** A line that describes the runtime and what each row of a run reports */
export function describeRun({
    rounds,
    warmupRounds,
    sampleTime
}: MeasureOptions): string {
    return `Node ${process.version}, V8 ${process.versions.v8}; per call, the fastest and median of ${rounds} interleaved rounds of ${sampleTime} ms after ${warmupRounds} warm-up rounds, the fastest's ratio to the group's first, and the bytes allocated on V8's heap`;
}

/** Measures each group whose name matches the filter, and prints it */
export function measureGroups(
    groups: Group[],
    options: RunOptions,
    print: (line: string) => void
): void {
    for (const { name, variants } of groups) {
        if (options.filter.test(name)) {
            printMeasurements(name, measure(variants, options), print);
        }
    }
}

/**
 * Times the variants interleaved: every round runs each of them once, in
 * alternating order, and the fastest round counts. Load from other processes
 * then slows every variant alike, and only for some rounds. Each variant gets
 * its own timer loop, compiled from a unique source, so that its call site
 * and type feedback aren't shared with the others.
 *
 * A round of a variant repeats its call for about `sampleTime`. The
 * repetitions are counted from the variant's first calls, and then again from
 * its fastest warm-up round, since the code that V8 optimizes during the
 * warm-up runs several times as fast as the first calls.
 *
 * It reports the median round too, which load moves away from the fastest.
 * Then it counts the bytes that each variant's calls allocate on V8's heap,
 * with the same timer loops, in samples apart from the timed rounds.
 */
export function measure(
    variants: Variant[],
    { rounds, warmupRounds, sampleTime }: MeasureOptions
): Measurement[] {
    const timers = variants.map(createTimer);
    let repetitions = variants.map(variant => calibrate(variant, sampleTime));

    if (warmupRounds > 0) {
        const warm = timeRounds(variants, timers, repetitions, warmupRounds);
        repetitions = warm.map(([fastest]) =>
            repetitionsOf(fastest, sampleTime)
        );
    }

    const times = timeRounds(variants, timers, repetitions, rounds);

    return variants.map(({ name, run }, i) => {
        const [bytes] = heapBytes(timers[i], run, repetitions[i]);

        return {
            name,
            fastest: times[i][0],
            median: times[i][Math.floor(rounds / 2)],
            heapBytes: (bytes - SAMPLE_BYTES) / repetitions[i]
        };
    });
}

/**
 * Runs `rounds` rounds of the variants, each once a round, in alternating
 * order, and returns each one's time per call in every round, in
 * microseconds, from the fastest
 */
function timeRounds(
    variants: Variant[],
    timers: Timer[],
    repetitions: number[],
    rounds: number
): Float64Array[] {
    const times = variants.map(() => new Float64Array(rounds));

    for (let round = 0; round < rounds; round++) {
        for (let j = 0; j < variants.length; j++) {
            const i = round % 2 === 0 ? j : variants.length - 1 - j;
            times[i][round] = timers[i](variants[i].run, repetitions[i]);
        }
    }

    for (const variantTimes of times) {
        variantTimes.sort();
    }

    return times;
}

/**
 * The bytes that `repetitions` calls of `run` allocated on V8's heap in each
 * of `samples` samples, from the fewest: how much the heap's used size grew,
 * and what the garbage collections meanwhile freed, which a `GCProfiler`
 * reports. A variant's fewest counts, since compiling and deoptimizing
 * allocate too.
 */
function heapBytes(
    timer: Timer,
    run: () => unknown,
    repetitions: number,
    samples = HEAP_SAMPLES
): Float64Array {
    const bytes = new Float64Array(samples);
    for (let sample = 0; sample < samples; sample++) {
        const profiler = new GCProfiler();
        profiler.start();
        const before = getHeapStatistics().used_heap_size;
        timer(run, repetitions);
        const after = getHeapStatistics().used_heap_size;
        const freed = freedBytes(profiler.stop());

        bytes[sample] = after - before + freed;
    }

    bytes.sort();
    return bytes;
}

/**
 * What a sample of heap bytes allocates itself, with a timer of nothing: the
 * median of 20 samples, rather than the fewest, since the used size can
 * shrink in a sample just after startup. In one process, one sample gave
 * -22,896 bytes against 880 for the others, which added 77 bytes to each
 * call of a getter that allocates none.
 */
function sampleBytes(): number {
    const bytes = heapBytes(createTimer(), () => undefined, 1, 20);
    return bytes[bytes.length / 2];
}

function freedBytes({ statistics }: GCProfilerResult): number {
    let freed = 0;
    for (const { beforeGC, afterGC } of statistics) {
        freed +=
            beforeGC.heapStatistics.usedHeapSize -
            afterGC.heapStatistics.usedHeapSize;
    }

    return freed;
}

function createTimer(): Timer {
    // Identical sources would share feedback through V8's compilation cache
    // oxlint-disable-next-line typescript/no-implied-eval -- A timing loop of its own for each variant
    return new Function(
        'run',
        'repetitions',
        `let result;
        const start = performance.now();
        for (let i = 0; i < repetitions; i++) {
            result = run();
        }
        const elapsed = performance.now() - start;
        globalThis.benchmarkSink = result;
        return (elapsed / repetitions) * 1000; // timer ${timerId++}`
    ) as Timer;
}

/**
 * The repetitions that make one sample of the variant's first calls take
 * about `sampleTime`
 */
function calibrate({ run }: Variant, sampleTime: number): number {
    let repetitions = 1;
    for (;;) {
        const start = performance.now();
        for (let i = 0; i < repetitions; i++) {
            run();
        }
        const elapsed = performance.now() - start;

        if (elapsed >= sampleTime / 4 || repetitions >= 1 << 20) {
            return repetitionsOf((elapsed / repetitions) * 1000, sampleTime);
        }

        repetitions *= 2;
    }
}

/**
 * The repetitions of a call of `microseconds` that take about `sampleTime`,
 * in milliseconds
 */
function repetitionsOf(microseconds: number, sampleTime: number): number {
    return Math.max(
        1,
        Math.round((sampleTime * 1000) / Math.max(microseconds, 1e-6))
    );
}

/**
 * Prints a group's measurements: each variant's fastest and median call, the
 * fastest's ratio to the first variant's, and its bytes on V8's heap
 */
function printMeasurements(
    group: string,
    measurements: Measurement[],
    print: (line: string) => void
): void {
    const [reference] = measurements;

    print(`\n${group}`);
    print(formatRow('', 'fastest', 'median', 'ratio', 'heap'));
    for (const { name, fastest, median, heapBytes } of measurements) {
        const ratio = fastest / reference.fastest;

        print(
            formatRow(
                name,
                `${formatMicroseconds(fastest)} µs`,
                `${formatMicroseconds(median)} µs`,
                `${ratio.toFixed(2)}×`,
                formatBytes(heapBytes)
            )
        );
    }
}

function formatRow(
    name: string,
    fastest: string,
    median: string,
    ratio: string,
    heap: string
): string {
    return `  ${name.padEnd(18)} ${fastest.padStart(12)} ${median.padStart(12)} ${ratio.padStart(8)} ${heap.padStart(12)}`;
}

function formatMicroseconds(microseconds: number): string {
    return microseconds.toFixed(microseconds < 10 ? 3 : 1);
}

/** Bytes allocated on V8's heap per call, rounded to whole bytes */
function formatBytes(bytes: number): string {
    return `${Math.max(0, Math.round(bytes)).toLocaleString('en-US')} B`;
}
