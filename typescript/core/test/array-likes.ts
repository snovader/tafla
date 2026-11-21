/**
 * An array-like of `values` whose length reads as `counted` the first time,
 * as a setter counts it, and as the length of `values` after, as if it grew
 * in between
 */
export function growing<T>(
    values: readonly T[],
    counted: number
): ArrayLike<T> {
    let reads = 0;
    const arrayLike: { [index: number]: T; readonly length: number } = {
        get length(): number {
            reads++;
            return reads === 1 ? counted : values.length;
        }
    };
    for (let i = 0; i < values.length; i++) {
        arrayLike[i] = values[i];
    }

    return arrayLike;
}
