package io.github.snovader.tafla.gen;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.stream.Stream;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.Arguments;
import org.junit.jupiter.params.provider.MethodSource;
import org.junit.jupiter.params.provider.ValueSource;

/**
 * The command of the jar that the build packages, run as {@code java -jar}, with the cases of
 * the native command's tests in rust/gen/tests/cli.rs that a host could get wrong: the files it
 * reads and writes, what it prints, and its exit codes.
 */
class CommandTest {
    private static final Path JAR = Path.of(System.getProperty("tafla.gen.jar"));
    private static final Path REPOSITORY = Path.of(System.getProperty("tafla.repository"));
    private static final String VERSION = System.getProperty("tafla.gen.version");

    private static final Path DIAGNOSTICS = REPOSITORY.resolve("rust/gen/tests/diagnostics");
    private static final String JAVA_GOLDEN = "java/core/src/test/java";
    private static final String JAVA_PACKAGE = "io.github.snovader.tafla.test.generated.shapes";

    private static final String USAGE =
            "usage: tafla-gen <typescript|javascript|python|java|rust|go|cpp|swift|csharp>"
                    + " <schema>... [--out <dir>] [--runtime <module>] [--package <package>]";
    private static final String POINT = "message Point { x: uint8; }\n";

    private record Output(int status, String stdout, String stderr) {}

    @TempDir Path directory;
    @TempDir Path out;

    private static Output taflaGen(Path directory, String... arguments) {
        List<String> command = new ArrayList<>();
        command.add(Path.of(System.getProperty("java.home"), "bin", "java").toString());
        command.add("-jar");
        command.add(JAR.toString());
        command.addAll(List.of(arguments));

        try {
            Process process = new ProcessBuilder(command).directory(directory.toFile()).start();
            process.getOutputStream().close();

            // The command prints less than a pipe holds, so reading one stream after the other
            // can't block it
            String stdout = new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
            String stderr = new String(process.getErrorStream().readAllBytes(), StandardCharsets.UTF_8);
            return new Output(process.waitFor(), stdout, stderr);
        } catch (IOException error) {
            throw new UncheckedIOException(error);
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new IllegalStateException(error);
        }
    }

    private void writeSchema(String path, byte[] source) throws IOException {
        Path schema = directory.resolve(path);
        Files.createDirectories(schema.getParent());
        Files.write(schema, source);
    }

    private void writeSchema(String path, String source) throws IOException {
        writeSchema(path, source.getBytes(StandardCharsets.UTF_8));
    }

    /** Every file under a directory, by its path with {@code /} between its names */
    private static Map<String, String> readTree(Path root) throws IOException {
        Map<String, String> files = new TreeMap<>();
        if (!Files.exists(root)) {
            return files;
        }

        try (Stream<Path> paths = Files.walk(root)) {
            for (Path path : paths.filter(Files::isRegularFile).toList()) {
                String relative = root.relativize(path).toString().replace('\\', '/');
                files.put(relative, Files.readString(path));
            }
        }
        return files;
    }

    private static List<String> pathsOf(Path root) throws IOException {
        return List.copyOf(readTree(root).keySet());
    }

    @Test
    void writesTheGoldenFilesOfAnExampleSchemaForJava() throws IOException {
        Output output =
                taflaGen(
                        REPOSITORY,
                        "java", "schemas/shapes.tafla",
                        "--package", JAVA_PACKAGE,
                        "--out", out.toString());

        assertEquals(new Output(0, "", ""), output);
        String packageDirectory = JAVA_PACKAGE.replace('.', '/');
        assertEquals(
                readTree(REPOSITORY.resolve(JAVA_GOLDEN).resolve(packageDirectory)),
                readTree(out.resolve(packageDirectory)));
        assertEquals(
                List.of(),
                pathsOf(out).stream().filter(path -> !path.startsWith(packageDirectory)).toList());
    }

    @Test
    void writesTheGoldenFilesOfAnExampleSchemaForRust() throws IOException {
        Output output =
                taflaGen(REPOSITORY, "rust", "schemas/shapes.tafla", "--out", out.toString());

        assertEquals(new Output(0, "", ""), output);
        assertEquals(
                readTree(REPOSITORY.resolve("rust/gen/tests/golden/shapes/rust")), readTree(out));
    }

    @Test
    void writesEverySchemaItIsGivenUnderTheCurrentDirectoryByDefault() throws IOException {
        writeSchema("point.tafla", POINT);
        writeSchema("nested/pair.tafla", "message Pair { a: uint8; }\n");

        Output output = taflaGen(directory, "py", "point.tafla", "nested/pair.tafla");

        assertEquals(new Output(0, "", ""), output);
        assertEquals(
                List.of("nested/pair.tafla", "pair.py", "point.py", "point.tafla"),
                pathsOf(directory));
    }

    @Test
    void writesTheClassesOfAJavaPackageInItsDirectories() throws IOException {
        writeSchema("point.tafla", POINT);

        Output output =
                taflaGen(
                        directory,
                        "java", "point.tafla",
                        "--package", "com.example.shapes",
                        "--out", out.toString());

        assertEquals(new Output(0, "", ""), output);
        assertEquals(List.of("com/example/shapes/Point.java"), pathsOf(out));
    }

    @Test
    void printsTheErrorsOfAnInvalidSchemaAndFails() throws IOException {
        Output output =
                taflaGen(DIAGNOSTICS, "rust", "unknown_types.tafla", "--out", out.toString());

        String errors = Files.readString(DIAGNOSTICS.resolve("unknown_types.stderr"));
        assertEquals(new Output(1, "", errors), output);
        assertEquals(List.of(), pathsOf(out));
    }

    @Test
    void writesNeitherOfTwoSchemasThatWriteTheSameFile() throws IOException {
        writeSchema("a/point.tafla", POINT);
        writeSchema("b/point.tafla", POINT);
        writeSchema("point.v2.tafla", POINT);
        writeSchema("pair.tafla", "message Pair { a: uint8; }\n");

        Output output =
                taflaGen(
                        directory,
                        "py", "a/point.tafla", "b/point.tafla", "point.v2.tafla", "pair.tafla",
                        "--out", out.toString());

        assertEquals(
                new Output(
                        1,
                        "",
                        """
                        a/point.tafla and b/point.tafla both write point.py

                        a/point.tafla and point.v2.tafla both write point.py
                        """),
                output);
        assertEquals(List.of("pair.py"), pathsOf(out));
    }

    @Test
    void reportsASchemaItCannotReadAndWritesTheOthers() throws IOException {
        Output output =
                taflaGen(
                        REPOSITORY,
                        "rust", "schemas/missing.tafla", "schemas/shapes.tafla",
                        "--out", out.toString());

        assertEquals(1, output.status());
        assertEquals("", output.stdout());
        assertTrue(
                output.stderr().matches("schemas/missing\\.tafla: [^\n/]+\n"), output::stderr);
        assertEquals(List.of("shapes.rs"), pathsOf(out));
    }

    @Test
    void reportsAFileItCannotWriteAfterTheSchemasErrorsAndFails() throws IOException {
        writeSchema("point.tafla", POINT);
        writeSchema("unknown_types.tafla", Files.readString(DIAGNOSTICS.resolve("unknown_types.tafla")));
        // A file where the directory to write under should be
        Files.writeString(directory.resolve("out"), "");

        Output output =
                taflaGen(directory, "py", "point.tafla", "unknown_types.tafla", "--out", "out");

        String errors = Files.readString(DIAGNOSTICS.resolve("unknown_types.stderr"));
        assertEquals(1, output.status());
        assertEquals(errors + "\nout/point.py: file already exists\n", output.stderr());
    }

    @Test
    void rejectsASchemaThatIsntUtf8AtItsFirstSuchBytes() throws IOException {
        // A doc comment in Latin-1 rather than UTF-8
        writeSchema(
                "point.tafla",
                "message Point {\n    /// café au lait\n    x: float64;\n}\n"
                        .getBytes(StandardCharsets.ISO_8859_1));

        Output output = taflaGen(directory, "rust", "point.tafla", "--out", out.toString());

        assertEquals(
                new Output(
                        1,
                        "",
                        """
                        error: the schema isn't UTF-8
                         --> point.tafla:2:12
                          |
                        2 |     /// caf\uFFFD au lait
                          |            ^ this isn't a character in UTF-8
                        """),
                output);
        assertEquals(List.of(), pathsOf(out));
    }

    @Test
    void writesStringsOutsideTheBasicMultilingualPlaneAsUtf8() throws IOException {
        writeSchema("point.tafla", "message Point {\n    /// A 𝄞 clef\n    x: uint8;\n}\n");

        Output output = taflaGen(directory, "rust", "point.tafla", "--out", out.toString());

        assertEquals(new Output(0, "", ""), output);
        assertTrue(Files.readString(out.resolve("point.rs")).contains("/// A 𝄞 clef\n"));
    }

    @Test
    void takesOptionsBeforeTheTargetAndSchemasAfterADoubleDash() throws IOException {
        writeSchema("-point.tafla", POINT);

        Output output = taflaGen(directory, "--out", out.toString(), "py", "--", "-point.tafla");

        assertEquals(new Output(0, "", ""), output);
        assertEquals(List.of("-point.py"), pathsOf(out));
    }

    static Stream<Arguments> helpArguments() {
        return Stream.of(
                Arguments.of((Object) new String[] {"--help"}),
                Arguments.of((Object) new String[] {"-h"}),
                Arguments.of((Object) new String[] {"rust", "schema.tafla", "--help"}));
    }

    @ParameterizedTest
    @MethodSource("helpArguments")
    void printsItsHelpToStdout(String[] arguments) {
        Output output = taflaGen(REPOSITORY, arguments);

        assertEquals(0, output.status());
        assertEquals("", output.stderr());
        assertTrue(output.stdout().startsWith("tafla-gen " + VERSION + "\n"), output::stdout);
        assertTrue(output.stdout().contains("\n" + USAGE + "\n"), output::stdout);
        assertTrue(output.stdout().contains("\n  2  The arguments are wrong\n"), output::stdout);
    }

    @ParameterizedTest
    @ValueSource(strings = {"--version", "-V"})
    void printsTheJarsVersionToStdout(String argument) {
        Output output = taflaGen(REPOSITORY, argument);

        assertEquals(new Output(0, "tafla-gen " + VERSION + "\n", ""), output);
    }

    static Stream<Arguments> wrongArguments() {
        return Stream.of(
                Arguments.of(new String[] {}, "no target is given"),
                Arguments.of(new String[] {"rust"}, "no schema is given"),
                Arguments.of(new String[] {"rust", "point.tafla", "--out"}, "`--out` needs a value"),
                Arguments.of(
                        new String[] {"rust", "point.tafla", "--verbose"},
                        "unknown option `--verbose`"),
                Arguments.of(
                        new String[] {"java", "point.tafla", "--package", "../generated"},
                        "`../generated` isn't a Java package, which is identifiers in ASCII"
                                + " separated by dots, such as `com.example.shapes`"));
    }

    @ParameterizedTest
    @MethodSource("wrongArguments")
    void rejectsWrongArgumentsWithTheUsageAndExitCode2(String[] arguments, String error)
            throws IOException {
        writeSchema("point.tafla", POINT);

        Output output = taflaGen(directory, arguments);

        assertEquals(
                new Output(
                        2,
                        "",
                        "error: " + error + "\n" + USAGE + "\nRun `tafla-gen --help` for more.\n"),
                output);
        assertEquals(List.of("point.tafla"), pathsOf(directory));
    }
}
