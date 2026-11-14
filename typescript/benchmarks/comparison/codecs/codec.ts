import type {
    DocumentInput,
    DocumentOutput,
    ListInput,
    RecordInput,
    SampleInput
} from '../workloads.ts';

export interface Workload<I, O> {
    serialize(input: I): Uint8Array;
    /** Reads every field, with columns as typed arrays */
    deserialize(bytes: Uint8Array): O;
}

export interface DocumentWorkload
    extends Workload<DocumentInput, DocumentOutput> {
    /** Sums column `a` through the codec's own per-element accessor */
    sumA(bytes: Uint8Array): number;
    /**
     * Writes into bytes kept between calls, where the library can: the
     * start of `bytes`, which are long enough, or a buffer of its own that
     * it reuses, which leaves `bytes` alone. Returns the bytes that hold it.
     */
    serializeInto?(input: DocumentInput, bytes: Uint8Array): Uint8Array;
}

/** One library's encoding of the workloads */
export interface Codec {
    name: string;
    record: Workload<RecordInput, RecordInput>;
    document: DocumentWorkload;
    list: Workload<ListInput, ListInput>;
    /** The list's items as an array of structs, in the library's encoding */
    structs: Workload<ListInput, ListInput>;
    sample: Workload<SampleInput, SampleInput>;
}
