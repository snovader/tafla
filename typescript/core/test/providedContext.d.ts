// The values that `core/vitest.config.ts` provides, which tests read with
// `inject()`
declare module 'vitest' {
    export interface ProvidedContext {
        /** Whether to run tests that allocate buffers of more than 4 GiB */
        largeBufferTests: boolean;
    }
}

// oxlint-disable-next-line unicorn/require-module-specifiers -- A module, so that it augments vitest's
export {};
