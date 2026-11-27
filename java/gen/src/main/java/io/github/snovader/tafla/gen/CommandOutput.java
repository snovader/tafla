package io.github.snovader.tafla.gen;

/**
 * What the command prints, and its exit code. Its stdout is bytes, since {@code encode} prints a
 * table's.
 */
final class CommandOutput {
    private final int code;
    private final byte[] stdout;
    private final String stderr;

    CommandOutput(int code, byte[] stdout, String stderr) {
        this.code = code;
        this.stdout = stdout;
        this.stderr = stderr;
    }

    /** The output of a command that's over, as the generator's module responds with it */
    static CommandOutput read(Response response) {
        int code = response.count();
        byte[] stdout = response.bytes();
        String stderr = response.string();
        return new CommandOutput(code, stdout, stderr);
    }

    int code() {
        return code;
    }

    /** The bytes to print to stdout, which the caller doesn't change */
    byte[] stdout() {
        return stdout;
    }

    String stderr() {
        return stderr;
    }
}
