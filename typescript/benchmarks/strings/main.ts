/**
 * Compares how tafla and other libraries write and read strings. Each table
 * holds 16 distinct strings of one shape and nothing else, so that a call
 * spends its time on their UTF-8, and the 16 empty strings time what a call
 * takes without any. `benchmarks/README.md` describes the suite.
 */
import { bench, parseOptions, type Group } from 'benk';
import { tafla, taflaGen, CODECS, type Codec } from './codecs.ts';
import { COUNT, SHAPES } from './shapes.ts';

/** Checks that every codec reads back each shape's strings, in order */
function checkRoundTrips(codecs: Codec[]): void {
    for (const { name, strings } of SHAPES) {
        for (const codec of codecs) {
            const read = codec.deserialize(codec.serialize(strings));
            const valid =
                read.length === COUNT &&
                read.every((string, i) => string === strings[i]);

            if (!valid) {
                throw new Error(`${codec.name} didn't read back ${name}`);
            }
        }
    }
}

/** Checks that tafla-gen's code writes the same tables as `TableBuilder` */
function checkSameTables(): void {
    for (const { name, strings } of SHAPES) {
        const built = tafla.serialize(strings);
        const generated = taflaGen.serialize(strings);
        const same =
            built.length === generated.length &&
            built.every((byte, i) => byte === generated[i]);

        if (!same) {
            throw new Error(`tafla-gen didn't write TableBuilder's ${name}`);
        }
    }
}

function groups(codecs: Codec[]): Group[] {
    return SHAPES.flatMap(({ name, strings }) => [
        {
            name: `16 × ${name}: serialize`,
            variants: codecs.map(codec => ({
                name: codec.name,
                run: () => codec.serialize(strings)
            }))
        },
        {
            name: `16 × ${name}: deserialize`,
            variants: codecs.map(codec => {
                const bytes = codec.serialize(strings);
                return {
                    name: codec.name,
                    run: () => codec.deserialize(bytes)
                };
            })
        }
    ]);
}

function printSizes(codecs: Codec[], print: (line: string) => void): void {
    print(
        `\nEncoded size (bytes)\n  ${''.padEnd(26)}${codecs.map(c => c.name.padStart(12)).join('')}`
    );

    for (const { name, strings } of SHAPES) {
        const sizes = codecs.map(codec =>
            String(codec.serialize(strings).length).padStart(12)
        );
        print(`  ${name.padEnd(26)}${sizes.join('')}`);
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

checkRoundTrips(CODECS);
checkSameTables();
await bench(groups(CODECS), { args, print });
printSizes(CODECS, print);
