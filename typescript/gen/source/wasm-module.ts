import type { GeneratedFile } from './index.ts';

/**
 * The exports of tafla-gen's WebAssembly module, which the "WebAssembly"
 * section of the generator's README describes: requests and responses of
 * little-endian `u32` counts and of strings, each its byte length as a `u32`
 * and then its UTF-8 bytes.
 */
interface Exports {
    readonly memory: WebAssembly.Memory;
    readonly tafla_alloc: (length: number) => number;
    readonly tafla_free: (pointer: number, length: number) => void;
    readonly tafla_compile: Operation;
    readonly tafla_command: Operation;
    readonly tafla_generate: Operation;
}

/** Takes a request and its length, frees it, and returns a response */
type Operation = (request: number, length: number) => number;

/** The exports that take a request */
export type OperationName =
    | 'tafla_command'
    | 'tafla_compile'
    | 'tafla_generate';

const SIZEOF_U32 = 4;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** A request's counts and strings, in the order they were added */
export class Request {
    readonly #parts: Array<number | Uint8Array> = [];
    #length = 0;

    public count(count: number): this {
        this.#parts.push(count);
        this.#length += SIZEOF_U32;
        return this;
    }

    public string(string: string): this {
        return this.bytes(encoder.encode(string));
    }

    /** A string that needn't be UTF-8, such as a schema as it was read */
    public bytes(bytes: Uint8Array): this {
        this.count(bytes.length);
        this.#parts.push(bytes);
        this.#length += bytes.length;
        return this;
    }

    public length(): number {
        return this.#length;
    }

    /** Writes the request into `memory` at `address` */
    public writeTo(memory: ArrayBuffer, address: number): void {
        const view = new DataView(memory);
        const target = new Uint8Array(memory);
        let offset = address;

        for (const part of this.#parts) {
            if (typeof part === 'number') {
                view.setUint32(offset, part, true);
                offset += SIZEOF_U32;
            } else {
                target.set(part, offset);
                offset += part.length;
            }
        }
    }
}

/**
 * A response in the module's memory, read in order from after its status.
 * It's only valid during the call that it was passed to, which frees it.
 */
export class Response {
    readonly #memory: ArrayBuffer;
    readonly #view: DataView;
    #offset: number;

    public constructor(memory: ArrayBuffer, address: number) {
        this.#memory = memory;
        this.#view = new DataView(memory);
        this.#offset = address;
    }

    public count(): number {
        const count = this.#view.getUint32(this.#offset, true);
        this.#offset += SIZEOF_U32;
        return count;
    }

    public string(): string {
        const length = this.count();
        const bytes = new Uint8Array(this.#memory, this.#offset, length);
        this.#offset += length;
        return decoder.decode(bytes);
    }

    /** A count and that many strings */
    public strings(): string[] {
        return Array.from({ length: this.count() }, () => this.string());
    }

    /** A count of files and each file's path and contents */
    public files(): GeneratedFile[] {
        return Array.from({ length: this.count() }, () => ({
            path: this.string(),
            contents: this.string()
        }));
    }
}

/** An instance of tafla-gen's WebAssembly module */
export class GeneratorModule {
    readonly #exports: Exports;

    /** Instantiates the module from its bytes. It imports nothing. */
    public constructor(wasm: BufferSource) {
        const instance = new WebAssembly.Instance(
            new WebAssembly.Module(wasm),
            {}
        );
        this.#exports = instance.exports as unknown as Exports;
    }

    /**
     * Passes a request to one of the module's operations, and returns what
     * `read` reads from its response, which is freed afterwards
     */
    public call<Result>(
        operation: OperationName,
        request: Request,
        read: (status: number, response: Response) => Result
    ): Result {
        const { memory } = this.#exports;
        const length = request.length();

        const requestAddress = this.#exports.tafla_alloc(length);
        // Allocating may grow the memory, which detaches earlier views of
        // it, so each view is made after the call before it
        request.writeTo(memory.buffer, requestAddress);

        const address = this.#exports[operation](requestAddress, length);
        const response = new Response(memory.buffer, address);
        const responseLength = response.count();
        try {
            return read(response.count(), response);
        } finally {
            this.#exports.tafla_free(address, responseLength);
        }
    }
}
