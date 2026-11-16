// tafla's rules for oxlint, which loads them as a JS plugin

import type { RuleTester } from 'oxlint/plugins-dev';

// oxlint names the types of its plugins' API only in RuleTester's
export type Rule = Parameters<RuleTester['run']>[1];
type Context = Parameters<NonNullable<Rule['create']>>[0];
type Visitor = ReturnType<NonNullable<Rule['create']>>;
type ImportDeclaration = Parameters<
    NonNullable<Visitor['ImportDeclaration']>
>[0];
type ImportDeclarationSpecifier = ImportDeclaration['specifiers'][number];
type ImportSpecifier = Extract<
    ImportDeclarationSpecifier,
    { type: 'ImportSpecifier' }
>;
type Report = Parameters<Context['report']>[0];
type Fix = NonNullable<Report['fix']>;

// Natural order, ignoring case: `SIZEOF_8` before `SIZEOF_16`
const COLLATOR = new Intl.Collator('en', {
    numeric: true,
    sensitivity: 'base'
});

/**
 * The group of an imported name: functions and other values whose names
 * start in lower case, then classes and constants, then other names, then
 * types
 */
function groupOf(
    specifier: ImportSpecifier,
    declaration: ImportDeclaration
): number {
    if (declaration.importKind === 'type' || specifier.importKind === 'type') {
        return 3;
    }

    const first = specifier.local.name.charAt(0);
    if (first >= 'a' && first <= 'z') {
        return 0;
    }
    if (first >= 'A' && first <= 'Z') {
        return 1;
    }

    return 2;
}

function isImportSpecifier(
    node: ImportDeclarationSpecifier
): node is ImportSpecifier {
    return node.type === 'ImportSpecifier';
}

interface SortEntry {
    readonly specifier: ImportSpecifier;
    readonly group: number;
}

function compareEntries(left: SortEntry, right: SortEntry): number {
    return (
        left.group - right.group ||
        COLLATOR.compare(left.specifier.local.name, right.specifier.local.name)
    );
}

/** The names of an import in the order that the rule expects */
function sortedSpecifiers(
    declaration: ImportDeclaration
): readonly [ImportSpecifier[], ImportSpecifier[]] {
    const specifiers = declaration.specifiers.filter(isImportSpecifier);
    const entries = specifiers.map(specifier => ({
        specifier,
        group: groupOf(specifier, declaration)
    }));
    const sorted = entries
        .toSorted(compareEntries)
        .map(entry => entry.specifier);

    return [specifiers, sorted];
}

/** Writes the text of each name where the sorted order puts it */
function moveSpecifiers(
    context: Context,
    specifiers: readonly ImportSpecifier[],
    sorted: readonly ImportSpecifier[]
): Fix {
    return fixer =>
        specifiers.map((specifier, i) =>
            fixer.replaceText(specifier, context.sourceCode.getText(sorted[i]))
        );
}

const sortNamedImports: Rule = {
    meta: {
        type: 'layout',
        fixable: 'code',
        docs: {
            description:
                'Orders the names in braces of an import: names in lower case, then upper case, then types, each in natural order'
        }
    },
    create(context) {
        return {
            ImportDeclaration(declaration): void {
                const [specifiers, sorted] = sortedSpecifiers(declaration);
                const index = specifiers.findIndex(
                    (specifier, i) => specifier !== sorted[i]
                );
                if (index === -1) {
                    return;
                }

                // Names can't move without the comments between them
                const movable =
                    context.sourceCode.getCommentsInside(declaration).length ===
                    0;
                const expected = sorted[index].local.name;
                const found = specifiers[index].local.name;

                context.report({
                    node: specifiers[index],
                    message: `Expected "${expected}" before "${found}": names in lower case, then upper case, then types, each in natural order`,
                    fix: movable
                        ? moveSpecifiers(context, specifiers, sorted)
                        : undefined
                });
            }
        };
    }
};

interface Plugin {
    readonly meta: { readonly name: string };
    readonly rules: Readonly<Record<string, Rule>>;
}

const plugin: Plugin = {
    meta: { name: 'tafla' },
    rules: { 'sort-named-imports': sortNamedImports }
};

export default plugin;
