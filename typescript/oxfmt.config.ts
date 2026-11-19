import { defineConfig } from 'oxfmt';

export default defineConfig({
    ignorePatterns: [
        '.cache',
        'core/.bundle',
        '.yarn',
        '**/dist',
        // Yarn writes these in a format of its own
        '**/package.json',
        // Prose wrapped by hand, as in the other languages' READMEs
        '**/*.md'
    ],
    endOfLine: 'lf',
    tabWidth: 4,
    printWidth: 80,
    singleQuote: true,
    quoteProps: 'as-needed',
    trailingComma: 'none',
    semi: true,
    arrowParens: 'avoid',
    bracketSpacing: true,
    bracketSameLine: false,
    objectWrap: 'preserve',
    // Node's modules, hypotes, other packages, this project's package and its
    // `#` imports, then relative paths, nearest last
    sortImports: {
        customGroups: [
            {
                groupName: 'hypotes',
                elementNamePattern: ['hypotes', 'hypotes/**']
            }
        ],
        groups: [
            'builtin',
            'hypotes',
            'external',
            ['internal', 'subpath'],
            'parent',
            ['sibling', 'index'],
            'unknown'
        ],
        internalPattern: ['tafla'],
        newlinesBetween: false
    },
    overrides: [
        {
            files: ['**/*.test.ts'],
            options: { printWidth: 100 }
        },
        {
            files: ['**/tsconfig.json', '**/tsconfig.*.json'],
            options: { printWidth: 60 }
        }
    ]
});
