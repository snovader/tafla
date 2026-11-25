/**
 * Times what optional fields and fields added `since` a version cost the
 * code that tafla-gen generates: reads of the required fields of tables
 * that also have such fields, against tables that don't; reads of such
 * fields, present and missing; the constructor of a message that reads a
 * range of versions, against one that reads its own; and writes.
 * `benchmarks/README.md` describes the suite.
 */
import { bench, type Group, type Variant } from 'benk';
import {
    Exact,
    Holder,
    HolderExact,
    HolderV1,
    Optional,
    Plain,
    Ranged,
    RangedV1,
    type OptionalFields,
    type PlainFields
} from './generated/tafla/presence.ts';

/** The tables that each call reads or writes, each of its own values */
const TABLES = 64;

/** Numbers the sources of the variants' calls, so that no two are the same */
let sourceId = 0;

const plainFields: PlainFields[] = Array.from({ length: TABLES }, (_, i) => ({
    x: i + 0.5,
    id: i * 7,
    name: `name ${i}`,
    y: i * 2.25
}));

const presentFields: OptionalFields[] = plainFields.map((fields, i) => ({
    ...fields,
    z: i * 0.75,
    label: `label ${i}`
}));

const absentFields: OptionalFields[] = plainFields.map(fields => ({
    ...fields
}));

const exactFields = presentFields.map(fields => ({
    ...fields,
    z: fields.z ?? 0
}));

const points = plainFields.map(({ x, y }) => ({ x, y, z: x + y }));

const bytes = {
    plain: plainFields.map(fields => Plain.serialize(fields)),
    present: presentFields.map(fields => Optional.serialize(fields)),
    absent: absentFields.map(fields => Optional.serialize(fields)),
    exact: exactFields.map(fields => Exact.serialize(fields)),
    ranged: exactFields.map(fields => Ranged.serialize(fields)),
    rangedV1: plainFields.map(fields => RangedV1.serialize(fields)),
    holderExact: points.map(point => HolderExact.serialize({ point })),
    holder: points.map(point => Holder.serialize({ point })),
    holderV1: points.map(({ x, y }) => HolderV1.serialize({ point: { x, y } }))
};

const readers = {
    plain: bytes.plain.map(table => new Plain(table)),
    present: bytes.present.map(table => new Optional(table)),
    absent: bytes.absent.map(table => new Optional(table)),
    exact: bytes.exact.map(table => new Exact(table)),
    ranged: bytes.ranged.map(table => new Ranged(table)),
    rangedFromV1: bytes.rangedV1.map(table => new Ranged(table)),
    holderExact: bytes.holderExact.map(table => new HolderExact(table)),
    holder: bytes.holder.map(table => new Holder(table)),
    holderFromV1: bytes.holderV1.map(table => new Holder(table))
};

/**
 * A variant whose call runs `body` with `parameters` in a function compiled
 * from a source of its own, so that no two variants share type feedback,
 * and that writes what it reads into `results`, which keeps every value
 */
function variant(
    name: string,
    parameters: Record<string, unknown>,
    body: string,
    expected: () => unknown
): Variant {
    // oxlint-disable-next-line typescript/no-implied-eval -- A call of its own for each variant
    const create = new Function(
        ...Object.keys(parameters),
        `return () => {
        ${body}
        }; // presence variant ${sourceId++}`
    ) as (...values: unknown[]) => () => unknown;
    const run = create(...Object.values(parameters));

    const read = JSON.stringify(run(), replacer);
    const wanted = JSON.stringify(expected(), replacer);
    if (read !== wanted) {
        throw new Error(
            `${name} read ${read.slice(0, 80)} rather than ${wanted.slice(0, 80)}`
        );
    }

    return { name, run };
}

/** JSON of typed arrays and undefined, to compare what a call returns */
function replacer(_key: string, value: unknown): unknown {
    if (value === undefined) {
        return 'undefined';
    }

    return ArrayBuffer.isView(value)
        ? Array.from(value as unknown as ArrayLike<number>)
        : value;
}

/** Reads x, id and y of every table, as each kind of message holds them */
function requiredReads(name: string, tables: unknown[]): Variant {
    const results = new Float64Array(TABLES * 3);

    return variant(
        name,
        { tables, results },
        `for (let i = 0; i < tables.length; i++) {
            const table = tables[i];
            results[3 * i] = table.x();
            results[3 * i + 1] = table.id();
            results[3 * i + 2] = table.y();
        }
        return results;`,
        () => plainFields.flatMap(({ x, id, y }) => [x, id, y])
    );
}

/** Reads `z` of every table, with NaN for a table that doesn't hold it */
function zReads(
    name: string,
    tables: unknown[],
    path: string,
    expected: (i: number) => number
): Variant {
    const results = new Float64Array(TABLES);

    return variant(
        name,
        { tables, results },
        `for (let i = 0; i < tables.length; i++) {
            results[i] = tables[i]${path}.z() ?? NaN;
        }
        return results;`,
        () => Array.from({ length: TABLES }, (_, i) => expected(i))
    );
}

/** Constructs a reader of every table, and reads its `x` */
function constructs(
    name: string,
    Reader: unknown,
    tables: Uint8Array[]
): Variant {
    const results = new Float64Array(TABLES);

    return variant(
        name,
        { Reader, tables, results },
        `for (let i = 0; i < tables.length; i++) {
            results[i] = new Reader(tables[i]).x();
        }
        return results;`,
        () => plainFields.map(({ x }) => x)
    );
}

/** Writes every table, and keeps the bytes of each */
function writes(
    name: string,
    Writer: unknown,
    fields: unknown[],
    expected: Uint8Array[]
): Variant {
    const results = Array.from<Uint8Array | undefined>({ length: TABLES });

    return variant(
        name,
        { Writer, fields, results },
        `for (let i = 0; i < fields.length; i++) {
            results[i] = Writer.serialize(fields[i]);
        }
        return results;`,
        () => expected
    );
}

const NAN = (): number => Number.NaN;

/**
 * Reads the optional `z` of every table as a getter that returns a default
 * for an absent field, rather than undefined, would read it once V8 inlines
 * it. A default of `NaN`, which JavaScript loads as a global, merges a
 * tagged value with the double that's read, as undefined does, which V8
 * boxes; a default of `0`, a literal, doesn't.
 */
function zDefaultReads(
    name: string,
    tables: unknown[],
    missing: 'NaN' | '0',
    expected: (i: number) => number
): Variant {
    const results = new Float64Array(TABLES);

    return variant(
        name,
        { tables, results },
        `for (let i = 0; i < tables.length; i++) {
            const reader = tables[i]._reader;
            const offset = reader.rootOffset(4);
            results[i] = offset === 0 ? ${missing} : reader.getFloat64(offset);
        }
        return results;`,
        () => Array.from({ length: TABLES }, (_, i) => expected(i))
    );
}

const groups: Group[] = [
    {
        name: 'required fields: x, id and y of 64 tables',
        variants: [
            requiredReads('Plain', readers.plain),
            requiredReads('Optional, present', readers.present),
            requiredReads('Optional, absent', readers.absent),
            requiredReads('Exact, version 2', readers.exact),
            requiredReads('Ranged, version 2', readers.ranged),
            requiredReads('Ranged, version 1', readers.rangedFromV1)
        ]
    },
    {
        name: 'z of 64 root tables',
        variants: [
            zReads('Exact: required', readers.exact, '', i => exactFields[i].z),
            zReads(
                'Optional: present',
                readers.present,
                '',
                i => exactFields[i].z
            ),
            zReads('Optional: absent', readers.absent, '', NAN),
            zDefaultReads(
                'Optional, NaN default: present',
                readers.present,
                'NaN',
                i => exactFields[i].z
            ),
            zDefaultReads(
                'Optional, NaN default: absent',
                readers.absent,
                'NaN',
                NAN
            ),
            zDefaultReads(
                'Optional, 0 default: present',
                readers.present,
                '0',
                i => exactFields[i].z
            ),
            zDefaultReads(
                'Optional, 0 default: absent',
                readers.absent,
                '0',
                () => 0
            ),
            zReads(
                'Ranged: version 2',
                readers.ranged,
                '',
                i => exactFields[i].z
            ),
            zReads('Ranged: version 1', readers.rangedFromV1, '', NAN)
        ]
    },
    {
        name: 'z of 64 nested tables',
        variants: [
            zReads(
                'PointExact: required',
                readers.holderExact,
                '.point()',
                i => points[i].z
            ),
            zReads(
                'Point: revision 2',
                readers.holder,
                '.point()',
                i => points[i].z
            ),
            zReads('Point: revision 1', readers.holderFromV1, '.point()', NAN)
        ]
    },
    {
        name: 'construct 64 readers and read x',
        variants: [
            constructs('Plain', Plain, bytes.plain),
            constructs('Exact: version 2', Exact, bytes.exact),
            constructs('Ranged: version 2', Ranged, bytes.ranged),
            constructs('Ranged: version 1', Ranged, bytes.rangedV1)
        ]
    },
    {
        name: 'write 64 tables',
        variants: [
            writes('Plain', Plain, plainFields, bytes.plain),
            writes('Optional: absent', Optional, absentFields, bytes.absent),
            writes('Optional: present', Optional, presentFields, bytes.present),
            writes('Exact', Exact, exactFields, bytes.exact),
            writes('Ranged', Ranged, exactFields, bytes.ranged)
        ]
    }
];

await bench(groups);
