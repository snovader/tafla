/**
 * Compares how tafla and other libraries write and read lists of 10, 100
 * and 10,000 strings: the vector of strings, a vector of tables of one
 * string each, and the libraries' own lists. `benchmarks/README.md`
 * describes the suite.
 */
import { bench, parseOptions, type Group } from 'benk';
import {
    taflaGenStrings,
    taflaStrings,
    LIST_CODECS,
    type ListCodec
} from './codecs.ts';
import { LIST_SHAPES } from './shapes.ts';

/** Checks that every codec reads back each shape's strings, in order */
function checkRoundTrips(codecs: ListCodec[]): void {
    for (const { name, strings } of LIST_SHAPES) {
        for (const codec of codecs) {
            const bytes = codec.serialize(strings);
            const read = codec.deserialize(bytes);
            const valid =
                read.length === strings.length &&
                read.every((string, i) => string === strings[i]) &&
                (codec.readLast == null ||
                    codec.readLast(bytes) === strings.at(-1));

            if (!valid) {
                throw new Error(`${codec.name} didn't read back ${name}`);
            }
        }
    }
}

/** Checks that tafla-gen's code writes the same tables as `TableBuilder` */
function checkSameTables(): void {
    for (const { name, strings } of LIST_SHAPES) {
        const built = taflaStrings.serialize(strings);
        const generated = taflaGenStrings.serialize(strings);
        const same =
            built.length === generated.length &&
            built.every((byte, i) => byte === generated[i]);

        if (!same) {
            throw new Error(`tafla-gen didn't write TableBuilder's ${name}`);
        }
    }
}

function groups(codecs: ListCodec[]): Group[] {
    return LIST_SHAPES.flatMap(({ name, strings }) => [
        {
            name: `${name}: serialize`,
            // A codec that shares another's serialize() isn't timed twice
            variants: codecs
                .filter(
                    (codec, index) =>
                        codecs.findIndex(
                            other => other.serialize === codec.serialize
                        ) === index
                )
                .map(codec => ({
                    name: codec.name,
                    run: () => codec.serialize(strings)
                }))
        },
        {
            name: `${name}: deserialize`,
            variants: codecs.map(codec => {
                const bytes = codec.serialize(strings);
                return {
                    name: codec.name,
                    run: () => codec.deserialize(bytes)
                };
            })
        },
        {
            name: `${name}: read the last string`,
            variants: codecs.flatMap(
                ({ name: codecName, serialize, readLast }) => {
                    if (readLast == null) {
                        return [];
                    }

                    const bytes = serialize(strings);
                    return [{ name: codecName, run: () => readLast(bytes) }];
                }
            )
        }
    ]);
}

function printSizes(codecs: ListCodec[], print: (line: string) => void): void {
    print(
        `\nEncoded size (bytes)\n  ${''.padEnd(34)}${codecs.map(c => c.name.padStart(20)).join('')}`
    );

    for (const { name, strings } of LIST_SHAPES) {
        const sizes = codecs.map(codec =>
            String(codec.serialize(strings).length).padStart(20)
        );
        print(`  ${name.padEnd(34)}${sizes.join('')}`);
    }
}

/**
 * Prints a line where benk prints its tables: to stdout, or to stderr when
 * `--json -` writes the results there
 */
function printer(args: readonly string[]): (line: string) => void {
    const { json } = parseOptions(args);

    return json === '-'
        ? line => console.error(line)
        : line => console.log(line);
}

const args = process.argv.slice(2);
const print = printer(args);

checkRoundTrips(LIST_CODECS);
checkSameTables();
await bench(groups(LIST_CODECS), { args, print });
printSizes(LIST_CODECS, print);
