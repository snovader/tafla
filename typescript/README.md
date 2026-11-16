# tafla for TypeScript

`core/` holds `tafla`, the runtime. Its tests also generate the
repository's conformance suite. `benchmarks/` times it, and compares it with
other libraries; its README describes how.

## Development

This is a Yarn project with its release in `.yarn/releases`, so `yarn` runs the
pinned version from here. Run the commands below from this directory.

```sh
yarn install
yarn format:check
yarn lint:check
yarn typecheck
yarn test
```

`yarn format:write` and `yarn lint:write` fix what they can.
`TAFLA_LARGE_BUFFER_TESTS=1 yarn test` also runs the tests that allocate
buffers of more than 4 GiB, which are skipped by default.
`yarn conformance:generate` regenerates the conformance suite at the root of the
repository from `core/test/conformance/cases.ts`, and the tables of the
generated code in its `generated` directory from
`core/test/generated/shapesCases.ts`.

`core/test/generated` holds the TypeScript and JavaScript that tafla-gen
generates from the example schemas in `schemas/`, in a directory of each
schema, and the tests that run them, of the JavaScript through its JSDoc
types. They're the generator's golden files, which `rust/gen/README.md`
describes: rewrite them with the generator rather than by hand.

Tests run in Node's own module loader, which strips the types of `.ts` files.
CI runs them in Node 24 and 26.

## Source and build

Within this project, the `tafla-source` condition resolves `tafla` and
its `#` imports to its TypeScript source: the TypeScript configs set it in
`customConditions`, and the generators of the conformance suite pass it to
Node. Without it, as for the package's users, they resolve to the build in
`dist`.

Tests and benchmarks import the package bundled, as it's published. The root
Vitest config's global setup bundles it once per run, into
`core/.bundle`, which the `tafla-bundle` condition resolves, and
removes the bundle after the run. The big-endian tests run in a project of
their own, of the source, since they emulate a big-endian host by mocking
`source/constants.ts`, which the bundle has no module of. `yarn bench:compare`
runs the comparison
through `bundle.ts`, which does the same around a command.

`yarn workspace tafla build` bundles the package into `dist/index.js` with
rolldown, and compiles its declarations to `dist` with tsc. In one module,
the package's functions and classes are local, and rolldown writes the value
of each constant, such as `SIZEOF_64`, where it's used: V8 loads an imported
binding from its module at every use, even in optimized code.
`core/rolldown.config.ts` configures the bundle. Packing the package builds
it first. The package holds only `dist`, so its `publishConfig`
replaces `exports` and `imports` with copies without the condition, which
Yarn writes into the packed manifest. Pack and publish it with Yarn, since npm
doesn't apply them. `yarn package:check` packs it, checks the archive with Are
the Types Wrong and publint, and imports the build to read back a table.
