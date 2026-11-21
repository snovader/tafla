// Imports the build as a dependent package would, without the source
// condition, and reads back a table it wrote. `yarn package:check` runs it.
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
