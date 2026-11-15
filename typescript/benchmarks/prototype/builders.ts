// Times TableBuilder against the prototype DirectTableBuilder, and each of
// the prototype's variants, on every builder row of the comparison:
//
//   node --conditions=tafla-source benchmarks/prototype/builders.ts \
//       [--rounds 41] [--sample-ms 2] [--filter <regexp>] [--reverse] \
//       [--out <directory>] [--json <file>]
//
// Each variant runs from a copy of its own of the runtime's sources, and of
// the codec in builderCodec.ts, so that V8 keeps the type feedback of each
// apart: TableBuilder twice and the proposed design twice, as A/A pairs. The
// copies go in `--out`, or a new temporary directory. Before timing, every
// variant must write the bytes that the comparison's codec writes.
import { cp, mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { GCProfiler, getHeapStatistics } from 'node:v8';
import type { Options } from '../../core/prototype/directTableBuilder.ts';
import { measure, type Variant } from '../comparison/harness.ts';
import {
    DOCUMENT_ROWS,
    documentInput,
    documentNumbers,
    LIST_ITEMS,
    listInput,
    RECORD
} from '../comparison/workloads.ts';
import type { BuilderCodec } from './builderCodec.ts';

interface VariantSpec {
    name: string;
    /** The prototype's options, or none for TableBuilder */
    options?: Partial<Options>;
}

const VARIANTS: VariantSpec[] = [
    { name: 'TableBuilder' },
    { name: 'TableBuilder A/A' },
    { name: 'direct', options: {} },
    { name: 'direct A/A', options: {} },
    { name: 'direct, new buffers', options: { reuse: false } },
    {
        name: 'direct, copying',
        options: { segmentBytes: Number.POSITIVE_INFINITY }
    },
    { name: 'direct, 256-byte segments', options: { segmentBytes: 256 } },
    { name: 'direct, scalars as bytes', options: { scalars: 'bytes' } },
    {
        name: 'direct, strings counted without room',
        options: { strings: 'room' }
    },
    { name: 'direct, strings counted', options: { strings: 'count' } },
    { name: 'direct, transfer() growth', options: { growth: 'transfer' } },
    { name: 'direct, transfer() result', options: { result: 'transfer' } }
];

const args = process.argv.slice(2);
const rounds = Number(option('--rounds') ?? 41);
const sampleTime = Number(option('--sample-ms') ?? 2);
const filter = new RegExp(option('--filter') ?? '');
const reverse = args.includes('--reverse');

const here = new URL('.', import.meta.url);
const core = new URL('../../core/', import.meta.url);
const out =
    option('--out') ?? (await mkdtemp(join(tmpdir(), 'tafla-builders-')));

const specs = reverse ? [...VARIANTS].reverse() : VARIANTS;
const codecs = await Promise.all(specs.map(spec => load(spec)));

const documents = DOCUMENT_ROWS.map(documentInput);
const numbers = documents.map(documentNumbers);
const list = listInput(LIST_ITEMS);

interface Row {
    name: string;
    run: (codec: BuilderCodec) => () => Uint8Array;
    /** Calls whose off-heap bytes are counted, few enough for no GC */
    calls: number;
}

// The number[] rows last, as the comparison runs them, since they change the
// type feedback of the code that they share with typed arrays
const ROWS: Row[] = [
    { name: 'record: serialize', run: c => () => c.record(RECORD), calls: 200 },
    ...documents.map((input, i) => ({
        name: `document of ${DOCUMENT_ROWS[i].toLocaleString('en')} rows: serialize`,
        run: (c: BuilderCodec) => () => c.document(input),
        calls: i === 0 ? 200 : 4
    })),
    {
        name: `vector of ${LIST_ITEMS} nested tables: serialize`,
        run: c => () => c.list(list),
        calls: 100
    },
    {
        name: `vector of ${LIST_ITEMS} nested tables from builders: serialize`,
        run: c => () => c.listFromBuilders(list),
        calls: 100
    },
    {
        name: `array of ${LIST_ITEMS} structs: serialize`,
        run: c => () => c.structs(list),
        calls: 200
    },
    ...numbers.map((input, i) => ({
        name: `document of ${DOCUMENT_ROWS[i].toLocaleString('en')} rows: serialize number[]`,
        run: (c: BuilderCodec) => () => c.document(input),
        calls: i === 0 ? 200 : 4
    }))
];

console.log(
    `Node ${process.version}, V8 ${process.versions.v8}; fastest of ${rounds} interleaved rounds of ${sampleTime} ms; load ${loadAverage()}; copies in ${out}`
);

const reference = await referenceCodec();
const results: unknown[] = [];

for (const row of ROWS) {
    if (!filter.test(row.name)) {
        continue;
    }

    const runs = codecs.map(codec => row.run(codec));
    checkBytes(row, runs, reference);

    const variants: Variant[] = specs.map(({ name }, i) => ({
        name,
        run: runs[i]
    }));
    const measurements = measure(variants, {
        rounds,
        warmupRounds: 10,
        sampleTime
    });
    const offHeap = runs.map(run => offHeapBytes(run, row.calls));

    const base =
        measurements[specs.findIndex(({ name }) => name === 'TableBuilder')];
    console.log(`\n${row.name}`);
    for (const [
        i,
        { name, microseconds, heapBytes }
    ] of measurements.entries()) {
        const ratio = microseconds / base.microseconds;
        console.log(
            `  ${name.padEnd(44)} ${format(microseconds).padStart(9)} µs ${ratio.toFixed(2).padStart(5)}×  heap ${Math.round(heapBytes).toLocaleString('en').padStart(9)} B  off-heap ${offHeap[i].toLocaleString('en').padStart(11)} B`
        );
        results.push({
            row: row.name,
            name,
            microseconds,
            heapBytes,
            offHeapBytes: offHeap[i]
        });
    }
}

const json = option('--json');
if (json !== undefined) {
    await writeFile(
        json,
        `${JSON.stringify({ load: loadAverage(), reverse, rounds, sampleTime, results })}\n`
    );
}

/**
 * Copies the runtime's sources and the codec into a directory of the
 * variant's own, with the prototype's options set, and loads the codec
 */
async function load({ name, options }: VariantSpec): Promise<BuilderCodec> {
    const directory = join(
        out,
        name.replace(/[^a-zA-Z0-9]+/g, '-').replace(/-$/, '')
    );
    await mkdir(directory, { recursive: true });
    await cp(
        fileURLToPath(new URL('source/', core)),
        join(directory, 'source'),
        { recursive: true }
    );

    let builderImport =
        "import { TableBuilder as Builder } from './source/tableBuilder.ts';";
    if (options !== undefined) {
        await mkdir(join(directory, 'prototype'), { recursive: true });
        const prototype = await readFile(
            new URL('prototype/directTableBuilder.ts', core),
            'utf8'
        );
        await writeFile(
            join(directory, 'prototype', 'directTableBuilder.ts'),
            withOptions(prototype, options)
        );
        builderImport =
            "import { DirectTableBuilder as Builder } from './prototype/directTableBuilder.ts';";
    }

    const template = await readFile(new URL('builderCodec.ts', here), 'utf8');
    const codec = replaceOnce(
        template,
        "import { DirectTableBuilder as Builder } from '../../core/prototype/directTableBuilder.ts';",
        builderImport
    ).replaceAll("'../../core/source/", "'./source/");
    await writeFile(join(directory, 'codec.ts'), codec);

    const module = await import(
        pathToFileURL(join(directory, 'codec.ts')).href
    );
    return module.codec as BuilderCodec;
}

/** The prototype's source with its options' initial values replaced */
function withOptions(source: string, options: Partial<Options>): string {
    const values: Options = {
        growth: 'copy',
        result: 'slice',
        strings: 'short',
        scalars: 'view',
        segmentBytes: 1024,
        reuse: true,
        ...options
    };
    const segmentBytes = Number.isFinite(values.segmentBytes)
        ? String(values.segmentBytes)
        : 'Number.POSITIVE_INFINITY';

    let replaced = replaceOnce(
        source,
        "let growth: Growth = 'copy';",
        `let growth: Growth = '${values.growth}';`
    );
    replaced = replaceOnce(
        replaced,
        "let result: Result = 'slice';",
        `let result: Result = '${values.result}';`
    );
    replaced = replaceOnce(
        replaced,
        "let stringRoom: StringRoom = 'short';",
        `let stringRoom: StringRoom = '${values.strings}';`
    );
    replaced = replaceOnce(
        replaced,
        "let scalars: Scalars = 'view';",
        `let scalars: Scalars = '${values.scalars}';`
    );
    replaced = replaceOnce(
        replaced,
        'let segmentBytes = 1024;',
        `let segmentBytes = ${segmentBytes};`
    );
    return replaceOnce(
        replaced,
        'let reuse = true;',
        `let reuse = ${values.reuse};`
    );
}

function replaceOnce(source: string, from: string, to: string): string {
    if (source.split(from).length !== 2) {
        throw new Error(`Expected one ${from}`);
    }

    return source.replace(from, () => to);
}

/**
 * The comparison's own codec, whose bytes every variant must write. It
 * imports `tafla`, which a frozen copy of the benchmarks may not resolve.
 */
async function referenceCodec(): Promise<BuilderCodec | undefined> {
    try {
        const { tafla } = await import('../comparison/codecs/tafla.ts');
        return {
            record: input => tafla.record.serialize(input),
            document: input => tafla.document.serialize(input),
            list: input => tafla.list.serialize(input),
            structs: input => tafla.structs.serialize(input),
            // The same bytes as the vector written in place
            listFromBuilders: input => tafla.list.serialize(input)
        };
    } catch (error) {
        console.log(
            `The comparison's codec didn't load, so variants are checked against TableBuilder's copy: ${error}`
        );
        return undefined;
    }
}

/** @throws {Error} If a variant writes other bytes than the reference's. */
function checkBytes(
    row: Row,
    runs: Array<() => Uint8Array>,
    reference: BuilderCodec | undefined
): void {
    const expected =
        reference === undefined
            ? runs[specs.findIndex(({ options }) => options === undefined)]()
            : row.run(reference)();

    for (const [i, run] of runs.entries()) {
        const bytes = run();
        if (
            !(
                bytes.length === expected.length &&
                bytes.every((byte, j) => byte === expected[j])
            )
        ) {
            throw new Error(
                `${specs[i].name} wrote other bytes for ${row.name}`
            );
        }
        if (
            bytes.buffer.byteLength !== bytes.length ||
            bytes.byteOffset !== 0
        ) {
            throw new Error(
                `${specs[i].name} wrote bytes that aren't a buffer of their own for ${row.name}`
            );
        }
    }
}

/**
 * The bytes that a call allocated outside V8's heap, for buffers of more
 * than 64 bytes: how much V8's external memory grew over `calls` calls, the
 * median of samples without a garbage collection, which frees it. V8 frees
 * the buffers of an earlier collection in the background, which can shrink a
 * sample, and a buffer transferred is freed at once, so it's left out.
 */
function offHeapBytes(run: () => Uint8Array, calls: number): number {
    const samples: number[] = [];

    for (let attempt = 0; attempt < 60 && samples.length < 9; attempt++) {
        const profiler = new GCProfiler();
        profiler.start();
        const before = getHeapStatistics().external_memory;
        for (let call = 0; call < calls; call++) {
            run();
        }
        const after = getHeapStatistics().external_memory;

        if (profiler.stop().statistics.length === 0) {
            samples.push((after - before) / calls);
        }
    }

    samples.sort((a, b) => a - b);
    return Math.round(samples[samples.length >> 1] ?? Number.NaN);
}

function loadAverage(): string {
    try {
        return readLoad();
    } catch {
        return 'unknown';
    }
}

function readLoad(): string {
    // os.loadavg() is the 1, 5 and 15-minute load
    const { loadavg } = process.getBuiltinModule('node:os');
    return loadavg()
        .map(load => load.toFixed(2))
        .join(' ');
}

function format(microseconds: number): string {
    return microseconds < 1
        ? microseconds.toFixed(4)
        : microseconds < 100
          ? microseconds.toFixed(3)
          : microseconds.toFixed(1);
}

function option(name: string): string | undefined {
    const index = args.indexOf(name);
    return index === -1 ? undefined : args[index + 1];
}
