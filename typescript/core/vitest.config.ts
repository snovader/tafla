import { defineProject, type UserWorkspaceConfig } from 'vitest/config';

// Emulates a big-endian host by mocking `source/constants.ts`, a module that
// the bundle doesn't have
const BIG_ENDIAN_TESTS = 'test/bigEndian.test.ts';

/**
 * A project of the package's tests, which resolve `tafla` with `condition`:
 * `tafla-bundle` for the bundle that the root config's global setup writes,
 * as it's published, or `tafla-source` for its modules
 */
function testProject(
    condition: 'tafla-bundle' | 'tafla-source',
    test: { name?: string; include: string[]; exclude?: string[] }
): UserWorkspaceConfig {
    return defineProject({
        root: import.meta.dirname,
        test: {
            ...test,
            pool: 'threads',
            execArgv: [`--conditions=${condition}`],
            // Run tests in Node's own module loader, as the package runs,
            // rather than in Vite's
            experimental: {
                viteModuleRunner: false
            },
            // The tests can't read the environment themselves, as they're
            // typed without Node's globals
            provide: {
                // Tests that allocate buffers of more than 4 GiB
                largeBufferTests: process.env.TAFLA_LARGE_BUFFER_TESTS === '1'
            }
        }
    });
}

/** The big-endian tests, of the package's modules */
export const bigEndianProject = testProject('tafla-source', {
    name: 'tafla (big-endian)',
    include: [BIG_ENDIAN_TESTS]
});

const config = testProject('tafla-bundle', {
    include: ['test/**/*.test.ts'],
    exclude: [BIG_ENDIAN_TESTS]
});

export default config;
