# @tafla/gen

tafla-gen, tafla's schema compiler, for npm. It compiles a schema of
messages, tables and structs into code that writes and reads them with
tafla's runtime for TypeScript, JavaScript, Python, Java, Rust, Go, C++,
Swift or C#. The package runs the generator's WebAssembly module, so it needs
no Rust toolchain and is the same on every platform, and it depends on
nothing.

[The generator's README](https://github.com/snovader/tafla/tree/main/rust/gen#readme)
describes the schema language, the generated code and the command.

## Command

```sh
npx @tafla/gen typescript shapes.tafla --out source/generated
```

In a project that depends on the package, the command is `tafla-gen`, as in
a script of its `package.json`:

```json
{
    "scripts": {
        "generate": "tafla-gen typescript schemas/shapes.tafla --out source/generated"
    },
    "devDependencies": {
        "@tafla/gen": "^0.1.0"
    }
}
```

It takes the arguments, prints the errors and exits with the codes of the
`tafla-gen` that `cargo install tafla-gen` installs, since the generator's
module decides them for both. `tafla-gen --help` describes them. The message
of a file that can't be read or written is Node's rather than Rust's.

The generated TypeScript and JavaScript import tafla's runtime from
[`tafla`](https://www.npmjs.com/package/tafla), which the project that runs
them depends on.

## API

`compile()` generates a schema's files from its source, without reading or
writing a file, for a build tool's plugin or a script.

```ts
import { compile, CompileError } from '@tafla/gen';

const source = 'message Point { x: float64; y: float64; }';
const files = compile('point.tafla', source, 'typescript');
// [{ path: 'point.ts', contents: '…' }]

try {
    compile('point.tafla', 'message Point { x: flaot64; }', 'typescript');
} catch (error) {
    if (error instanceof CompileError) {
        // error: unknown type `flaot64`
        //  --> point.tafla:1:20
        console.error(error.message);
    }
}
```

- The file name names the generated files, and the schema in their headers
  and in errors.
- The target is a language's name or abbreviation, as the command takes it.
- The options are `runtime`, the module that the generated code imports
  tafla's runtime from, and `package`, as the command's `--runtime` and
  `--package`.
- Each file's path is relative to the directory that the files belong under,
  and within it.

It throws a `CompileError`, whose message is the errors as the command prints
them, for a schema with errors, an unknown target, and a package that isn't a
Java package.

`compile()` is synchronous. Its first call reads and instantiates the
module, of about 590 KB, from the package's files, which takes Node; later
calls reuse it.

## Versions

The package's version is the generator's, which
[its changelog](https://github.com/snovader/tafla/blob/main/rust/gen/CHANGELOG.md)
describes: `@tafla/gen` and the crate `tafla-gen` of the same version
generate the same code. The changelog names the release of a language's
runtime that the generated code needs, when it needs a newer one.

## Development

[The TypeScript README](https://github.com/snovader/tafla/tree/main/typescript#readme)
describes the project. `yarn test:gen`, from `typescript/`, builds the
generator's module with cargo, which needs the `wasm32-unknown-unknown`
target, builds the package, and runs its tests against the build.
