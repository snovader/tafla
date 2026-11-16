import { defineProject } from 'vitest/config';

const config = defineProject({
    test: {
        include: [],
        benchmark: {
            include: ['*.bench.ts']
        },
        // Resolve tafla to the bundle that the root config's global setup
        // writes, as it's published
        execArgv: ['--conditions=tafla-bundle'],
        experimental: {
            viteModuleRunner: false
        }
    }
});

export default config;
