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
    // This project's package, which sorts after the other packages
    sortImports: { ...oxfmt.sortImports, internalPattern: ['tafla'] }
});
