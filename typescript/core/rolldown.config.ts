import { join } from 'node:path';
import { defineConfig, type BuildOptions } from 'rolldown';

/**
 * Bundles the package's modules into one module at `file`, as it's published
 * and as its tests and benchmarks import it.
 *
 * V8 loads an imported binding from its module at every use, and checks that
 * it's initialized: 5 bytes of bytecode where a literal or a local function
 * takes 2 or 3, which its optimized code keeps too. In one module, functions
 * and classes are local, and `inlineConst` writes the value of an imported
 * constant, such as `SIZEOF_64`, at each use. Without it, the bundle exports
 * the constants, and its uses still load them as module variables.
 *
 * V8 also keeps a module's source text on its heap for as long as the module
 * lives, at two bytes per character if any character is outside Latin-1.
 * Comments were nearly half of the bundle, and a few en dashes in them made
 * every character take two bytes, so the bundle leaves its comments out. The
 * declarations keep the JSDoc. The `@__PURE__` annotations go too, and
 * rolldown tree-shakes a consumer's imports from the bundle to the same code
 * without them.
 */
export function bundleOptions(file: string): BuildOptions {
    return {
        input: join(import.meta.dirname, 'source/index.ts'),
        platform: 'neutral',
        // As `tsconfig.dist.json` compiles the declarations
        transform: { target: 'es2022' },
        optimization: { inlineConst: { mode: 'all', pass: 1 } },
        output: { file, format: 'esm', comments: false }
    };
}

export default defineConfig(
    bundleOptions(join(import.meta.dirname, 'dist/index.js'))
);
