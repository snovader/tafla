// Times the ways that the prototype's buffer can be made, grow and become
// the table's exact bytes, on their own:
//
//   node benchmarks/prototype/buffers.ts [--rounds 41] [--sample-ms 2]
//
// Every variant makes its buffer in each call, since transferring one
// detaches it, so the groups compare the steps after that.
import { measure, type Variant } from '../comparison/harness.ts';

const args = process.argv.slice(2);
const rounds = Number(option('--rounds') ?? 41);
const sampleTime = Number(option('--sample-ms') ?? 2);

const KIB = 1024;
const MIB = 1024 * KIB;

interface Group {
    name: string;
    variants: Variant[];
}

const reserves: Array<[string, number]> = [
    ['64 KiB', 64 * KIB],
    ['1 MiB', MIB],
    ['16 MiB', 16 * MIB],
    ['256 MiB', 256 * MIB],
    ['4 GiB', 2 ** 32 - 1]
];

const groups: Group[] = [
    {
        name: "a builder's buffer: 64 bytes, with views",
        variants: [
            {
                name: 'new Uint8Array(64), on the heap',
                run: () => new Uint8Array(64)
            },
            { name: 'new Uint8Array(512)', run: () => new Uint8Array(512) },
            {
                name: 'new Uint8Array(64) and a DataView',
                run: () => {
                    const bytes = new Uint8Array(64);
                    return new DataView(bytes.buffer);
                }
            },
            ...reserves.map(([name, max]) => ({
                name: `resizable, maxByteLength ${name}, and views`,
                run: () => {
                    const buffer = new ArrayBuffer(64, { maxByteLength: max });
                    const bytes = new Uint8Array(buffer);
                    return [bytes, new DataView(buffer)];
                }
            }))
        ]
    },
    ...[512, 4 * KIB, 64 * KIB].map(
        (length): Group => ({
            name: `growing a buffer of 64 bytes to ${length.toLocaleString('en')}, doubling from 512`,
            variants: [
                {
                    name: 'grow and copy: new Uint8Array(), set()',
                    run: () => {
                        let bytes = new Uint8Array(64);
                        for (
                            let capacity = 512;
                            capacity <= length;
                            capacity *= 2
                        ) {
                            const next = new Uint8Array(capacity);
                            next.set(bytes);
                            bytes = next;
                        }
                        return bytes;
                    }
                },
                {
                    name: 'transfer(newLength)',
                    run: () => {
                        let bytes = new Uint8Array(64);
                        for (
                            let capacity = 512;
                            capacity <= length;
                            capacity *= 2
                        ) {
                            bytes = new Uint8Array(
                                (bytes.buffer as ArrayBuffer).transfer(capacity)
                            );
                        }
                        return bytes;
                    }
                },
                {
                    name: 'resize(), maxByteLength 64 KiB',
                    run: () => {
                        const buffer = new ArrayBuffer(64, {
                            maxByteLength: 64 * KIB
                        });
                        const bytes = new Uint8Array(buffer);
                        for (
                            let capacity = 512;
                            capacity <= length;
                            capacity *= 2
                        ) {
                            buffer.resize(capacity);
                        }
                        return bytes;
                    }
                }
            ]
        })
    ),
    ...(
        [
            [64, 60],
            [4 * KIB, 3_208],
            [2 * MIB, 1_200_096]
        ] as const
    ).map(
        ([capacity, length]): Group => ({
            name: `exact bytes of ${length.toLocaleString('en')} from a buffer of ${capacity.toLocaleString('en')}`,
            variants: [
                {
                    name: 'slice()',
                    run: () => new Uint8Array(capacity).slice(0, length)
                },
                {
                    name: 'transfer(length)',
                    run: () =>
                        new Uint8Array(
                            (
                                new Uint8Array(capacity).buffer as ArrayBuffer
                            ).transfer(length)
                        )
                },
                {
                    name: 'resizable, slice()',
                    run: () => {
                        const buffer = new ArrayBuffer(capacity, {
                            maxByteLength: Math.max(capacity, 64 * KIB)
                        });
                        return new Uint8Array(buffer).slice(0, length);
                    }
                },
                {
                    name: 'resizable, transferToFixedLength(length)',
                    run: () => {
                        const buffer = new ArrayBuffer(capacity, {
                            maxByteLength: Math.max(capacity, 64 * KIB)
                        });
                        return new Uint8Array(
                            buffer.transferToFixedLength(length)
                        );
                    }
                }
            ]
        })
    )
];

console.log(
    `Node ${process.version}, V8 ${process.versions.v8}; fastest of ${rounds} interleaved rounds of ${sampleTime} ms; load ${loadAverage()}`
);

for (const { name, variants } of groups) {
    console.log(`\n${name}`);
    for (const { name: variant, microseconds, heapBytes } of measure(variants, {
        rounds,
        warmupRounds: 10,
        sampleTime
    })) {
        console.log(
            `  ${variant.padEnd(48)} ${(microseconds * 1000).toFixed(1).padStart(10)} ns  heap ${Math.round(heapBytes).toLocaleString('en').padStart(7)} B`
        );
    }
}

function loadAverage(): string {
    const { loadavg } = process.getBuiltinModule('node:os');
    return loadavg()
        .map(load => load.toFixed(2))
        .join(' ');
}

function option(name: string): string | undefined {
    const index = args.indexOf(name);
    return index === -1 ? undefined : args[index + 1];
}
