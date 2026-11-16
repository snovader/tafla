import { defineProject } from 'vitest/config';

const config = defineProject({
    test: {
        include: ['*.test.ts'],
        experimental: {
            viteModuleRunner: false
        }
    }
});

export default config;
