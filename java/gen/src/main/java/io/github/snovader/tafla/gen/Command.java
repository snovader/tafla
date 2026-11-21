package io.github.snovader.tafla.gen;

import io.github.snovader.tafla.gen.GeneratorModule.Operation;
import java.io.IOException;
import java.nio.file.AccessDeniedException;
import java.nio.file.FileSystemException;
import java.nio.file.Files;
import java.nio.file.InvalidPathException;
import java.nio.file.NoSuchFileException;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * Runs the {@code tafla-gen} command: reads the schemas, writes the generated files, and
 * returns what to print. The generator's module decides everything else, as it does for the
 * command's native binary, so that the two agree.
 */
final class Command {
    private static final int FAILURE = 1;

    private final GeneratorModule module = new GeneratorModule();
    private final String[] arguments;

    private Command(String[] arguments) {
        this.arguments = arguments;
    }

    /**
     * @param arguments the command's arguments, without its own name
     */
    static CommandOutput run(String[] arguments) {
        return new Command(arguments).run();
    }

    private CommandOutput run() {
        Response parsed = module.call(Operation.COMMAND, argumentsOf(new Request()));
        if (parsed.count() != 0) {
            return CommandOutput.read(parsed);
        }
        String out = parsed.string();
        List<String> schemas = parsed.strings();

        Response generated = module.call(Operation.GENERATE, generation(schemas));
        if (generated.count() != 0) {
            return CommandOutput.read(generated);
        }
        List<String> errors = writeFileSets(out, generated);
        errors.addAll(0, generated.strings());

        return new CommandOutput(errors.isEmpty() ? 0 : FAILURE, "", String.join("\n", errors));
    }

    private Request argumentsOf(Request request) {
        for (String argument : arguments) {
            request.string(argument);
        }
        return request;
    }

    /** The request to generate: the arguments, and each schema's bytes or why it can't be read */
    private Request generation(List<String> schemas) {
        Request request = argumentsOf(new Request().count(arguments.length));

        for (String schema : schemas) {
            try {
                byte[] source = Files.readAllBytes(Path.of(schema));
                request.count(0).bytes(source);
            } catch (IOException | InvalidPathException error) {
                request.count(1).string(describe(error));
            }
        }
        return request;
    }

    /**
     * Writes the files of each schema in the response under {@code out}, a schema's up to the
     * first that fails.
     *
     * @return the errors of the files that failed
     */
    private static List<String> writeFileSets(String out, Response generated) {
        List<String> errors = new ArrayList<>();

        int fileSets = generated.count();
        for (int fileSet = 0; fileSet < fileSets; fileSet++) {
            String error = null;

            int files = generated.count();
            for (int file = 0; file < files; file++) {
                String path = generated.string();
                byte[] contents = generated.bytes();
                if (error == null) {
                    error = write(out, path, contents);
                }
            }

            if (error != null) {
                errors.add(error);
            }
        }
        return errors;
    }

    /**
     * Writes a generated file at its path under {@code out}, and makes the path's directories.
     *
     * @return the file's error, if it can't be written
     */
    private static String write(String out, String path, byte[] contents) {
        String target = out + "/" + path;
        try {
            Path file = Path.of(out, path);
            target = file.toString();

            Path directory = file.getParent();
            if (directory != null) {
                Files.createDirectories(directory);
            }
            Files.write(file, contents);
            return null;
        } catch (IOException | InvalidPathException error) {
            return target + ": " + describe(error) + "\n";
        }
    }

    /**
     * Why a file can't be read or written, in Java's words rather than Rust's, and without the
     * file's path, which a file system's exception has for its message
     */
    private static String describe(Exception error) {
        if (error instanceof NoSuchFileException) {
            return "no such file or directory";
        }
        if (error instanceof AccessDeniedException) {
            return "access denied";
        }

        String reason =
                switch (error) {
                    case FileSystemException fileSystem -> fileSystem.getReason();
                    case InvalidPathException invalidPath -> invalidPath.getReason();
                    default -> error.getMessage();
                };
        return reason != null ? reason : error.getClass().getSimpleName();
    }
}
