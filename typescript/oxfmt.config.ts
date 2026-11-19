import oxfmt from '@snovader/oxc-config/oxfmt';
import { defineConfig } from 'oxfmt';

export default defineConfig({
    ...oxfmt,
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
    // Node's modules, hypotes, other packages, this project's package and its
    // `#` imports, then relative paths, nearest last. The shared order sorts
    // hypotes among the other packages, and `#` imports after the package's
    // name.
    sortImports: {
        ...oxfmt.sortImports,
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
        internalPattern: ['tafla']
    }
});
