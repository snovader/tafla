# tafla for Java

`core/` holds `io.github.snovader.tafla:tafla`, the runtime, whose module
and package are `io.github.snovader.tafla`. Its package documentation has an
example.

`benchmarks/` compares tafla with FlatBuffers, protobuf, MessagePack and SBE
in JMH benchmarks, on the same workloads as the TypeScript comparison. Its
[README](benchmarks/README.md) describes them and has results.

`gen/` holds `io.github.snovader.tafla:tafla-gen`, the jar of tafla-gen, the
schema compiler in `rust/gen`, whose WebAssembly module it runs. Its
[README](gen/README.md) describes it.

## Development

This is a Gradle build. It compiles for Java 21 with a JDK 25, which Gradle
downloads when none is installed, and checks the code with Error Prone and
NullAway. Run the commands below from this directory.

```sh
./gradlew build
./gradlew javadoc
```

`./gradlew test -PtestJavaVersion=21` runs the tests in Java 21 rather than 25.
`./gradlew :core:publishToMavenLocal` publishes the runtime's jar, sources,
javadoc and POM to the local Maven repository, signed with an ASCII-armored key
in the `signingInMemoryKey` and `signingInMemoryKeyPassword` Gradle
properties, without which it fails. The release workflow publishes them to
Maven Central, as [RELEASING.md](../RELEASING.md) describes.
`./gradlew build` also checks that every codec in the benchmarks reads back
what it wrote, and `./gradlew :benchmarks:jmh` runs the benchmarks.

`./gradlew build` builds `gen/` too, which builds the generator's module with
cargo, so it needs Rust with the `wasm32-unknown-unknown` target.
`./gradlew :core:build` and the benchmarks' tasks don't.

The test module holds the Java that tafla-gen generates from the example
schemas in `schemas/`, and from `typescript/core/test/presence.tafla`, of
optional fields and versions, in a package of each schema,
`io.github.snovader.tafla.test.generated.<schema>`, which its tests run.
They're the generator's golden files, which `rust/gen/README.md` describes:
rewrite them with the generator rather than by hand.
