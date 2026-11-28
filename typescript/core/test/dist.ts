// Imports the build as a dependent package would, without the source
// condition, reads back a table it wrote, and checks what V8 makes of the
// bundle. `yarn package:check` runs it.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { TableBuilder, TableReader } from 'tafla';

const builder = new TableBuilder(1);
builder.setString('tafla');
const reader = new TableReader(builder.bytes());

if (reader.getString(reader.rootOffset(0)) !== 'tafla') {
    throw new Error('The build read back another string');
}

// V8 keeps a module's source on its heap, at two bytes per character rather
// than one if a single character is outside Latin-1.
const bundle = readFileSync(new URL(import.meta.resolve('tafla')), 'utf8');
const wide = /[Ā-\u{10ffff}]/u.exec(bundle);

if (wide != null) {
    const codePoint = wide[0].codePointAt(0)!.toString(16).toUpperCase();
    throw new Error(
        `The bundle has U+${codePoint.padStart(4, '0')}, outside Latin-1, at ${wide.index}`
    );
}

// TurboFan never inlines a function of more bytecode than V8's
// --max-inlined-bytecode-size, 460 bytes. TableBuilder keeps these functions
// over it, as table-builder.ts describes, so that V8 compiles each on its own
// with what it calls, rather than inlining it into its caller.
const OUT_OF_LINE = ['writeField', '#writeFields'];

// Compiles both, in a process that V8 prints the bytecode of
const BUILD_TABLE = `
const { TableBuilder } = await import(process.argv[1]);
const builder = new TableBuilder(2);
builder.setInt32(1);
builder.setString('tafla');
builder.bytes();
`;

const limit = inliningLimit();

for (const name of OUT_OF_LINE) {
    const length = bytecodeLength(name);

    if (!(length > limit)) {
        throw new Error(
            `The bundle's ${name}() is ${length} bytes of bytecode, which V8 inlines, rather than more than ${limit}`
        );
    }
}

/** V8's --max-inlined-bytecode-size, as Node's V8 sets it by default */
function inliningLimit(): number {
    const options = execFileSync(process.execPath, ['--v8-options'], {
        encoding: 'utf8'
    });
    const limit = /--max-inlined-bytecode-size=(\d+)/.exec(options);

    if (limit == null) {
        throw new Error("V8 doesn't list --max-inlined-bytecode-size");
    }

    return Number(limit[1]);
}

/** The length of the bytecode that V8 compiles for the bundle's `name` */
function bytecodeLength(name: string): number {
    const output = execFileSync(
        process.execPath,
        [
            '--print-bytecode',
            `--print-bytecode-filter=${name}`,
            '--input-type=module',
            '--eval',
            BUILD_TABLE,
            import.meta.resolve('tafla')
        ],
        { encoding: 'utf8' }
    );
    const length = /^Bytecode length: (\d+)$/m.exec(output);

    if (length == null) {
        throw new Error(`V8 compiled no function ${name} of the bundle`);
    }

    return Number(length[1]);
}
