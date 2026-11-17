// The titles of the tests and suites that `.each` and `.for` declare, as
// Vitest 5.1's runner formats them, with the functions that it formats them
// with, from @vitest/utils
import {
    format,
    formatRegExp,
    inspect,
    truncateString
} from '@vitest/utils/display';
import { isNegativeNaN, isObject, objectAttr } from '@vitest/utils/helpers';

/** The characters that a value prints in, Vitest's default */
const TRUNCATE = 40;

const INSPECT_OPTIONS = { truncate: TRUNCATE };

/** `$name` and `$0.name`, the paths of values in a row */
const ATTRIBUTE = /\$([$\p{ID_Continue}.]+)/gu;

/**
 * Formats a title of `.each` or `.for` with the row's values: `%s`, `%d`,
 * `%i`, `%f`, `%j`, `%o` and `%O` print them in order, as `util.format`
 * does, `%#` and `%$` print the row's index from 0 and from 1, and `$name`
 * prints a property of the row, if it's an object, or of an array's element,
 * as `$0.name`.
 */
export function formatTitle(
    template: string,
    items: readonly unknown[],
    index: number
): string {
    const indexed = formatIndex(template, index);
    const signed = signZeroes(indexed, items);
    const count = signed.split('%').length - 1;

    let title = '';
    let position = 0;
    let used = 0;
    for (const match of signed.matchAll(formatRegExp)) {
        title += formatAttributes(signed.slice(position, match.index), items);
        title +=
            used < count
                ? format([match[0], items[used++]], INSPECT_OPTIONS)
                : match[0];
        position = match.index + match[0].length;
    }

    return title + formatAttributes(signed.slice(position), items);
}

/** Replaces `%#` with the row's index, and `%$` with it from 1 */
function formatIndex(template: string, index: number): string {
    if (!(template.includes('%#') || template.includes('%$'))) {
        return template;
    }

    return template
        .replaceAll('%%', '__vitest_escaped_%__')
        .replaceAll('%#', String(index))
        .replaceAll('%$', String(index + 1))
        .replaceAll('__vitest_escaped_%__', '%%');
}

/**
 * Puts a minus before each `%f` whose value is -0 or a negative NaN, which
 * `%f` prints without its sign
 */
function signZeroes(template: string, items: readonly unknown[]): string {
    let signed = template;

    for (const [index] of (template.match(/%f/g) ?? []).entries()) {
        const value = items[index] as number;
        if (isNegativeNaN(value) || Object.is(value, -0)) {
            let occurrence = 0;
            signed = signed.replaceAll('%f', match =>
                ++occurrence === index + 1 ? '-%f' : match
            );
        }
    }

    return signed;
}

/** Replaces `$name` with the property of the row, or of its element */
function formatAttributes(text: string, items: readonly unknown[]): string {
    const isObjectItem = isObject(items[0]);

    return text.replaceAll(ATTRIBUTE, (_, key: string) => {
        const isArrayKey = /^\d+$/.test(key);
        if (!isObjectItem && !isArrayKey) {
            return `$${key}`;
        }

        const element: unknown = isArrayKey
            ? objectAttr(items, key)
            : undefined;
        const value: unknown = isObjectItem
            ? objectAttr(items[0], key, element as undefined)
            : element;

        // Strings print without quotes
        return typeof value === 'string'
            ? truncateString(value, TRUNCATE)
            : inspect(value, INSPECT_OPTIONS);
    });
}
