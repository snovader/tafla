import { readFileSync } from 'node:fs';
import { GeneratorModule } from './wasm-module.ts';

let generator: GeneratorModule | undefined;

/**
 * The generator's WebAssembly module, which the package holds beside its
 * `source` and `dist`, instantiated at its first use
 */
export function loadGenerator(): GeneratorModule {
    generator ??= new GeneratorModule(
        readFileSync(new URL('../tafla_gen.wasm', import.meta.url))
    );
    return generator;
}
