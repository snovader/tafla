/**
 * Compares layouts that a list of strings could have in a table, each
 * written and read by hand in `layouts.ts` with the same UTF-8 code, so that
 * their times differ only by the layout. `benchmarks/README.md` describes
 * the suite.
 */
import { bench, parseOptions, type Group } from 'benk';
import { LAYOUTS, type Layout } from './layouts.ts';
import { LIST_SHAPES } from './shapes.ts';

/** Checks that every layout reads back each shape's strings */
function checkRoundTrips(layouts: Layout[]): void {
    for (const { name, strings } of LIST_SHAPES) {
        for (const layout of layouts) {
            const bytes = layout.write(strings);
            const read = layout.readAll(bytes);
            const valid =
                read.length === strings.length &&
                read.every((string, i) => string === strings[i]) &&
                strings.every(
                    (string, i) => layout.readOne(bytes, i) === string
                );

            if (!valid) {
                throw new Error(`${layout.name} didn't read back ${name}`);
            }
        }
    }
}

function groups(layouts: Layout[]): Group[] {
    return LIST_SHAPES.flatMap(({ name, strings }) => {
        const last = strings.length - 1;

        return [
            {
                name: `${name}: write`,
                variants: layouts
                    // Layouts that share another's bytes share its write
                    .filter(
                        layout =>
                            layout.write !== layouts[0].write ||
                            layout === layouts[0]
                    )
                    .map(layout => ({
                        name: layout.name,
                        run: () => layout.write(strings)
                    }))
            },
            {
                name: `${name}: read every string`,
                variants: layouts.map(layout => {
                    const bytes = layout.write(strings);
                    return {
                        name: layout.name,
                        run: () => layout.readAll(bytes)
                    };
                })
            },
            {
                name: `${name}: read the last string`,
                variants: layouts
                    .filter(
                        layout =>
                            layout.readOne !== layouts[0].readOne ||
                            layout === layouts[0]
                    )
                    .map(layout => {
                        const bytes = layout.write(strings);
                        return {
                            name: layout.name,
                            run: () => layout.readOne(bytes, last)
                        };
                    })
            }
        ];
    });
}

function printSizes(layouts: Layout[], print: (line: string) => void): void {
    print(
        `\nEncoded size (bytes)\n  ${''.padEnd(34)}${layouts.map(l => l.name.padStart(28)).join('')}`
    );

    for (const { name, strings } of LIST_SHAPES) {
        const sizes = layouts.map(layout =>
            String(layout.write(strings).length).padStart(28)
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

checkRoundTrips(LAYOUTS);
await bench(groups(LAYOUTS), { args, print });
printSizes(LAYOUTS, print);
