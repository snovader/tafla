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
        'unicorn/escape-case': 'off',
        // A class's own fields are `declare private _name`, since V8 holds
        // a `#name` field, which is defined before it's assigned, as mutable
        // and of any type
        'eslint/no-underscore-dangle': ['error', { allowAfterThis: true }]
    },
    overrides: [
        {
            // Written by hand in the shape that flatc generates
            files: [
                'benchmarks/comparison/generated/flatbuffers.ts',
                'benchmarks/strings/generated/flatbuffers.ts'
            ],
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
