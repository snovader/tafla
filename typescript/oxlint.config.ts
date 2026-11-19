import oxlint from '@snovader/oxc-config/oxlint';
import hypotes from '@snovader/oxc-config/oxlint/hypotes';
import { defineConfig } from 'oxlint';

export default defineConfig({
    extends: [oxlint, hypotes],
    ignorePatterns: ['.cache', 'core/.bundle', '.yarn', '**/dist'],
    rules: {
        // Arrays allocated at their length and filled by index, as generated
        // code does
        'unicorn/no-new-array': 'off',
        // UTF-16 code units, which encoding and its tests handle one by one,
        // conversions to int32, and `.slice()`, which copies a typed array
        'unicorn/prefer-code-point': 'off',
        'unicorn/prefer-math-trunc': 'off',
        'unicorn/prefer-spread': 'off',
        // Regular expressions of code units, as JSON escapes them
        'eslint/require-unicode-regexp': 'off',
        'unicorn/escape-case': 'off',
        // Indexed access, rather than `.at()`, which the benchmarks compare
        'unicorn/prefer-at': 'off',
        // Scripts that write files write them in order
        'eslint/no-await-in-loop': 'off'
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
