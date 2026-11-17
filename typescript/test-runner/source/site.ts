// node:test records where each test was declared as the position of the
// code that called it, which would be this package's. Reporters print it,
// as `test at file:line:column`, and JUnit's names the file of each test with
// it. So each declaration calls node:test from a function compiled at the
// position of the code that called this package, which it records instead.
import { getCallSites } from 'node:util';
import { compileFunction } from 'node:vm';

/** Where a test or a suite was declared */
export interface Site {
    readonly scriptName: string;
    readonly lineNumber: number;
    readonly columnNumber: number;
}

/** node:test's `it` or `describe` */
type Register<Options, Fn> = (
    name: string,
    options: Options,
    fn: Fn
) => unknown;
type Caller = (
    register: Register<unknown, unknown>,
    name: string,
    options: unknown,
    fn: unknown
) => unknown;

/** The functions compiled at each site, which each row of `.each` shares */
const callers = new Map<string, Caller>();

/**
 * The position of the code that called the function that calls this: the
 * test file's, when that function is one that a test file calls
 */
export function callerSite(): Site | undefined {
    // This function, the one that calls it, then the one that calls that
    const site = getCallSites(3)[2];
    return site?.scriptName ? site : undefined;
}

/** Calls `register` from code at the site, if there is one */
export function callFrom<Options, Fn>(
    site: Site | undefined,
    register: Register<Options, Fn>,
    name: string,
    options: Options,
    fn: Fn
): unknown {
    if (site === undefined) {
        return register(name, options, fn);
    }

    const key = `${site.scriptName}:${site.lineNumber}:${site.columnNumber}`;
    let caller = callers.get(key);
    if (caller === undefined) {
        caller = compileCaller(site);
        callers.set(key, caller);
    }

    return caller(register as Register<unknown, unknown>, name, options, fn);
}

/** A function whose call to `register` lies at the site */
function compileCaller(site: Site): Caller {
    // The call starts the body's second line, which `lineOffset` puts at the
    // site's line, indented to its column
    const indent = ' '.repeat(site.columnNumber - 1);
    const body = `return (\n${indent}register(name, options, fn));`;

    return compileFunction(body, ['register', 'name', 'options', 'fn'], {
        filename: site.scriptName,
        lineOffset: site.lineNumber - 2
    }) as Caller;
}
