import { defineConfig } from 'vitest/config';
import { bigEndianProject } from './core/vitest.config.ts';

const config = defineConfig({
    test: {
        // Bundles the package once per run, for every project to import
        globalSetup: ['./vitest.globalSetup.ts'],
        projects: ['benchmarks', 'core', 'lint', bigEndianProject]
    }
});

export default config;
