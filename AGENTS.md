# Code and design guidelines

These repository guidelines apply when designing, implementing, reviewing, and
refactoring code. Use judgment about tradeoffs; existing code is evidence of
intent, not a requirement to preserve every detail.

## Responsibilities and architecture

- Give each function, class, and module a clear scope of responsibility. Keep
  layout, writing, reading, and code generation with their respective owners.
  For example, `TableLayout` describes a table's header and offset type,
  `TableBuilder` writes tables, and `TableReader` reads them. In the generator,
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

## TypeScript idioms

The shared lint configuration allows each of these, so that the code says
what it means rather than what a rule asks for. The linter doesn't choose
between them: the author does.

- Check a nullable string, number or boolean by its truthiness, as
  `if (value)`, rather than by `!== undefined` or `=== true`. Do so only
  where '' and 0 are to be treated as missing too, or can't occur, as the
  error of a file that tafla-gen's command couldn't write, which always
  names the file. Otherwise compare: a table's version may be 0, and an
  option of `TableLayout.create()` may be named ''.
- Check for a missing value with `== null` or `!= null`, which match both
  null and undefined. Compare strictly only where the two mean different
  things, or where only one can occur and the code is measured, as below.
- Write a default for a nullable value as `a ?? b`. Use `a || b` only where
  '' or 0 is meant to be replaced too, and then say so.
- Sort or reverse a copy that the function owns in place, with `.sort()` or
  `.reverse()`. `toSorted()` and `toReversed()` are for an array that's
  someone else's, or that is also read in its own order, as the test that
  compares a vector's ends with themselves sorted.
- Give a regular expression a `u` flag only where the pattern needs one: to
  match by code point beyond the Basic Multilingual Plane, for `\p{…}`, or
  for `\u{…}`, as the check of the bundle for characters outside Latin-1.
- Index an array where the index is known to be in range, as
  `fields[fields.length - 2]`. Use `.at()` where it may not be, and handle
  the undefined.
- In tests, give `vi.fn()` type arguments only when it has no
  implementation to infer them from, and write asymmetric matchers, such as
  `expect.objectContaining()`, in place, rather than through a variable
  typed `unknown`.
- Fix what the linter reports rather than suppressing it. Where the rewrite
  that a rule asks for reads worse, keep the code, with a disable directive
  that gives the reason. A directive that no longer suppresses anything is
  itself an error.
- These are choices of style, so they follow the rule for measured code:
  change a comparison on a reader's or a builder's hot path, such as
  `=== undefined` in `TableBuilder`, only with a benchmark that shows no
  change. The bytecode that V8 inlines is counted in bytes there.

## Encapsulation and API design

- Prefer private fields and method-style accessors, especially when access
  performs computation, validation, caching, or other logic. Use `readonly` where
  the field should not be reassigned. This preference can evolve when language
  features offer a compelling improvement.
- In the TypeScript runtime, declare a class's fields as
  `declare private readonly _name`, or `private _name = value` where a field
  starts with a constant, and assign them in the constructor. V8 defines a
  `#name` field, and any field declared without `declare`, as `undefined`
  before the constructor assigns it, and then holds it as mutable and of any
  type, which made reads up to 17% slower. Private methods stay `#name()`.
  tafla-gen generates the same fields. Code that isn't measured, such as
  `@tafla/gen` and the tests, keeps `#name` fields.
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
  offset type, offset size, vector, builder, reader, and decode error. Make
  the syntax idiomatic for each language.
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
  (`DecodeError`, or `DecodeException` in Java and C#). It never fails with
  another error, a crash, a panic, an out-of-bounds read, or an allocation out
  of proportion to the input.
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
  its directory. In TypeScript, oxfmt sets four-space indentation, single
  quotes, semicolons, no trailing commas, and contextual line widths.
  Intentional whitespace and readable structure still require judgment after
  automatic formatting.
- TypeScript's configurations extend the shared ones of `@snovader/tsconfig`
  and `@snovader/oxc-config`, and keep only what is tafla's own: the paths
  that the tools ignore, the name `tafla` that oxfmt sorts after other
  packages, the `tafla-source` condition, and in `oxlint.config.ts`, the two
  rules that tafla turns off, `unicorn/prefer-spread`, for `.slice()` of
  typed arrays, and `unicorn/escape-case`, and the rules that the
  FlatBuffers codec written in flatc's shape breaks. Each has its comment.
  The shared base already allows the idioms above and turns off what tafla
  used to turn off itself, such as `new Array(length)`, code units, and
  awaits in loops, so don't repeat its rules here. Change a shared setting
  in those packages, rather than overriding it here.

## Repository commands

- `typescript/`, `python/`, `java/`, and `rust/` are each the root of their
  language's toolchain. Run a language's checks from its directory; its README
  lists them, and `.github/workflows/<language>.yml` runs the same checks in CI.
- A change to `conformance/` runs every language's tests.
- Yarn installs a version of a package only a day after it's published.
  `typescript/.yarnrc.yml` exempts `@snovader/*` and `hypotes`, the owner's
  own packages, with `npmPreapprovedPackages`, so that a new shared
  configuration or test runner installs on the day it's published.
