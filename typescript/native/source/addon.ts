import { createRequire } from 'node:module';

/**
 * The addon's module, which `build.ts` builds for one platform. Each of the
 * package's modules declares the functions of it that it exports.
 */
export const addon: unknown = createRequire(import.meta.url)(
    '../tafla-native.node'
);
