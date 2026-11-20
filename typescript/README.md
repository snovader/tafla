# tafla for TypeScript

`core/` holds `tafla`, the runtime. Its tests also generate the
repository's conformance suite. `benchmarks/` times it, and compares it with
other libraries; its README describes how. `gen/` holds `@tafla/gen`, the
npm package of tafla-gen, the schema compiler in `rust/gen`, whose
WebAssembly module it runs.

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

`yarn test:gen` builds `@tafla/gen` and runs its tests against the build,
as its users run it. It builds the generator's module with cargo, so it needs
Rust with the `wasm32-unknown-unknown` target, which the other commands
don't: `yarn typecheck` checks the package's source without the module.

`yarn format:write` and `yarn lint:write` fix what they can.
`TAFLA_LARGE_BUFFER_TESTS=1 yarn test` also runs the tests that allocate
buffers of more than 4 GiB, which are skipped by default. `yarn test:watch`
runs the tests again as their files and the package's source change.
`yarn conformance:generate` regenerates the conformance suite at the root of the
repository from `core/test/conformance/cases.ts`, and the tables of the
generated code in its `generated` directory from
`core/test/generated/shapes-cases.ts`.

The configurations of oxfmt, oxlint and TypeScript extend the shared ones of
[`@snovader/oxc-config`](https://github.com/snovader/tooling/tree/main/packages/oxc-config)
and
[`@snovader/tsconfig`](https://github.com/snovader/tooling/tree/main/packages/tsconfig),
which need Node 26 to load oxfmt's and oxlint's. `oxfmt.config.ts` adds the
paths that oxfmt ignores and the name `tafla`, which sorts after the other
packages. `oxlint.config.ts` adds the rules that tafla turns off. Files are
named in kebab-case, as the shared rule checks, and as tafla-gen names the
modules that it generates. `tsconfig.base.json`,
`tsconfig.node.json` and `tsconfig.isomorphic.json` extend the presets of
the same names, with the composite preset for `tsc -b`, and add the
`tafla-source` condition; every project here extends one of them.

`core/test/generated` holds the TypeScript and JavaScript that tafla-gen
generates from the example schemas in `schemas/`, in a directory of each
schema, and the tests that run them, of the JavaScript through its JSDoc
types. They're the generator's golden files, which `rust/gen/README.md`
describes: rewrite them with the generator rather than by hand.

Tests run in Node's test runner, `node:test`, through
[hypotes](https://github.com/snovader/hypotes), which declares them with
Vitest's API and asserts with Vitest's matchers.
Tests import `describe`, `it`, `expect` and `vi` from `hypotes`, as they would
from `vitest`, and `fc` and `it`, with `it.prop()`, from `hypotes/fast-check`,
as they would from `@fast-check/vitest`. oxlint's rules for Vitest see only
what is imported from `vitest`, so the rules of `hypotes/oxlint` lint these
tests, such as that each test asserts. CI runs them in Node 24 and 26.

Node runs this project's TypeScript through the hook of
[`@snovader/node-hooks`](https://github.com/snovader/tooling/tree/main/packages/node-hooks),
which strips the types with oxc, rather than through its own stripping. Every
script that runs a `.ts` file passes Node
`--no-strip-types --import @snovader/node-hooks/register`, for `bundle.ts`
and for the command that it runs, since the tests and the benchmarks are
TypeScript even when the package comes from the bundle. Pass the same flags
to run a `.ts` file by hand. Node's own stripping of a module larger than
about 1.2–1.7 KB detaches an `ArrayBuffer`, and V8's optimized code then
checks every access to a typed array for a detached buffer, for the rest of
the process. Developing needs Node 24.13.1 or later: the hooks need 24.12,
and hypotes 24.13.1. The package itself supports the versions that its
`engines` names.

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
has no module of. `vi.mock()` needs the preload `hypotes/register`, which
that process and `yarn test:watch` import after `@snovader/node-hooks`.
Node runs the hook registered last first, and the TypeScript hook ends the
chain, so hypotes' hook hoists the mocks in the JavaScript that oxc writes.
`yarn test:watch` runs every test of the source, each file in a process of
its own, as nothing bundles the package again when it changes.

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
