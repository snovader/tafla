import oxlint from '@snovader/oxc-config/oxlint';
import hypotes from '@snovader/oxc-config/oxlint/hypotes';
import { defineConfig } from 'oxlint';

export default defineConfig({
    extends: [oxlint, hypotes],
    ignorePatterns: ['.cache', 'core/.bundle', '.yarn', '**/dist'],
    rules: {
        // `.slice()`, which copies a typed array
        'unicorn/prefer-spread': 'off',
        // Escapes of code units in lower case, as JSON writes them
        'unicorn/escape-case': 'off'
    },
    overrides: [
        {
            // Written by hand in the shape that flatc generates
            files: ['benchmarks/comparison/generated/flatbuffers.ts'],
            rules: {
                'eslint/no-underscore-dangle': 'off',
                'typescript/no-unnecessary-type-assertion': 'off',
                'typescript/prefer-nullish-coalescing': 'off',
                'typescript/prefer-return-this-type': 'off',
                'unicorn/no-zero-fractions': 'off',
                'unicorn/prefer-bigint-literals': 'off'
            }
        }
    ]
});
