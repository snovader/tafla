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
buffers of more than 4 GiB, which are skipped by default. `yarn test:watch`
runs the tests again as their files and the package's source change.
`yarn conformance:generate` regenerates the conformance suite at the root of the
repository from `core/test/conformance/cases.ts`, and the tables of the
generated code in its `generated` directory from
`core/test/generated/shapesCases.ts`.

`core/test/generated` holds the TypeScript and JavaScript that tafla-gen
generates from the example schemas in `schemas/`, in a directory of each
schema, and the tests that run them, of the JavaScript through its JSDoc
types. They're the generator's golden files, which `rust/gen/README.md`
describes: rewrite them with the generator rather than by hand.

Tests run in Node's test runner, `node:test`, whose module loader strips the
types of `.ts` files. They assert with Vitest's matchers, from
`@vitest/expect`, which `core/test/conformance/expect.ts` sets up on chai and
exports, and declare property tests with `prop()`, from
`core/test/conformance/property.ts`, which runs them with fast-check. oxlint's
rules for Vitest see only what is imported from `vitest`, so
`lint/testRules.ts` has rules of its own for these tests, such as that each
test asserts. CI runs them in Node 24 and 26. The big-endian tests mock a module with the `exports`
option of `mock.module()`, which Node 24 has from 24.15.

## Source and build

Within this project, the `tafla-source` condition resolves `tafla` and
its `#` imports to its TypeScript source: the TypeScript configs set it in
`customConditions`, and the generators of the conformance suite pass it to
Node. Without it, as for the package's users, they resolve to the build in
`dist`.

Tests and benchmarks import the package bundled, as it's published.
`bundle.ts` bundles it into `core/.bundle`, which the `tafla-bundle`
condition resolves, runs a command, and removes the bundle. `yarn test` runs
the tests through it, all in one process, and `yarn bench` and
`yarn bench:compare` run the benchmarks through it. The big-endian tests run
after the others, in a process of their own, of the source, since they
emulate a big-endian host by mocking `source/constants.ts`, which the bundle
has no module of. `yarn test:watch` runs every test of the source, each file
in a process of its own, as nothing bundles the package again when it
changes.

`yarn workspace tafla build` bundles the package into `dist/index.js` with
rolldown, and compiles its declarations to `dist` with tsc. In one module,
the package's functions and classes are local, and rolldown writes the value
of each constant, such as `SIZEOF_64`, where it's used: V8 loads an imported
binding from its module at every use, even in optimized code. The bundle
leaves out the comments, which the declarations keep, and `yarn package:check`
checks that it has no character outside Latin-1: V8 keeps a module's source
on its heap, at two bytes per character if one is outside Latin-1.
`core/rolldown.config.ts` configures the bundle. Packing the package builds
it first. The package holds only `dist`, so its `publishConfig`
replaces `exports` and `imports` with copies without the condition, which
Yarn writes into the packed manifest. Pack and publish it with Yarn, since npm
doesn't apply them. `yarn package:check` builds it and imports the build to
read back a table.
