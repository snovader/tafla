// Rules for tests that run on test-runner, which declares them through
// node:test with Vitest's API, in place of oxlint's vitest rules, which see
// only the tests and assertions imported from vitest. A test is a call of
// `it` or `test` from test-runner, its fast-check subpath or node:test, and a
// describe block is a call of `describe` or `suite` from them, through any of
// their variants, such as `it.only`, and of the functions that `it.each()`,
// `it.for()`, `it.prop()`, `it.skipIf()` and `it.runIf()` return. An
// assertion is a call of test-runner's `expect`, or, to expect-expect, of any
// function whose name starts with `expect`.
import type { Context, Fix, NodeOf, Rule } from './types.ts';

type CallExpression = NodeOf<'CallExpression'>;
type Argument = CallExpression['arguments'][number];
type Identifier = NodeOf<'Identifier'>;
type Node = CallExpression['parent'];
type StaticMemberExpression = Extract<
    NodeOf<'MemberExpression'>,
    { computed: false; property: { type: 'Identifier' } }
>;
type Scope = ReturnType<Context['sourceCode']['getScope']>;
type Variable = Scope['variables'][number];

/** What an imported name is to these rules */
type Role = 'describe' | 'expect' | 'test';

/** Members of `it`, `test`, `describe` and `suite` that are called as they are */
const VARIANTS: ReadonlySet<string> = new Set(['only', 'skip', 'todo']);

/**
 * Members of `it`, `test`, `describe` and `suite` that return the function
 * that declares, as `it.each(rows)` does
 */
const FACTORIES: ReadonlySet<string> = new Set([
    'each',
    'for',
    'prop',
    'runIf',
    'skipIf'
]);

/** Members between `expect()` and its matcher */
const MODIFIERS: ReadonlySet<string> = new Set(['not', 'rejects', 'resolves']);

/** Modifiers whose matchers return promises */
const ASYNC_MODIFIERS: ReadonlySet<string> = new Set(['rejects', 'resolves']);

/** Matchers of one value, for which others may say more */
const EQUALITY_MATCHERS: ReadonlySet<string> = new Set([
    'toBe',
    'toEqual',
    'toStrictEqual'
]);

const NOT: ReadonlySet<string> = new Set(['not']);

const LENGTH: ReadonlySet<string> = new Set(['length']);

/** An `expect()` call, the modifiers after it, such as `.not`, and its matcher */
interface ExpectChain {
    readonly expect: CallExpression;
    readonly modifiers: readonly StaticMemberExpression[];
    /** `.toBe`, if the chain names a matcher */
    readonly matcher: StaticMemberExpression | undefined;
    /** `.toBe(expected)`, if the chain calls its matcher */
    readonly matcherCall: CallExpression | undefined;
}

/** A chain that calls toBe(), toEqual() or toStrictEqual() */
interface Comparison extends ExpectChain {
    readonly matcher: StaticMemberExpression;
    readonly matcherCall: CallExpression;
}

/** The modules that declare tests, which test-runner declares them through */
const TEST_MODULES: ReadonlySet<string> = new Set([
    'node:test',
    'test-runner',
    'test-runner/fast-check'
]);

function roleOfImport(source: string, name: string): Role | undefined {
    if (!TEST_MODULES.has(source)) {
        return undefined;
    }

    switch (name) {
        case 'describe':
        case 'suite':
            return 'describe';
        case 'it':
        case 'test':
            return 'test';
        case 'expect':
            return source === 'test-runner' ? 'expect' : undefined;
        default:
            return undefined;
    }
}

/** The variable that an identifier refers to, from the scopes around it */
function variableOf(
    context: Context,
    identifier: Identifier
): Variable | undefined {
    let scope: Scope | null = context.sourceCode.getScope(identifier);
    while (scope !== null) {
        const variable = scope.set.get(identifier.name);
        if (variable !== undefined) {
            return variable;
        }
        scope = scope.upper;
    }

    return undefined;
}

/** What an identifier is to these rules, from the import that it refers to */
function roleOf(context: Context, identifier: Identifier): Role | undefined {
    const definition = variableOf(context, identifier)?.defs[0];
    if (definition?.type !== 'ImportBinding') {
        return undefined;
    }

    const { node: specifier, parent: declaration } = definition;
    if (
        specifier.type !== 'ImportSpecifier' ||
        declaration?.type !== 'ImportDeclaration'
    ) {
        return undefined;
    }

    const { imported } = specifier;
    const name =
        imported.type === 'Identifier' ? imported.name : imported.value;

    return roleOfImport(declaration.source.value, name);
}

/** Whether a node is a member named in the source, as `only` is in `it.only` */
function isStaticMember(node: Node | null): node is StaticMemberExpression {
    return (
        node?.type === 'MemberExpression' &&
        !node.computed &&
        node.property.type === 'Identifier'
    );
}

/** Whether a node is a member that one of the names names */
function isNamedMember(
    node: Node,
    names: ReadonlySet<string>
): node is StaticMemberExpression {
    return isStaticMember(node) && names.has(node.property.name);
}

/** The member of a node after it, as `.not` is of `expect(actual)` */
function memberAfter(
    node: Node,
    names?: ReadonlySet<string>
): StaticMemberExpression | undefined {
    const { parent } = node;
    if (!isStaticMember(parent) || parent.object !== node) {
        return undefined;
    }

    return names === undefined || names.has(parent.property.name)
        ? parent
        : undefined;
}

/** The nodes around a node, from its parent out */
function* ancestorsOf(node: Node): Generator<Node> {
    for (
        let ancestor = node.parent;
        ancestor !== null;
        ancestor = ancestor.parent
    ) {
        yield ancestor;
    }
}

function isFunction(node: Node): boolean {
    return (
        node.type === 'ArrowFunctionExpression' ||
        node.type === 'FunctionDeclaration' ||
        node.type === 'FunctionExpression'
    );
}

/** Whether a call is a test or a describe block */
function blockOf(
    context: Context,
    call: CallExpression
): 'describe' | 'test' | undefined {
    const target = declarerOf(call.callee);
    if (target.type !== 'Identifier') {
        return undefined;
    }

    const role = roleOf(context, target);
    return role === 'describe' || role === 'test' ? role : undefined;
}

/**
 * What a call's callee declares through: `it` for `it()`, `it.skip()`,
 * `it.each(rows)()` and `it.skipIf(condition).only()`
 */
function declarerOf(callee: CallExpression['callee']): Node {
    let target: Node = callee;

    for (;;) {
        if (
            target.type === 'CallExpression' &&
            isNamedMember(target.callee, FACTORIES)
        ) {
            target = target.callee.object;
        } else if (isNamedMember(target, VARIANTS)) {
            target = target.object;
        } else {
            return target;
        }
    }
}

/** Whether a call passes a function written in place as its last argument */
function hasCallback(call: CallExpression): boolean {
    const last = call.arguments.at(-1);
    return (
        last?.type === 'ArrowFunctionExpression' ||
        last?.type === 'FunctionExpression'
    );
}

function isExpectCall(context: Context, call: CallExpression): boolean {
    return (
        call.callee.type === 'Identifier' &&
        roleOf(context, call.callee) === 'expect'
    );
}

/** The chain that a call starts, if it calls `expect` */
function expectChainOf(
    context: Context,
    call: CallExpression
): ExpectChain | undefined {
    if (!isExpectCall(context, call)) {
        return undefined;
    }

    const modifiers: StaticMemberExpression[] = [];
    let end: Node = call;
    let modifier = memberAfter(end, MODIFIERS);
    while (modifier !== undefined) {
        modifiers.push(modifier);
        end = modifier;
        modifier = memberAfter(end, MODIFIERS);
    }

    const matcher = memberAfter(end);
    const parent = matcher?.parent;
    const matcherCall =
        parent?.type === 'CallExpression' && parent.callee === matcher
            ? parent
            : undefined;

    return { expect: call, modifiers, matcher, matcherCall };
}

/** The chain that a call starts, if it compares with toBe(), toEqual() or toStrictEqual() */
function comparisonOf(
    context: Context,
    call: CallExpression
): Comparison | undefined {
    const chain = expectChainOf(context, call);
    if (chain === undefined) {
        return undefined;
    }

    const { matcher, matcherCall } = chain;
    if (
        matcher === undefined ||
        matcherCall === undefined ||
        !EQUALITY_MATCHERS.has(matcher.property.name)
    ) {
        return undefined;
    }

    return { ...chain, matcher, matcherCall };
}

function hasModifier(chain: ExpectChain, names: ReadonlySet<string>): boolean {
    return chain.modifiers.some(modifier => names.has(modifier.property.name));
}

/**
 * Whether an expect() call runs in a test, or in a function of its own,
 * which a test may call. A function passed to a call, such as the callback
 * of `forEach()`, runs where the call runs.
 */
function runsInTest(context: Context, call: CallExpression): boolean {
    for (const ancestor of ancestorsOf(call)) {
        if (ancestor.type === 'CallExpression') {
            const block = blockOf(context, ancestor);
            if (block !== undefined) {
                return block === 'test';
            }
        } else if (
            isFunction(ancestor) &&
            ancestor.parent?.type !== 'CallExpression'
        ) {
            return true;
        }
    }

    return false;
}

function isAwaitedOrReturned(call: CallExpression): boolean {
    const { parent } = call;

    return (
        parent.type === 'AwaitExpression' ||
        parent.type === 'ReturnStatement' ||
        (parent.type === 'ArrowFunctionExpression' && parent.body === call)
    );
}

function isNull(node: Argument): boolean {
    return node.type === 'Literal' && !('regex' in node) && node.value === null;
}

function isIdentifierNamed(node: Argument, name: string): boolean {
    return node.type === 'Identifier' && node.name === name;
}

/** Whether a node is a string, number, bigint or boolean written literally */
function isPrimitiveLiteral(node: Argument): boolean {
    switch (node.type) {
        case 'Literal':
            return !('regex' in node) && node.value !== null;
        case 'TemplateLiteral':
            return node.expressions.length === 0;
        case 'UnaryExpression':
            return (
                node.operator === '-' &&
                node.argument.type === 'Literal' &&
                (typeof node.argument.value === 'number' ||
                    typeof node.argument.value === 'bigint')
            );
        default:
            return false;
    }
}

/**
 * The matcher that says more than toBe(), toEqual() or toStrictEqual() of
 * the value expected: toBeNull() of null, for example, or toBe() of a
 * primitive
 */
function preferredMatcher(
    comparison: Comparison,
    expected: Argument
): string | undefined {
    if (isNull(expected)) {
        return 'toBeNull';
    }
    if (isIdentifierNamed(expected, 'undefined')) {
        return hasModifier(comparison, NOT) ? 'toBeDefined' : 'toBeUndefined';
    }
    if (isIdentifierNamed(expected, 'NaN')) {
        return 'toBeNaN';
    }

    const isToBe = comparison.matcher.property.name === 'toBe';
    return !isToBe && isPrimitiveLiteral(expected) ? 'toBe' : undefined;
}

/** Writes the preferred matcher in place of the comparison's */
function replaceMatcher(comparison: Comparison, preferred: string): Fix {
    const { expect, matcher, matcherCall, modifiers } = comparison;

    if (preferred === 'toBe') {
        return fixer => fixer.replaceText(matcher.property, 'toBe');
    }

    // toBeDefined() is not.toBeUndefined(), so it takes the place of `.not`
    if (preferred === 'toBeDefined') {
        const kept = modifiers
            .filter(modifier => modifier.property.name !== 'not')
            .map(modifier => `.${modifier.property.name}`)
            .join('');

        return fixer =>
            fixer.replaceTextRange(
                [expect.end, matcherCall.end],
                `${kept}.toBeDefined()`
            );
    }

    return fixer =>
        fixer.replaceTextRange(
            [matcher.property.start, matcherCall.end],
            `${preferred}()`
        );
}

export const expectExpect: Rule = {
    meta: {
        type: 'problem',
        docs: {
            description:
                'Requires each test written in place to call expect(), or a function whose name starts with expect'
        }
    },
    create(context) {
        // Tests that call no assertion so far
        const unasserted = new Set<CallExpression>();

        return {
            CallExpression(call): void {
                const { callee } = call;
                const isAssertion =
                    callee.type === 'Identifier' &&
                    callee.name.startsWith('expect');

                if (isAssertion) {
                    for (const ancestor of ancestorsOf(call)) {
                        if (ancestor.type === 'CallExpression') {
                            unasserted.delete(ancestor);
                        }
                    }
                } else if (
                    hasCallback(call) &&
                    blockOf(context, call) === 'test'
                ) {
                    unasserted.add(call);
                }
            },
            'Program:exit'(): void {
                for (const test of unasserted) {
                    context.report({
                        node: test.callee,
                        message:
                            'Expected the test to call expect(), or a function whose name starts with expect'
                    });
                }
            }
        };
    }
};

export const noStandaloneExpect: Rule = {
    meta: {
        type: 'problem',
        docs: {
            description:
                'Requires expect() to run in a test, or in a function of its own, rather than in a describe block or at the top of a module'
        }
    },
    create(context) {
        return {
            CallExpression(call): void {
                if (isExpectCall(context, call) && !runsInTest(context, call)) {
                    context.report({
                        node: call,
                        message:
                            'Expected expect() in a test, or in a function of its own, rather than in a describe block or at the top of the module'
                    });
                }
            }
        };
    }
};

export const validExpect: Rule = {
    meta: {
        type: 'problem',
        docs: {
            description:
                'Requires a matcher to be called on expect(), and the promise of .resolves or .rejects to be awaited or returned. TypeScript checks the arguments of expect().'
        }
    },
    create(context) {
        return {
            CallExpression(call): void {
                const chain = expectChainOf(context, call);
                if (chain === undefined) {
                    return;
                }

                const { matcherCall } = chain;
                if (matcherCall === undefined) {
                    context.report({
                        node: call,
                        message:
                            'Expected a matcher, such as toBe(), to be called on expect()'
                    });
                } else if (
                    hasModifier(chain, ASYNC_MODIFIERS) &&
                    !isAwaitedOrReturned(matcherCall)
                ) {
                    context.report({
                        node: matcherCall,
                        message:
                            'Expected the promise of .resolves or .rejects to be awaited or returned'
                    });
                }
            }
        };
    }
};

export const preferToBe: Rule = {
    meta: {
        type: 'suggestion',
        fixable: 'code',
        docs: {
            description:
                'Prefers toBe() of a primitive, which compares with Object.is, to toEqual() and toStrictEqual(), and toBeNull(), toBeUndefined(), toBeDefined() and toBeNaN() to comparisons with those values'
        }
    },
    create(context) {
        return {
            CallExpression(call): void {
                const comparison = comparisonOf(context, call);
                const [expected] = comparison?.matcherCall.arguments ?? [];
                if (
                    comparison === undefined ||
                    expected === undefined ||
                    comparison.matcherCall.arguments.length !== 1
                ) {
                    return;
                }

                const preferred = preferredMatcher(comparison, expected);
                if (preferred === undefined) {
                    return;
                }

                // toBeDefined() replaces `.not` too
                const matcher = comparison.matcher.property.name;
                const found =
                    preferred === 'toBeDefined' ? `not.${matcher}` : matcher;
                const value = context.sourceCode.getText(expected);
                context.report({
                    node: comparison.matcher.property,
                    message: `Use ${preferred}() rather than ${found}(${value})`,
                    fix: replaceMatcher(comparison, preferred)
                });
            }
        };
    }
};

export const preferToHaveLength: Rule = {
    meta: {
        type: 'suggestion',
        fixable: 'code',
        docs: {
            description:
                'Prefers expect(values).toHaveLength(n) to comparing expect(values.length) with n'
        }
    },
    create(context) {
        return {
            CallExpression(call): void {
                const comparison = comparisonOf(context, call);
                const [actual] = call.arguments;
                if (
                    comparison === undefined ||
                    call.arguments.length !== 1 ||
                    actual === undefined ||
                    !isNamedMember(actual, LENGTH)
                ) {
                    return;
                }

                const values = context.sourceCode.getText(actual.object);
                context.report({
                    node: actual,
                    message: `Use toHaveLength() rather than comparing ${values}.length`,
                    fix: fixer => [
                        fixer.replaceText(actual, values),
                        fixer.replaceText(
                            comparison.matcher.property,
                            'toHaveLength'
                        )
                    ]
                });
            }
        };
    }
};
