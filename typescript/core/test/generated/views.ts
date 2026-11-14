/**
 * The view that a generated getter of a vector or an array of structs
 * returns, in either language, which reads each item when it's asked for
 */
export interface View<T> {
    length(): number;
    get(index: number): T;
}

/** Reads a view's items from the first to the last, into an array */
export function readItems<T, F>(view: View<T>, read: (item: T) => F): F[] {
    const items = new Array<F>(view.length());
    for (let i = 0; i < items.length; i++) {
        items[i] = read(view.get(i));
    }

    return items;
}

/**
 * Reads a view's items from the last to the first, as a reader that skips
 * items would, into an array in order
 */
export function readItemsFromLast<T, F>(
    view: View<T>,
    read: (item: T) => F
): F[] {
    const items = new Array<F>(view.length());
    for (let i = items.length - 1; i >= 0; i--) {
        items[i] = read(view.get(i));
    }

    return items;
}
