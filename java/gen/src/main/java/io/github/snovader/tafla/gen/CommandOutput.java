package io.github.snovader.tafla.gen;

/** What the command prints, and its exit code */
record CommandOutput(int code, String stdout, String stderr) {
    /** The output of a command that's over, as the generator's module responds with it */
    static CommandOutput read(Response response) {
        int code = response.count();
        String stdout = response.string();
        String stderr = response.string();
        return new CommandOutput(code, stdout, stderr);
    }
}
