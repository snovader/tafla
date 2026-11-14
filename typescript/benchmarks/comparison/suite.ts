import type { Codec, Workload } from './codecs/codec.ts';
import {
    type Measurement,
    type MeasureOptions,
    measure,
    type Variant
} from './harness.ts';
import { type ItemAccess, sumItem, writeItems } from './itemAccess.ts';
import {
    DOCUMENT_ROWS,
    type DocumentInput,
    type DocumentOutput,
    documentInput,
    documentNumbers,
    LIST_ITEMS,
    type ListInput,
    listInput,
    RECORD,
    SAMPLE
} from './workloads.ts';

export interface SuiteOptions extends MeasureOptions {
    /** Runs only the groups whose name matches */
    filter: RegExp;
    print: (line: string) => void;
}

interface Group {
    name: string;
    variants: Variant[];
}

/**
 * Runs every workload through every codec, and reads the items of tafla's
 * vector and array of structs with each of `accesses`. The groups with
 * typed-array inputs run first, since `number[]` inputs change the type
 * feedback of the code they share.
 */
export function runSuite(
    codecs: Codec[],
    accesses: ItemAccess[],
    options: SuiteOptions
): void {
    const documents = DOCUMENT_ROWS.map(documentInput);
    const list = listInput(LIST_ITEMS);

    checkRoundTrips(codecs, documents, list);
    checkItemAccess(accesses, list);

    const groups = [
        ...workloadGroups(codecs, 'record', c => c.record, RECORD),
        ...documents.flatMap((input, i) => [
            ...workloadGroups(
                codecs,
                `document of ${DOCUMENT_ROWS[i]} rows`,
                c => c.document,
                input
            ),
            group(
                `document of ${DOCUMENT_ROWS[i]} rows: sum a column per element`,
                codecs,
                codec => {
                    const bytes = codec.document.serialize(input);
                    return () => codec.document.sumA(bytes);
                }
            )
        ]),
        reusedBytesGroup(
            codecs,
            `document of ${DOCUMENT_ROWS[DOCUMENT_ROWS.length - 1]} rows: serialize into reused bytes`,
            documents[documents.length - 1]
        ),
        ...workloadGroups(
            codecs,
            `vector of ${LIST_ITEMS} nested tables`,
            c => c.list,
            list
        ),
        ...workloadGroups(
            codecs,
            `array of ${LIST_ITEMS} structs`,
            c => c.structs,
            list
        ),
        ...accessGroups(accesses, list),
        ...workloadGroups(codecs, 'fixed-size record', c => c.sample, SAMPLE),
        ...documents.map((input, i) =>
            serializeGroup(
                codecs,
                `document of ${DOCUMENT_ROWS[i]} rows: serialize number[]`,
                c => c.document,
                documentNumbers(input)
            )
        )
    ];

    for (const { name, variants } of groups) {
        if (!options.filter.test(name)) {
            continue;
        }

        printMeasurements(name, measure(variants, options), options.print);
    }

    printSizes(codecs, documents, list, options.print);
    printTruncation(codecs, documents[documents.length - 1], options.print);
}

function workloadGroups<I, O>(
    codecs: Codec[],
    name: string,
    select: (codec: Codec) => Workload<I, O>,
    input: I
): Group[] {
    return [
        serializeGroup(codecs, `${name}: serialize`, select, input),
        group(`${name}: deserialize`, codecs, codec => {
            const workload = select(codec);
            const bytes = workload.serialize(input);

            return () => workload.deserialize(bytes);
        })
    ];
}

function serializeGroup<I, O>(
    codecs: Codec[],
    name: string,
    select: (codec: Codec) => Workload<I, O>,
    input: I
): Group {
    return group(name, codecs, codec => {
        const workload = select(codec);
        return () => workload.serialize(input);
    });
}

/**
 * Writes a document into bytes kept between calls, with each codec that can:
 * tafla-gen into bytes that are made once, as long as its table, and
 * msgpackr into the buffer that its `Packr` reuses
 */
function reusedBytesGroup(
    codecs: Codec[],
    name: string,
    input: DocumentInput
): Group {
    const variants = codecs.flatMap(({ name, document }) => {
        const { serializeInto } = document;
        if (serializeInto === undefined) {
            return [];
        }

        const bytes = new Uint8Array(document.serialize(input).length);
        return [{ name, run: () => serializeInto(input, bytes) }];
    });

    return { name, variants };
}

/**
 * Reads the last item of the vector of nested tables and of the array of
 * structs, which a reader can find without reading the others, and sums
 * every item, in tafla's bytes
 */
function accessGroups(accesses: ItemAccess[], list: ListInput): Group[] {
    const { tables, structs } = writeItems(list);
    const vector = `vector of ${LIST_ITEMS} nested tables`;
    const array = `array of ${LIST_ITEMS} structs`;

    return [
        group(`${vector}: read the last item`, accesses, access => {
            return () => access.lastTable(tables);
        }),
        group(`${vector}: sum every item`, accesses, access => {
            return () => access.everyTable(tables);
        }),
        group(`${array}: read the last item`, accesses, access => {
            return () => access.lastStruct(structs);
        }),
        group(`${array}: sum every item`, accesses, access => {
            return () => access.everyStruct(structs);
        })
    ];
}

/** A group of a variant for each codec, or each other way of reading */
function group<T extends { name: string }>(
    name: string,
    readers: T[],
    createRun: (reader: T) => () => unknown
): Group {
    return {
        name,
        variants: readers.map(reader => ({
            name: reader.name,
            run: createRun(reader)
        }))
    };
}

function printMeasurements(
    name: string,
    measurements: Measurement[],
    print: (line: string) => void
): void {
    const [reference] = measurements;

    print(`\n${name}`);
    for (const { name: variant, microseconds, heapBytes } of measurements) {
        const ratio = microseconds / reference.microseconds;
        print(
            `  ${variant.padEnd(18)} ${formatMicroseconds(microseconds).padStart(9)} µs ${ratio.toFixed(2).padStart(7)}× ${formatBytes(heapBytes).padStart(12)}`
        );
    }
}

function formatMicroseconds(microseconds: number): string {
    return microseconds.toFixed(microseconds < 10 ? 3 : 1);
}

/** Bytes allocated on V8's heap per call, rounded to whole bytes */
function formatBytes(bytes: number): string {
    return `${Math.max(0, Math.round(bytes)).toLocaleString('en-US')} B`;
}

function printSizes(
    codecs: Codec[],
    documents: DocumentInput[],
    list: ReturnType<typeof listInput>,
    print: (line: string) => void
): void {
    const columns = [
        'record',
        ...DOCUMENT_ROWS.map(rows => `doc ${rows}`),
        'vector',
        'structs',
        'fixed'
    ];

    print(
        `\nEncoded size (bytes)\n  ${''.padEnd(16)} ${columns.map(c => c.padStart(11)).join('')}`
    );
    for (const codec of codecs) {
        const sizes = [
            codec.record.serialize(RECORD).length,
            ...documents.map(input => codec.document.serialize(input).length),
            codec.list.serialize(list).length,
            codec.structs.serialize(list).length,
            codec.sample.serialize(SAMPLE).length
        ];

        print(
            `  ${codec.name.padEnd(16)} ${sizes.map(size => String(size).padStart(11)).join('')}`
        );
    }
}

/**
 * Decodes copies of a document cut short, as a dropped connection would leave
 * it. A copy, since a view could read on into the full bytes.
 */
function printTruncation(
    codecs: Codec[],
    input: DocumentInput,
    print: (line: string) => void
): void {
    print(`\nTruncated document of ${input.a.length} rows: deserialize()`);
    for (const codec of codecs) {
        const bytes = codec.document.serialize(input);
        for (const length of [bytes.length >> 1, 24]) {
            const outcome = describeOutcome(
                () => codec.document.deserialize(bytes.slice(0, length)),
                input.a.length
            );
            print(
                `  ${codec.name.padEnd(16)} ${String(length).padStart(7)} B  ${outcome}`
            );
        }
    }
}

function describeOutcome(decode: () => DocumentOutput, length: number): string {
    try {
        const { a } = decode();
        return `returned ${a?.length ?? 0} of ${length} values`;
    } catch (error) {
        const { name, message } = error as Error;
        return `threw ${name}: ${message.slice(0, 50)}`;
    }
}

/**
 * Checks that every codec reads back what it wrote, so that none is timed
 * while it skips or corrupts part of the work.
 */
function checkRoundTrips(
    codecs: Codec[],
    documents: DocumentInput[],
    list: ReturnType<typeof listInput>
): void {
    for (const codec of codecs) {
        assertEqual(
            codec.name,
            'record',
            codec.record.deserialize(codec.record.serialize(RECORD)),
            RECORD
        );

        for (const document of documents) {
            for (const input of [document, documentNumbers(document)]) {
                const output = codec.document.deserialize(
                    codec.document.serialize(input)
                );
                assertDocument(codec.name, input, output);
            }

            const { serializeInto } = codec.document;
            if (serializeInto !== undefined) {
                // Longer bytes than the table, of other contents
                const length = codec.document.serialize(document).length;
                const bytes = new Uint8Array(length + 64).fill(0xff);
                const output = codec.document.deserialize(
                    serializeInto(document, bytes)
                );
                assertDocument(codec.name, document, output);
            }
        }

        assertEqual(
            codec.name,
            'list',
            codec.list.deserialize(codec.list.serialize(list)),
            list
        );
        assertEqual(
            codec.name,
            'structs',
            codec.structs.deserialize(codec.structs.serialize(list)),
            list
        );
        assertEqual(
            codec.name,
            'sample',
            codec.sample.deserialize(codec.sample.serialize(SAMPLE)),
            SAMPLE
        );
    }
}

/** Checks that every access reads the items' float32s, and sums them alike */
function checkItemAccess(accesses: ItemAccess[], list: ListInput): void {
    const { tables, structs } = writeItems(list);
    const last = sumItem(list.items[list.items.length - 1]);
    let every = 0;
    for (const item of list.items) {
        every += sumItem(item);
    }

    for (const access of accesses) {
        const sums = [
            access.lastTable(tables),
            access.everyTable(tables),
            access.lastStruct(structs),
            access.everyStruct(structs)
        ];
        const expected = [last, every, last, every];

        if (!sums.every((sum, i) => Object.is(sum, expected[i]))) {
            throw new Error(`${access.name} didn't sum the items: ${sums}`);
        }
    }
}

function assertDocument(
    codec: string,
    input: DocumentInput,
    output: DocumentOutput
): void {
    assertEqual(codec, 'document', headerOf(output), headerOf(input));
    assertColumn(codec, 'keys', output.keys, input.keys);
    assertColumn(codec, 'a', output.a, input.a);
    assertColumn(codec, 'b', output.b, input.b);
}

/** The document's fields other than its columns */
function headerOf({
    x,
    y,
    name,
    base
}: DocumentInput | DocumentOutput): Omit<DocumentOutput, 'keys' | 'a' | 'b'> {
    return { x, y, name, base };
}

function assertColumn(
    codec: string,
    name: string,
    actual: Float32Array | Uint32Array,
    expected: ArrayLike<number>
): void {
    const TypedArray = actual.constructor as Float32ArrayConstructor;
    const valid =
        actual.length === expected.length &&
        TypedArray.from(expected).every((value, i) =>
            Object.is(value, actual[i])
        );

    if (!valid) {
        throw new Error(`${codec} didn't read back column ${name}`);
    }
}

function assertEqual(
    codec: string,
    name: string,
    actual: object,
    expected: object
): void {
    // Keys in a fixed order, and bigints as strings
    const stringify = (value: object): string =>
        JSON.stringify(value, (_, v) =>
            typeof v === 'bigint' ? String(v) : v
        );

    if (stringify(actual) !== stringify(expected)) {
        throw new Error(
            `${codec} didn't read back the ${name}: ${stringify(actual)}`
        );
    }
}
