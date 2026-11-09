import { GCProfiler, type GCProfilerResult, getHeapStatistics } from 'node:v8';

export interface Variant {
    name: string;
    run: () => unknown;
}

export interface Measurement {
    name: string;
    /** The fastest call over all rounds, in microseconds */
    microseconds: number;
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

type Timer = (run: () => unknown, repetitions: number) => number;

let timerId = 0;

/** Samples of each variant's heap bytes, which run after its timed rounds */
const HEAP_SAMPLES = 5;

/** What a sample of heap bytes allocates itself, with a timer of nothing */
const SAMPLE_BYTES = fewestHeapBytes(createTimer(), () => undefined, 1, 20);

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
        const warm = fastestRounds(variants, timers, repetitions, warmupRounds);
        repetitions = warm.map(time => repetitionsOf(time, sampleTime));
    }

    const best = fastestRounds(variants, timers, repetitions, rounds);

    return variants.map(({ name, run }, i) => {
        const bytes = fewestHeapBytes(timers[i], run, repetitions[i]);

        return {
            name,
            microseconds: best[i],
            heapBytes: (bytes - SAMPLE_BYTES) / repetitions[i]
        };
    });
}

/**
 * Runs `rounds` rounds of the variants, each once a round, in alternating
 * order, and returns each one's fastest call, in microseconds
 */
function fastestRounds(
    variants: Variant[],
    timers: Timer[],
    repetitions: number[],
    rounds: number
): number[] {
    const best = variants.map(() => Number.POSITIVE_INFINITY);

    for (let round = 0; round < rounds; round++) {
        for (let j = 0; j < variants.length; j++) {
            const i = round % 2 === 0 ? j : variants.length - 1 - j;
            const time = timers[i](variants[i].run, repetitions[i]);

            best[i] = Math.min(best[i], time);
        }
    }

    return best;
}

/**
 * The fewest bytes that `repetitions` calls of `run` allocated on V8's heap
 * over `samples` samples: how much the heap's used size grew, and what the
 * garbage collections meanwhile freed, which a `GCProfiler` reports. The
 * fewest, since compiling and deoptimizing allocate too.
 */
function fewestHeapBytes(
    timer: Timer,
    run: () => unknown,
    repetitions: number,
    samples = HEAP_SAMPLES
): number {
    let fewest = Number.POSITIVE_INFINITY;
    for (let sample = 0; sample < samples; sample++) {
        const profiler = new GCProfiler();
        profiler.start();
        const before = getHeapStatistics().used_heap_size;
        timer(run, repetitions);
        const after = getHeapStatistics().used_heap_size;
        const freed = freedBytes(profiler.stop());

        fewest = Math.min(fewest, after - before + freed);
    }

    return fewest;
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
