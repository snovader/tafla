# Code and design guidelines

These repository guidelines apply when designing, implementing, reviewing, and
refactoring code. Use judgment about tradeoffs; existing code is evidence of
intent, not a requirement to preserve every detail.

## Responsibilities and architecture

- Give each function, class, and module a clear scope of responsibility. Keep
  layout, writing, reading, and code generation with their respective owners.
  For example, `TableLayout` describes a table's header and offset type,
  `TableBuilder` writes tables, and `ByteBuffer` reads them. In the generator,
  the parser owns syntax, the checker owns a schema's rules, and each target
  owns its language's code.
- Search for existing implementations and related abstractions before adding
  code. Always consider reusability and where behavior belongs. Keep a helper
  local when its responsibility is local; share it when it has a coherent role
  across callers.
- Use classes for encapsulated state, identity, lifecycle, or meaningful behavior.
  Use functions for stateless operations. Builders and caches can own mutable
  state when that is part of their purpose.
- Introduce interfaces at meaningful boundaries or useful substitution points.
  Inject dependencies directly through constructors or callbacks, as with a
  benchmark suite's codecs. A hypothetical future implementation alone needs
  stronger justification.
- A stable domain-facing API can justify a thin delegating wrapper, provided its
  purpose is clear. Do not remove such a wrapper merely because it delegates.
- Respect package exports and import maps. Keep dependencies consistent with the
  responsibility of the package and consider their runtime and bundle costs.

## Functions and readability

- Prefer small, focused functions with intentional names. A single caller does
  not prevent extraction: naming a meaningful operation or clarifying its scope
  is sufficient. Avoid arbitrary function-length limits.
- Use whitespace deliberately to separate logical sections within a function,
  such as preparation, computation, side effects, and the result. Keep closely
  related statements together.
- Avoid chained calls with large inline function bodies. Extract substantial
  callbacks into named functions or methods so the overall operation stays easy
  to follow.
- Review adjacent multiline calls and deeply nested expressions for readability.
  Use blank lines between distinct steps and meaningful intermediate variables
  where they clarify the sequence. Preserve efficient execution when restructuring
  expressions; readability does not require extra copies or traversals.
- Use domain terminology consistently. Comments should explain non-obvious
  constraints, algorithms, units, external behavior, or implementation tradeoffs.
  Document public contracts where callers need that information.

## Encapsulation and API design

- Prefer private fields and method-style accessors, especially when access
  performs computation, validation, caching, or other logic. Use `readonly` where
  the field should not be reassigned. This preference can evolve when language
  features offer a compelling improvement.
- Make construction and conversion intentional, using named factories or
  conversion methods where they clarify semantics.
- Make call sites understandable. Use options objects for related configuration
  in TypeScript and appropriate language idioms elsewhere, such as keyword-only
  options in Python. Preserve domain semantics across languages without forcing
  identical syntax on every language.
- Use types to express contracts, including identifiers, units, and supported
  representations where useful. Follow the existing explicit return types and
  type-only imports at API boundaries.
- Decide failure behavior explicitly for each API. Consider possible inputs,
  who calls it, and how its output will be consumed before choosing exceptions,
  missing values, non-finite results, or fallbacks. Document and test that choice.
- Validate external data at appropriate boundaries and establish internal
  invariants deliberately. Avoid silently replacing invalid results with plausible
  values unless that is the intended contract.
- Don't recheck at runtime what documented or generated usage already
  guarantees. Enforce correctness at the boundary that owns it: the API's
  contract, the generator, or a checked API such as `TableBuilder`. A check on
  a hot path needs a measured cost near noise, or a case of silent corruption
  in ordinary use, to justify it.

## The format and its implementations

- tafla is one format with several implementations. `SPEC.md` describes it,
  and the conformance suite in `conformance/` checks it: every implementation
  reads its golden tables, compares their values with the descriptions in
  `cases.json`, and rebuilds them byte for byte, and it rejects the corrupted
  tables with its decode error.
- TypeScript is the reference implementation, and writes the suite. To change
  or add a case, edit the cases in TypeScript, regenerate the suite, and run
  every implementation's tests.
- A change to the bytes of an existing case is a change to the format. Make it
  in one PR that changes `SPEC.md`, the suite, and every implementation,
  so that no commit has implementations that disagree.
- Each package has a version and a changelog of its own, which `RELEASING.md`
  lists. Describe a change in the Unreleased section of each package that it
  changes, and a change to the format in every implementation's. Versions
  change only in a release, with `scripts/version.sh`.
- The code that tafla-gen generates from the example schemas in `schemas/`
  lies in each language's test tree, whose checks and tests run it, and is
  the generator's golden output. So is the code that it generates from each
  comparison suite's `workloads.tafla`, in the suite. Regenerate it rather
  than edit it, as `rust/gen/README.md` describes; a check or test that it
  fails is a bug of the generator.
- Keep the domain names the same in every language: table, field, layout,
  offset type, offset size, vector, builder, and decode error. Make the syntax
  idiomatic for each language.
- The runtimes depend only on their language's standard library, except for
  JSpecify's annotations in Java, and PyO3, which binds the Python runtime's
  native classes, written in Rust. PyO3 and the Rust runtime are built into
  the native module, so the installed `tafla` still depends on nothing but
  Python. The native classes take about half the time of the pure-Python
  ones, which remain for every other platform. Adding a dependency to a
  runtime needs strong justification.

## Untrusted input

- Readers decode bytes from untrusted sources. Every read stays within the bytes
  it was given, and malformed input fails with the implementation's decode error
  (`DecodeError`, or `DecodeException` in Java). It never fails with another
  error, a crash, a panic, an out-of-bounds read, or an allocation out of
  proportion to the input.
- Builders reject what they can't write, such as a table too long for its
  offset type, with a documented error, rather than writing a table that
  readers misread. Values converted to a field's type, such as numbers written
  as float32, convert as documented.

## CPU, memory, and measurement

- Design for CPU and memory efficiency from the outset. Avoid unnecessary copies,
  allocations, loops, conversions, and repeated computation.
- Choose representations suited to the work. Use typed arrays, ranges into
  existing data, iterators, or streams when appropriate. Accept `ArrayLike`
  inputs where the algorithm only needs indexed access and length.
- Keep ownership and lifetime clear, especially for buffers, views of shared
  bytes, and WASM resources. Make eager versus lazy execution evident in the
  API.
- Balance performance against complexity and clarity. When the tradeoff is
  uncertain, run the code, measure representative workloads, and evaluate the
  result. Do not rely solely on intuition about what should be faster.
- For performance changes, compare actual behavior before and after, including
  relevant CPU or memory costs. Preserve numerical correctness and public
  contracts. Report the workload, results, and limitations of the measurement;
  distinguish measured results from expectations.

## Benchmarks

- Benchmarks time tafla's own operations and compare them with popular
  libraries that do similar work: zero-copy tables such as FlatBuffers,
  schema-based codecs such as protobuf, and schemaless binary formats such as
  MessagePack.
- Every language runs the same workloads with the same values, so that its
  numbers compare with the other languages': a record, documents of 100 and
  100,000 rows, a vector of 100 nested tables, an array of 100 structs, and a
  fixed-size record.
- Before timing, every codec must read back exactly what it wrote.
- Run variants interleaved, in alternating order, over many rounds after a
  warm-up, and report the fastest. Give each variant its own timing loop, so
  that a JIT doesn't share type feedback between them, and keep every result
  alive, so that it can't eliminate the work. Check the machine's load before
  running, and include an A/A pair when comparing two versions.

## Refactoring and language evolution

- Before undertaking a broader refactor discovered during a task, propose it to
  the user. Show concrete evidence of the problem, the proposed design, affected
  scope, benefits, and costs. Wait for a decision before implementing that broader
  change. Routine local structuring within the agreed task can proceed.
- Consider current language and platform capabilities. A built-in may replace
  custom code or a dependency when supported by the project's target runtimes and
  compatible with the required behavior. Preserve the intent of the design while
  evaluating better ways to express it.

## Tests

- Test the format and the public API, not the implementation. Assert what a
  caller can observe: the values read back, the bytes written, and the errors
  raised. Do not assert private state, internal buffer sizes, call counts, or
  which code path ran.
- Prefer property tests, with fast-check, Hypothesis, jqwik, or proptest. The
  core properties hold in every language:
  - A table built from any description reads back the same values.
  - Tables follow the format's rules: the length in the header, offsets within
    the table, typed arrays aligned to their element size from the table's
    start, and little-endian values.
  - Arbitrary and corrupted bytes read as values or fail with the decode error.
  - Builders reject what they can't write.
- Generate tables from the conformance suite's descriptions, which every
  language's conformance test already builds and reads, rather than from a
  second model of tables.
- Use generic shapes in examples, tests, and benchmarks, rather than the
  formats of any one application.
- Add example tests where they document behavior, such as the examples in
  READMEs and package documentation, and regression cases for known failures.
  Cover numerical, binary-format, and boundary cases: signed zero and NaN,
  integer limits, empty arrays, the limits of each offset type, strings outside
  the Basic Multilingual Plane, unaligned input, and big-endian hosts.
- Compare floats by their bits or with `Object.is`. Some deep-equality matchers
  treat -0 as 0, so check that an assertion about signed zero fails against
  code that loses the sign.
- Use meaningful test names and keep grouping shallow. Add `describe` blocks
  when they organize related behavior, rather than repeating the file's identity.
- Run the relevant code and checks. Select tests and benchmarks appropriate to
  the affected language and behavior, and state anything that could not be run.

## Formatting

- Follow each language's formatter and compiler configurations, which live in
  its directory. In TypeScript, Biome sets four-space indentation, single
  quotes, semicolons, no trailing commas, and contextual line widths.
  Intentional whitespace and readable structure still require judgment after
  automatic formatting.

## Repository commands

- `typescript/`, `python/`, `java/`, and `rust/` are each the root of their
  language's toolchain. Run a language's checks from its directory; its README
  lists them, and `.github/workflows/<language>.yml` runs the same checks in CI.
- A change to `conformance/` runs every language's tests.
