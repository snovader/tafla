package io.github.snovader.tafla.gen;

import com.dylibso.chicory.runtime.ExportFunction;
import com.dylibso.chicory.runtime.Instance;
import com.dylibso.chicory.runtime.Memory;

/**
 * An instance of tafla-gen's WebAssembly module, which imports nothing. Its functions are the
 * classes that Chicory compiled them to when the jar was built, rather than interpreted.
 */
final class GeneratorModule {
    /** The exports that take a request, free it, and return a response */
    enum Operation {
        COMMAND("tafla_command"),
        GENERATE("tafla_generate");

        private final String export;

        Operation(String export) {
            this.export = export;
        }
    }

    private final Instance instance;
    private final Memory memory;
    private final ExportFunction alloc;
    private final ExportFunction free;

    GeneratorModule() {
        instance =
                Instance.builder(CompiledGenerator.load())
                        .withMachineFactory(CompiledGenerator::create)
                        .build();
        memory = instance.memory();
        alloc = instance.export("tafla_alloc");
        free = instance.export("tafla_free");
    }

    /** Passes a request to one of the module's operations, and returns a copy of its response */
    Response call(Operation operation, Request request) {
        byte[] requestBytes = request.toByteArray();
        int requestAddress = (int) alloc.apply(requestBytes.length)[0];
        memory.write(requestAddress, requestBytes);

        ExportFunction function = instance.export(operation.export);
        int address = (int) function.apply(requestAddress, requestBytes.length)[0];

        int length = memory.readInt(address);
        byte[] response = memory.readBytes(address, length);
        free.apply(address, length);

        return new Response(response);
    }
}
