import {
    toJsonNumber,
    type FieldDescription,
    type JsonNumber,
    type LayoutDescription,
    type ScalarType,
    type TableDescription
} from '../conformance/description.ts';

// The fields of generated messages and tables in the conformance suite's
// model, so that `buildTable()` writes the same table by hand with
// `TableBuilder`, as the suite builds its tables, for tests to compare with
// the bytes that the generated code writes.

/** The layout of tables and of messages without attributes */
export const DEFAULT_LAYOUT: LayoutDescription = {
    sized: false,
    versioned: false,
    offsetType: 'uint32'
};

export function scalar(
    type: ScalarType,
    value: number | bigint
): FieldDescription {
    return { type, value: jsonOf(value) };
}

export function bool(value: boolean): FieldDescription {
    return { type: 'bool', value };
}

export function array(
    type: ScalarType,
    values: ArrayLike<number> | ArrayLike<bigint>
): FieldDescription {
    return { type: `${type}[]`, value: Array.from(values, jsonOf) };
}

export function string(value: string): FieldDescription {
    return { type: 'string', value };
}

/** A struct of scalars of these types and values, as generated code writes it */
export function struct(
    scalars: ReadonlyArray<readonly [ScalarType, number | bigint]>
): FieldDescription {
    return {
        type: 'struct',
        fields: scalars.map(([type, value]) => ({ type, value: jsonOf(value) }))
    };
}

/** An array of structs of scalars of these types, with each struct's values */
export function structArray(
    types: ScalarType[],
    items: ReadonlyArray<ReadonlyArray<number | bigint>>
): FieldDescription {
    return {
        type: 'struct[]',
        fields: types,
        items: items.map(values => values.map(jsonOf))
    };
}

/** A table nested from its builder at its default alignment, as generated code nests it */
export function table(fields: FieldDescription[]): FieldDescription {
    return {
        type: 'table',
        from: 'builder',
        table: { layout: DEFAULT_LAYOUT, fields }
    };
}

/** A vector of tables from their builders, as generated code writes it */
export function vector(items: readonly FieldDescription[][]): FieldDescription {
    return {
        type: 'vector',
        items: items.map(fields => ({
            from: 'builder',
            table: { layout: DEFAULT_LAYOUT, fields }
        }))
    };
}

/** A message's table, with its version when its layout is versioned */
export function message(
    layout: LayoutDescription,
    fields: FieldDescription[],
    version?: number
): TableDescription {
    return version == null ? { layout, fields } : { layout, version, fields };
}

function jsonOf(value: number | bigint): JsonNumber {
    return typeof value === 'bigint' ? String(value) : toJsonNumber(value);
}
