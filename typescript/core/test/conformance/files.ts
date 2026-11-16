import { corrupt, type Suite } from './description.ts';
import { buildTable } from './tables.ts';

/** The suite's directory, at the root of the repository */
export const SUITE_DIRECTORY = new URL(
    '../../../../conformance/',
    import.meta.url
);

export interface SuiteFiles {
    /** The text of `cases.json` */
    json: string;
    /** The bytes of the tables, by their paths relative to `cases.json` */
    tables: Map<string, Uint8Array>;
}

/** Writes the suite: its description, and its tables built by tafla */
export function writeSuite(suite: Suite): SuiteFiles {
    const tables = new Map<string, Uint8Array>();

    const valid = new Map<string, Uint8Array>();
    for (const { name, file, table } of suite.valid) {
        const bytes = buildTable(table).bytes();
        valid.set(name, bytes);
        tables.set(file, bytes);
    }

    for (const { file, case: name, corruption } of [
        ...suite.equivalent,
        ...suite.invalid
    ]) {
        const bytes = valid.get(name);
        if (bytes === undefined) {
            throw new Error(`There's no valid case ${name}`);
        }

        tables.set(file, corrupt(bytes, corruption));
    }

    return { json: formatSuiteJson(suite), tables };
}

/**
 * Formats a suite's description as its files hold it: indented, ASCII and
 * ending in a newline
 */
export function formatSuiteJson(value: unknown): string {
    return `${escapeNonAscii(formatJson(value))}\n`;
}

/**
 * Formats JSON indented by 4 spaces, but with an object or array on one line
 * when it holds no objects, such as a field and its values
 */
function formatJson(value: unknown, indent = ''): string {
    if (typeof value !== 'object' || value === null || isFlat(value)) {
        return formatInline(value);
    }

    const inner = `${indent}    `;

    if (Array.isArray(value)) {
        const items = value.map(item => inner + formatJson(item, inner));
        return `[\n${items.join(',\n')}\n${indent}]`;
    }

    const entries = Object.entries(value).map(
        ([key, item]) =>
            `${inner}${JSON.stringify(key)}: ${formatJson(item, inner)}`
    );
    return `{\n${entries.join(',\n')}\n${indent}}`;
}

function formatInline(value: unknown): string {
    if (Array.isArray(value)) {
        return `[${value.map(formatInline).join(', ')}]`;
    }

    if (typeof value === 'object' && value !== null) {
        const entries = Object.entries(value).map(
            ([key, item]) => `${JSON.stringify(key)}: ${formatInline(item)}`
        );
        return entries.length === 0 ? '{}' : `{ ${entries.join(', ')} }`;
    }

    return JSON.stringify(value);
}

/** Whether an object or array holds nothing but primitives and their arrays */
function isFlat(value: object): boolean {
    return Object.values(value).every(
        item =>
            typeof item !== 'object' ||
            item === null ||
            (Array.isArray(item) && isFlat(item))
    );
}

/**
 * Escapes every character outside ASCII, so that the file shows characters
 * such as U+FEFF, and readers can't mistake its encoding
 */
function escapeNonAscii(json: string): string {
    return json.replaceAll(
        /[\u007f-￿]/g,
        character =>
            `\\u${character.charCodeAt(0).toString(16).padStart(4, '0')}`
    );
}
