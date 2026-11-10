import {
    ByteBuffer,
    readFloat32,
    readFloat64,
    readInt8,
    readInt16,
    readInt32,
    readInt64,
    readUint8,
    readUint16,
    readUint32,
    readUint64,
    StructWriter,
    TableBuilder,
    TableLayout,
    TableSizer,
    TableWriter
} from 'tafla';
import {
    type Alignment,
    elementTypeOf,
    type FieldDescription,
    fromJsonNumber,
    isArrayField,
    type JsonNumber,
    type LayoutDescription,
    type NestedTable,
    type ScalarType,
    type StructArrayField,
    type TableDescription,
    type TableField,
    toJsonNumber
} from './description.ts';

/** The byte size of each scalar type, which its arrays are aligned to */
export const ELEMENT_SIZES: Readonly<Record<ScalarType, Alignment>> = {
    int8: 1,
    uint8: 1,
    int16: 2,
    uint16: 2,
    int32: 4,
    uint32: 4,
    int64: 8,
    uint64: 8,
    float32: 4,
    float64: 8
};

export function layoutOf({
    sized,
    versioned,
    offsetType
}: LayoutDescription): TableLayout {
    return TableLayout.create({ sized, versioned, offsetType });
}

/** Builds a table as its description sets it out */
export function buildTable(table: TableDescription): TableBuilder {
    const builder = createBuilder(table);
    for (const field of table.fields) {
        setField(builder, field);
    }

    return builder;
}

/** Creates the builder of a table, with none of its fields set */
export function createBuilder({
    layout,
    version,
    fields
}: TableDescription): TableBuilder {
    const { sized, offsetType } = layout;

    if (layout.versioned) {
        return new TableBuilder(
            fields.length,
            TableLayout.create({ sized, versioned: true, offsetType }),
            version as number
        );
    }

    return new TableBuilder(
        fields.length,
        TableLayout.create({ sized, offsetType })
    );
}

/** Sets a field as its description sets it out */
export function setField(builder: TableBuilder, field: FieldDescription): void {
    switch (field.type) {
        case 'int8':
            builder.setInt8(fromJsonNumber(field.value));
            break;

        case 'uint8':
            builder.setUint8(fromJsonNumber(field.value));
            break;

        case 'int16':
            builder.setInt16(fromJsonNumber(field.value));
            break;

        case 'uint16':
            builder.setUint16(fromJsonNumber(field.value));
            break;

        case 'int32':
            builder.setInt32(fromJsonNumber(field.value));
            break;

        case 'uint32':
            builder.setUint32(fromJsonNumber(field.value));
            break;

        case 'int64':
            builder.setInt64(BigInt(field.value));
            break;

        case 'uint64':
            builder.setUint64(BigInt(field.value));
            break;

        case 'float32':
            builder.setFloat32(fromJsonNumber(field.value));
            break;

        case 'float64':
            builder.setFloat64(fromJsonNumber(field.value));
            break;

        case 'bool':
            builder.setUint8(field.value ? 1 : 0);
            break;

        case 'int8[]':
            builder.setInt8Array(field.value.map(fromJsonNumber));
            break;

        case 'uint8[]':
            builder.setUint8Array(field.value.map(fromJsonNumber));
            break;

        case 'int16[]':
            builder.setInt16Array(field.value.map(fromJsonNumber));
            break;

        case 'uint16[]':
            builder.setUint16Array(field.value.map(fromJsonNumber));
            break;

        case 'int32[]':
            builder.setInt32Array(field.value.map(fromJsonNumber));
            break;

        case 'uint32[]':
            builder.setUint32Array(field.value.map(fromJsonNumber));
            break;

        case 'int64[]':
            builder.setInt64Array(field.value.map(toBigInt));
            break;

        case 'uint64[]':
            builder.setUint64Array(field.value.map(toBigInt));
            break;

        case 'float32[]':
            builder.setFloat32Array(field.value.map(fromJsonNumber));
            break;

        case 'float64[]':
            builder.setFloat64Array(field.value.map(fromJsonNumber));
            break;

        case 'string':
            builder.setString(field.value);
            break;

        case 'struct':
            builder.setStruct(
                packStructs(
                    field.fields.map(({ type }) => type),
                    [field.fields.map(({ value }) => value)]
                )
            );
            break;

        case 'struct[]':
            builder.setStructArray(
                packStructs(field.fields, field.items),
                structSize(field.fields),
                structAlignment(field.fields)
            );
            break;

        case 'table':
            builder.setTable(buildNested(field), field.alignment);
            break;

        case 'vector':
            builder.setVector(field.items.map(buildNested));
            break;
    }
}

/**
 * Sizes and writes a table with `TableSizer` and `TableWriter`, as its
 * description sets it out, into new bytes or the start of `bytes`. They
 * write tables nested in it in place, so those must have no header and its
 * offset type, as {@link nestedInPlace} makes them.
 */
export function writeTable(
    table: TableDescription,
    bytes?: Uint8Array
): Uint8Array {
    const sizer = createSizer(table);
    for (const field of table.fields) {
        addField(sizer, field);
    }

    return writeFields(sizer.writer(bytes), table);
}

/**
 * Writes a table with a `TableWriter`, as {@link writeTable} does, into
 * `bytes` rather than the sizer's
 */
export function writeTableInto(
    table: TableDescription,
    bytes: Uint8Array
): Uint8Array {
    return writeFields(createWriter(table, bytes), table);
}

/**
 * Writes a table into `bytes`, as {@link writeTableInto} does, through the
 * writer that `wrap` returns for its own, which can change the calls
 */
export function writeTableThrough(
    table: TableDescription,
    bytes: Uint8Array,
    wrap: (writer: TableWriter) => TableWriter
): Uint8Array {
    return writeFields(wrap(createWriter(table, bytes)), table);
}

/**
 * Returns the description with every table nested in it made one that a
 * `TableWriter` writes in place: from its builder, without a header, and
 * with the root's offset type
 */
export function nestedInPlace(table: TableDescription): TableDescription {
    const { offsetType } = table.layout;
    const inPlace = (nested: TableDescription): TableDescription => ({
        layout: { sized: false, versioned: false, offsetType },
        fields: nested.fields.map(field)
    });
    const field = (description: FieldDescription): FieldDescription => {
        switch (description.type) {
            case 'table':
                return {
                    ...description,
                    from: 'builder' as const,
                    table: inPlace(description.table)
                };

            case 'vector':
                return {
                    ...description,
                    items: description.items.map(
                        (item): NestedTable => ({
                            from: 'builder',
                            table: inPlace(item.table)
                        })
                    )
                };

            default:
                return description;
        }
    };

    return { ...table, fields: table.fields.map(field) };
}

function createSizer({
    layout,
    version,
    fields
}: TableDescription): TableSizer {
    const { sized, offsetType } = layout;

    if (layout.versioned) {
        return new TableSizer(
            fields.length,
            TableLayout.create({ sized, versioned: true, offsetType }),
            version as number
        );
    }

    return new TableSizer(
        fields.length,
        TableLayout.create({ sized, offsetType })
    );
}

function createWriter(
    { layout, version, fields }: TableDescription,
    bytes: Uint8Array
): TableWriter {
    const { sized, offsetType } = layout;

    if (layout.versioned) {
        return new TableWriter(
            bytes,
            fields.length,
            TableLayout.create({ sized, versioned: true, offsetType }),
            version as number
        );
    }

    return new TableWriter(
        bytes,
        fields.length,
        TableLayout.create({ sized, offsetType })
    );
}

/** Writes the table's fields, and returns its bytes */
function writeFields(writer: TableWriter, table: TableDescription): Uint8Array {
    for (const [index, field] of table.fields.entries()) {
        writeField(writer, index, field);
    }

    return writer.bytes();
}

/** Adds a field to the sizer as its description sets it out */
function addField(sizer: TableSizer, field: FieldDescription): void {
    switch (field.type) {
        case 'int8':
            sizer.addInt8();
            break;

        case 'uint8':
            sizer.addUint8();
            break;

        case 'int16':
            sizer.addInt16();
            break;

        case 'uint16':
            sizer.addUint16();
            break;

        case 'int32':
            sizer.addInt32();
            break;

        case 'uint32':
            sizer.addUint32();
            break;

        case 'int64':
            sizer.addInt64();
            break;

        case 'uint64':
            sizer.addUint64();
            break;

        case 'float32':
            sizer.addFloat32();
            break;

        case 'float64':
            sizer.addFloat64();
            break;

        case 'bool':
            sizer.addUint8();
            break;

        case 'int8[]':
            sizer.addInt8Array(field.value.length);
            break;

        case 'uint8[]':
            sizer.addUint8Array(field.value.length);
            break;

        case 'int16[]':
            sizer.addInt16Array(field.value.length);
            break;

        case 'uint16[]':
            sizer.addUint16Array(field.value.length);
            break;

        case 'int32[]':
            sizer.addInt32Array(field.value.length);
            break;

        case 'uint32[]':
            sizer.addUint32Array(field.value.length);
            break;

        case 'int64[]':
            sizer.addInt64Array(field.value.length);
            break;

        case 'uint64[]':
            sizer.addUint64Array(field.value.length);
            break;

        case 'float32[]':
            sizer.addFloat32Array(field.value.length);
            break;

        case 'float64[]':
            sizer.addFloat64Array(field.value.length);
            break;

        case 'string':
            sizer.addString(field.value);
            break;

        case 'struct':
            sizer.addStruct(structSize(field.fields.map(({ type }) => type)));
            break;

        case 'struct[]':
            sizer.addStructArray(
                field.items.length,
                structSize(field.fields),
                structAlignment(field.fields)
            );
            break;

        case 'table':
            sizer.addTable(alignmentOf(field));
            addTable(sizer, field.table);
            break;

        case 'vector':
            sizer.addVector(field.items.length);
            for (const item of field.items) {
                addTable(sizer, item.table);
            }
            break;
    }
}

function addTable(sizer: TableSizer, table: TableDescription): void {
    const parent = sizer.startTable(table.fields.length);
    for (const field of table.fields) {
        addField(sizer, field);
    }
    sizer.endTable(parent);
}

/** Writes field `index` as its description sets it out */
function writeField(
    writer: TableWriter,
    index: number,
    field: FieldDescription
): void {
    switch (field.type) {
        case 'int8':
            writer.setInt8(index, fromJsonNumber(field.value));
            break;

        case 'uint8':
            writer.setUint8(index, fromJsonNumber(field.value));
            break;

        case 'int16':
            writer.setInt16(index, fromJsonNumber(field.value));
            break;

        case 'uint16':
            writer.setUint16(index, fromJsonNumber(field.value));
            break;

        case 'int32':
            writer.setInt32(index, fromJsonNumber(field.value));
            break;

        case 'uint32':
            writer.setUint32(index, fromJsonNumber(field.value));
            break;

        case 'int64':
            writer.setInt64(index, BigInt(field.value));
            break;

        case 'uint64':
            writer.setUint64(index, BigInt(field.value));
            break;

        case 'float32':
            writer.setFloat32(index, fromJsonNumber(field.value));
            break;

        case 'float64':
            writer.setFloat64(index, fromJsonNumber(field.value));
            break;

        case 'bool':
            writer.setUint8(index, field.value ? 1 : 0);
            break;

        case 'int8[]':
            writer.setInt8Array(index, field.value.map(fromJsonNumber));
            break;

        case 'uint8[]':
            writer.setUint8Array(index, field.value.map(fromJsonNumber));
            break;

        case 'int16[]':
            writer.setInt16Array(index, field.value.map(fromJsonNumber));
            break;

        case 'uint16[]':
            writer.setUint16Array(index, field.value.map(fromJsonNumber));
            break;

        case 'int32[]':
            writer.setInt32Array(index, field.value.map(fromJsonNumber));
            break;

        case 'uint32[]':
            writer.setUint32Array(index, field.value.map(fromJsonNumber));
            break;

        case 'int64[]':
            writer.setInt64Array(index, field.value.map(toBigInt));
            break;

        case 'uint64[]':
            writer.setUint64Array(index, field.value.map(toBigInt));
            break;

        case 'float32[]':
            writer.setFloat32Array(index, field.value.map(fromJsonNumber));
            break;

        case 'float64[]':
            writer.setFloat64Array(index, field.value.map(fromJsonNumber));
            break;

        case 'string':
            writer.setString(index, field.value);
            break;

        case 'struct':
            writer.setStruct(
                index,
                packStructs(
                    field.fields.map(({ type }) => type),
                    [field.fields.map(({ value }) => value)]
                )
            );
            break;

        case 'struct[]':
            writer.setStructArray(
                index,
                packStructs(field.fields, field.items),
                structSize(field.fields),
                structAlignment(field.fields)
            );
            break;

        case 'table':
            writer.setTable(index, alignmentOf(field));
            writeNested(writer, field.table);
            break;

        case 'vector': {
            const vector = writer.setVector(index, field.items.length);
            for (const [item, { table }] of field.items.entries()) {
                writeNested(writer, table);
                writer.setItemEnd(vector, item);
            }
            break;
        }
    }
}

function writeNested(writer: TableWriter, table: TableDescription): void {
    const parent = writer.startTable(table.fields.length);
    for (const [index, field] of table.fields.entries()) {
        writeField(writer, index, field);
    }
    writer.endTable(parent);
}

/** The byte length of a struct of scalars of these types */
export function structSize(types: readonly ScalarType[]): number {
    return types.reduce((size, type) => size + ELEMENT_SIZES[type], 0);
}

/** The alignment of an array of structs: their largest scalar's size */
export function structAlignment(types: readonly ScalarType[]): Alignment {
    let largest: Alignment = 1;
    for (const type of types) {
        if (ELEMENT_SIZES[type] > largest) {
            largest = ELEMENT_SIZES[type];
        }
    }

    return largest;
}

/** The bytes of structs of scalars of these types, one after another */
function packStructs(
    types: readonly ScalarType[],
    items: readonly JsonNumber[][]
): Uint8Array {
    const writer = new StructWriter(
        new Uint8Array(items.length * structSize(types))
    );
    for (const values of items) {
        for (const [index, type] of types.entries()) {
            setScalar(writer, type, values[index]);
        }
    }

    return writer.bytes();
}

/** Writes a scalar after the writer's previous one */
function setScalar(
    writer: StructWriter,
    type: ScalarType,
    value: JsonNumber
): void {
    switch (type) {
        case 'int8':
            writer.setInt8(fromJsonNumber(value));
            break;

        case 'uint8':
            writer.setUint8(fromJsonNumber(value));
            break;

        case 'int16':
            writer.setInt16(fromJsonNumber(value));
            break;

        case 'uint16':
            writer.setUint16(fromJsonNumber(value));
            break;

        case 'int32':
            writer.setInt32(fromJsonNumber(value));
            break;

        case 'uint32':
            writer.setUint32(fromJsonNumber(value));
            break;

        case 'int64':
            writer.setInt64(BigInt(value));
            break;

        case 'uint64':
            writer.setUint64(BigInt(value));
            break;

        case 'float32':
            writer.setFloat32(fromJsonNumber(value));
            break;

        case 'float64':
            writer.setFloat64(fromJsonNumber(value));
            break;
    }
}

/**
 * Reads the scalars of a struct at `offset` in its bytes with the runtime's
 * read functions, as descriptions hold them
 */
function readStruct(
    bytes: Uint8Array,
    types: readonly ScalarType[],
    offset: number
): JsonNumber[] {
    let position = offset;

    return types.map(type => {
        const value = readScalar(bytes, type, position);
        position += ELEMENT_SIZES[type];
        return value;
    });
}

function readScalar(
    bytes: Uint8Array,
    type: ScalarType,
    offset: number
): JsonNumber {
    switch (type) {
        case 'int8':
            return readInt8(bytes, offset);

        case 'uint8':
            return readUint8(bytes, offset);

        case 'int16':
            return readInt16(bytes, offset);

        case 'uint16':
            return readUint16(bytes, offset);

        case 'int32':
            return readInt32(bytes, offset);

        case 'uint32':
            return readUint32(bytes, offset);

        case 'int64':
            return String(readInt64(bytes, offset));

        case 'uint64':
            return String(readUint64(bytes, offset));

        case 'float32':
            return toJsonNumber(readFloat32(bytes, offset));

        case 'float64':
            return toJsonNumber(readFloat64(bytes, offset));
    }
}

function buildNested({ from, table }: NestedTable): TableBuilder | Uint8Array {
    const builder = buildTable(table);
    return from === 'builder' ? builder : builder.bytes();
}

function toBigInt(value: JsonNumber): bigint {
    return BigInt(value);
}

/**
 * The alignment that `setTable()` pads a nested table to: the one passed, or
 * by default for a builder the largest alignment of its arrays and nested
 * tables, or 1 if it has neither, and for bytes 4.
 */
export function alignmentOf({ from, alignment, table }: TableField): Alignment {
    if (alignment !== undefined) {
        return alignment;
    }

    if (from === 'bytes') {
        return 4;
    }

    let largest: Alignment = 1;
    for (const field of table.fields) {
        const fieldAlignment = alignmentWithin(field);
        if (fieldAlignment > largest) {
            largest = fieldAlignment;
        }
    }

    return largest;
}

/** The alignment that a field needs from the start of its table */
function alignmentWithin(field: FieldDescription): Alignment {
    if (isArrayField(field)) {
        return ELEMENT_SIZES[elementTypeOf(field.type)];
    }

    if (field.type === 'struct[]') {
        return structAlignment(field.fields);
    }

    // Scalars, strings, structs and vectors aren't padded
    return field.type === 'table' ? alignmentOf(field) : 1;
}

/**
 * Thrown by {@link TableReader} when bytes read as values that don't match
 * the shape of a description: a size slot that doesn't hold the table's
 * length, or a vector of another number of items. Unlike a `DecodeError`, the
 * bytes were read.
 */
export class DescriptionMismatch extends Error {
    constructor(message: string) {
        super(message);
        this.name = 'DescriptionMismatch';
    }
}

/**
 * Reads the bytes of a table as descriptions of its fields set them out, and
 * returns those descriptions with the values that were read in place of
 * theirs, so that they equal the descriptions when the bytes hold the values.
 *
 * A nested table, from `setTable()` or in a vector, is read in place when it
 * has no header and its parent's offset type. Any other is read as a table of
 * its own: a vector item from its bytes, and a field from its position to the
 * end of its parent.
 */
export class TableReader {
    readonly #buffer: ByteBuffer;
    readonly #layout: LayoutDescription;

    constructor(bytes: Uint8Array, layout: LayoutDescription) {
        this.#buffer = new ByteBuffer(bytes, layoutOf(layout));
        this.#layout = layout;
    }

    /**
     * Reads the header and every field of the table, which must have the
     * reader's layout.
     *
     * @throws {DescriptionMismatch} If the size slot doesn't hold the table's
     * length.
     */
    readTable(table: TableDescription): TableDescription {
        const buffer = this.#buffer;
        const { layout } = table;

        if (layout.sized && buffer.getSize() !== buffer.bytes().length) {
            throw new DescriptionMismatch(
                `The size slot holds ${buffer.getSize()}, but the table has ${buffer.bytes().length} bytes`
            );
        }

        const version = layout.versioned ? buffer.getVersion() : undefined;
        const fields = table.fields.map((field, index) =>
            this.readRootField(index, field)
        );

        return version === undefined
            ? { layout, fields }
            : { layout, version, fields };
    }

    readRootField(index: number, field: FieldDescription): FieldDescription {
        return this.#readField(this.#buffer.rootOffset(index), field);
    }

    #readField(position: number, field: FieldDescription): FieldDescription {
        const buffer = this.#buffer;

        switch (field.type) {
            case 'int8':
                return { ...field, value: buffer.getInt8(position) };

            case 'uint8':
                return { ...field, value: buffer.getUint8(position) };

            case 'int16':
                return { ...field, value: buffer.getInt16(position) };

            case 'uint16':
                return { ...field, value: buffer.getUint16(position) };

            case 'int32':
                return { ...field, value: buffer.getInt32(position) };

            case 'uint32':
                return { ...field, value: buffer.getUint32(position) };

            case 'int64':
                return { ...field, value: String(buffer.getInt64(position)) };

            case 'uint64':
                return { ...field, value: String(buffer.getUint64(position)) };

            case 'float32':
                return {
                    ...field,
                    value: toJsonNumber(buffer.getFloat32(position))
                };

            case 'float64':
                return {
                    ...field,
                    value: toJsonNumber(buffer.getFloat64(position))
                };

            case 'bool':
                return { ...field, value: buffer.getUint8(position) !== 0 };

            case 'int8[]':
                return {
                    ...field,
                    value: Array.from(buffer.getInt8Array(position))
                };

            case 'uint8[]':
                return {
                    ...field,
                    value: Array.from(buffer.getUint8Array(position))
                };

            case 'int16[]':
                return {
                    ...field,
                    value: Array.from(buffer.getInt16Array(position))
                };

            case 'uint16[]':
                return {
                    ...field,
                    value: Array.from(buffer.getUint16Array(position))
                };

            case 'int32[]':
                return {
                    ...field,
                    value: Array.from(buffer.getInt32Array(position))
                };

            case 'uint32[]':
                return {
                    ...field,
                    value: Array.from(buffer.getUint32Array(position))
                };

            case 'int64[]':
                return {
                    ...field,
                    value: Array.from(buffer.getInt64Array(position), String)
                };

            case 'uint64[]':
                return {
                    ...field,
                    value: Array.from(buffer.getUint64Array(position), String)
                };

            case 'float32[]':
                return {
                    ...field,
                    value: Array.from(
                        buffer.getFloat32Array(position),
                        toJsonNumber
                    )
                };

            case 'float64[]':
                return {
                    ...field,
                    value: Array.from(
                        buffer.getFloat64Array(position),
                        toJsonNumber
                    )
                };

            case 'string':
                return { ...field, value: buffer.getString(position) };

            case 'struct': {
                const types = field.fields.map(({ type }) => type);
                const values = readStruct(
                    buffer.getStruct(position, structSize(types)),
                    types,
                    0
                );

                return {
                    ...field,
                    fields: field.fields.map((scalar, index) => ({
                        ...scalar,
                        value: values[index]
                    }))
                };
            }

            case 'struct[]':
                return {
                    ...field,
                    items: this.#readStructArray(position, field)
                };

            case 'table':
                return {
                    ...field,
                    table: this.#readNested(position, field.table)
                };

            case 'vector':
                return {
                    ...field,
                    items: this.#readVector(position, field.items)
                };
        }
    }

    /**
     * @throws {DescriptionMismatch} If the vector holds another number of
     * items.
     */
    #readVector(position: number, items: NestedTable[]): NestedTable[] {
        const offsets = this.#buffer.getVectorOffsets(position);
        if (offsets.length !== items.length) {
            throw new DescriptionMismatch(
                `The vector holds ${offsets.length} items rather than ${items.length}`
            );
        }

        // Finding each item from its ends alone finds the same item
        for (const [index, offset] of offsets.entries()) {
            const found = this.#buffer.getVectorOffset(position, index);
            if (found !== offset) {
                throw new DescriptionMismatch(
                    `Item ${index} of the vector is at ${found} from its ends, but at ${offset} among every item`
                );
            }
        }

        const vector = this.#buffer.getVector(position);

        return items.map((item, index) => ({
            ...item,
            table: this.#readsInPlace(item.table)
                ? this.#readInPlace(offsets[index], item.table)
                : readTable(vector[index], item.table)
        }));
    }

    #readStructArray(
        position: number,
        { fields }: StructArrayField
    ): JsonNumber[][] {
        const size = structSize(fields);
        const bytes = this.#buffer.getStructArray(position, size);

        return Array.from({ length: bytes.length / size }, (_, index) =>
            readStruct(bytes, fields, index * size)
        );
    }

    /** Reads a table nested with `setTable()` */
    #readNested(position: number, table: TableDescription): TableDescription {
        if (this.#readsInPlace(table)) {
            return this.#readInPlace(position, table);
        }

        // Its length isn't written, so it's read up to the end of its parent
        return readTable(this.#buffer.bytes().subarray(position), table);
    }

    #readsInPlace({ layout }: TableDescription): boolean {
        return (
            !layout.sized &&
            !layout.versioned &&
            layout.offsetType === this.#layout.offsetType
        );
    }

    #readInPlace(position: number, table: TableDescription): TableDescription {
        const buffer = this.#buffer;

        return {
            layout: table.layout,
            fields: table.fields.map((field, index) =>
                this.#readField(buffer.fieldOffset(position, index), field)
            )
        };
    }
}

/** Reads a table from its own bytes, as {@link TableReader.readTable} does */
export function readTable(
    bytes: Uint8Array,
    table: TableDescription
): TableDescription {
    return new TableReader(bytes, table.layout).readTable(table);
}
