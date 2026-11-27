# tafla-gen for the JVM

tafla-gen, tafla's schema compiler, as a jar:
`io.github.snovader.tafla:tafla-gen` on Maven Central. It compiles a schema
of messages, tables and structs into code that writes and reads them with
tafla's runtime for TypeScript, JavaScript, Python, Java, Rust, Go, C++,
Swift or C#. The jar holds the generator's WebAssembly module, compiled to
classes of the JVM, so it needs no Rust toolchain and is the same on every
platform. It runs on Java 21 or later and depends on nothing.

[The generator's README](https://github.com/snovader/tafla/tree/main/rust/gen#readme)
describes the schema language, the generated code and the command.

## Command

```sh
java -jar tafla-gen-0.1.0.jar java shapes.tafla --package com.example.shapes --out src/main/java
java -jar tafla-gen-0.1.0.jar decode shapes.tafla Document document.bin > document.json
java -jar tafla-gen-0.1.0.jar encode shapes.tafla Document document.json > document.bin
```

`decode` prints a table's values as JSON, and `encode` prints the table's
bytes from that JSON, each reading the table from stdin without a file, as
the generator's README describes in
[Tables as JSON](https://github.com/snovader/tafla/tree/main/rust/gen#tables-as-json).

It takes the arguments, prints the errors and exits with the codes of the
`tafla-gen` that `cargo install tafla-gen` installs, since the generator's
module decides them for both. `--help` describes them. The reason that a
file can't be read or written is Java's rather than Rust's. A run takes about
a sixth of a second, most of which is the JVM loading the module's classes.

The generated Java imports tafla's runtime from
[`io.github.snovader.tafla:tafla`](https://central.sonatype.com/artifact/io.github.snovader.tafla/tafla),
which the project that compiles it depends on.

## Gradle

A configuration resolves the jar, and a `JavaExec` task runs it before the
project's Java compiles:

```kotlin
val taflaGen = configurations.dependencyScope("taflaGen")
val taflaGenClasspath = configurations.resolvable("taflaGenClasspath") {
    extendsFrom(taflaGen.get())
}

dependencies {
    "taflaGen"("io.github.snovader.tafla:tafla-gen:0.1.0")
    implementation("io.github.snovader.tafla:tafla:0.1.0")
}

val generateTables = tasks.register<JavaExec>("generateTables") {
    val schemas = fileTree("src/main/tafla") { include("*.tafla") }
    val generated = layout.buildDirectory.dir("generated/sources/tafla")

    classpath = taflaGenClasspath.get()
    mainClass = "io.github.snovader.tafla.gen.Main"
    args("java", "--package", "com.example.shapes", "--out", generated.get().asFile.path)
    args(schemas.files.map { it.path })

    inputs.files(schemas)
    outputs.dir(generated)
}

sourceSets.main {
    java.srcDir(generateTables)
}
```

## Maven

`exec-maven-plugin` runs the jar's class from the plugin's own dependencies:

```xml
<plugin>
  <groupId>org.codehaus.mojo</groupId>
  <artifactId>exec-maven-plugin</artifactId>
  <executions>
    <execution>
      <phase>generate-sources</phase>
      <goals><goal>java</goal></goals>
    </execution>
  </executions>
  <configuration>
    <includePluginDependencies>true</includePluginDependencies>
    <includeProjectDependencies>false</includeProjectDependencies>
    <mainClass>io.github.snovader.tafla.gen.Main</mainClass>
    <arguments>
      <argument>java</argument>
      <argument>src/main/tafla/shapes.tafla</argument>
      <argument>--package</argument>
      <argument>com.example.shapes</argument>
      <argument>--out</argument>
      <argument>${project.build.directory}/generated-sources/tafla</argument>
    </arguments>
  </configuration>
  <dependencies>
    <dependency>
      <groupId>io.github.snovader.tafla</groupId>
      <artifactId>tafla-gen</artifactId>
      <version>0.1.0</version>
    </dependency>
  </dependencies>
</plugin>
```

The `java` goal calls the command in Maven's own JVM. The command returns
when it succeeds, and Maven goes on; when it fails, it exits the JVM with its
code, which ends Maven's build there, after the command's errors.

## What the jar holds

- The generator's WebAssembly module, which
  [Chicory](https://github.com/dylibso/chicory)'s build-time compiler
  compiled to classes when the jar was built, rather than the module for an
  interpreter: started and run so, the command takes a third of the time.
- Chicory's runtime, under its Apache License 2.0, in `META-INF/LICENSE-chicory`.
  Its classes keep their packages, `com.dylibso.chicory`, so a classpath that
  holds the jar and another version of Chicory holds both: give the jar a
  classpath of its own, as above.
- `io.github.snovader.tafla.gen.Main`, which reads the schemas, writes the
  files and prints. It's the jar's only API.

## Development

`./gradlew :gen:build` in `java/` builds the generator's module with cargo,
so it needs Rust with the `wasm32-unknown-unknown` target, compiles the
module to classes, packages the jar, and runs its command with the cases of
the native command's tests in `rust/gen/tests/cli.rs`.
