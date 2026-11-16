import { defineConfig } from 'vitest/config';

const config = defineConfig({
    test: {
        projects: ['benchmarks', 'core', 'lint']
    }
});

export default config;
