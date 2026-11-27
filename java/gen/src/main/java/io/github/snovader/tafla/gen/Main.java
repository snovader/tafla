package io.github.snovader.tafla.gen;

import java.io.PrintStream;
import java.nio.charset.StandardCharsets;

/** The {@code tafla-gen} command, which the jar runs. */
public final class Main {
    private Main() {}

    /**
     * Runs the command, and returns when every schema generated its files, or the table was
     * decoded or encoded. Otherwise it exits the JVM, with code 1 when a schema has errors, a
     * file can't be read or written, or a table or its JSON can't be converted, and 2 when the
     * arguments are wrong, so a build that runs it in a JVM of its own fails, and one that calls
     * it in its own JVM continues only when it succeeded. {@code --help} describes the
     * arguments.
     *
     * @param arguments the command's arguments, without its own name
     */
    public static void main(String[] arguments) {
        CommandOutput output = Command.run(arguments);

        print(System.out, output.stdout());
        print(System.err, output.stderr().getBytes(StandardCharsets.UTF_8));
        if (output.code() != 0) {
            System.exit(output.code());
        }
    }

    /**
     * Prints bytes as they are: text in UTF-8, as the generator's native command prints it,
     * whatever the platform's charset, and a table's bytes
     */
    private static void print(PrintStream stream, byte[] bytes) {
        stream.write(bytes, 0, bytes.length);
        stream.flush();
    }
}
