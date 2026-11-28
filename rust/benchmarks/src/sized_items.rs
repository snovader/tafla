//! The items of the vector of nested tables as tables with headers of their
//! own, which `TableReader`'s getters read by hand.
//!
//! Each item is a sized table, whose header holds its length, so that
//! `Vector::get()` returns it as a table of its own, from its start to its
//! end, rather than in place, as `Vector::get_in_place()` returns a table
//! without a header. The workloads' schema, which every language's suite
//! shares, has no such vector, and tafla-gen's views read only items in
//! place, so only tafla's runtime reads it.

use tafla::{BuildError, DecodeError, TableBuilder, TableLayout, TableReader};

use crate::codec::Error;
use crate::item_access::{no_items, sum_item};
use crate::workloads::{Item, List};

/// An item's layout: a header that holds the table's length, and 32-bit
/// offsets, as the vector's
pub const ITEM_LAYOUT: TableLayout = TableLayout::DEFAULT.sized();

/// Writes the list as a vector of sized tables, each a float32 and a nested
/// table of two float32s, from a builder of its own
///
/// # Errors
///
/// The builder's error, if it can't write the list.
pub fn write(list: &List) -> Result<Vec<u8>, Error> {
    let items = list
        .items
        .iter()
        .map(item_table)
        .collect::<Result<Vec<_>, _>>()?;

    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
    builder.set_vector(items)?;

    Ok(builder.to_bytes()?)
}

/// Sums the last item, from the bytes that `Vector::get()` returns of it
///
/// # Errors
///
/// A decode error, or an error if the vector is empty.
pub fn last_item(bytes: &[u8]) -> Result<f64, Error> {
    let reader = TableReader::new(bytes, TableLayout::DEFAULT);
    let vector = reader.get_vector(reader.root_offset(0)?)?;
    let last = vector.len().checked_sub(1).ok_or_else(no_items)?;
    let item = read_item(vector.get(last).ok_or_else(no_items)??)?;

    Ok(sum_item(item.value, item.first, item.second))
}

/// Checks that every item reads back from `Vector::get()` as written
///
/// Each item must read as `list`'s item of the same index, with the same
/// bits, from bytes whose length its header gives, and [`last_item()`] must
/// sum the last item to the same bits as `list`'s.
///
/// # Errors
///
/// A decode error, or an error that names the item that differs.
pub fn check(list: &List, bytes: &[u8]) -> Result<(), Error> {
    let reader = TableReader::new(bytes, TableLayout::DEFAULT);
    let vector = reader.get_vector(reader.root_offset(0)?)?;
    if vector.len() != list.items.len() {
        return Err(format!("The vector holds {} items", vector.len()).into());
    }

    for (index, expected) in list.items.iter().enumerate() {
        let item_bytes = vector.get(index).ok_or_else(no_items)??;
        let size = TableReader::new(item_bytes, ITEM_LAYOUT).size()?;
        if size != item_bytes.len() {
            let length = item_bytes.len();
            return Err(format!("Item {index} has {length} bytes, not {size}").into());
        }

        let item = read_item(item_bytes)?;
        if bits(&item) != bits(expected) {
            return Err(format!("Item {index} reads as {item:?} rather than {expected:?}").into());
        }
    }

    let sum = last_item(bytes)?;
    let item = list.items.last().ok_or_else(no_items)?;
    let expected = sum_item(item.value, item.first, item.second);
    if sum.to_bits() != expected.to_bits() {
        return Err(format!("The last item sums to {sum} rather than {expected}").into());
    }

    Ok(())
}

fn item_table(item: &Item) -> Result<TableBuilder<'static>, BuildError> {
    let mut table = TableBuilder::new(2, ITEM_LAYOUT)?;
    table.set_f32(item.value)?;
    table.set_table_with(2, |pair| {
        pair.set_f32(item.first)?;
        pair.set_f32(item.second)
    })?;

    Ok(table)
}

/// Reads an item as a table of its own, in its layout
fn read_item(bytes: &[u8]) -> Result<Item, DecodeError> {
    let item = TableReader::new(bytes, ITEM_LAYOUT);
    let pair = item.root_offset(1)?;

    Ok(Item {
        value: item.get_f32(item.root_offset(0)?)?,
        first: item.get_f32(item.field_offset(pair, 0)?)?,
        second: item.get_f32(item.field_offset(pair, 1)?)?,
    })
}

const fn bits(item: &Item) -> [u32; 3] {
    [
        item.value.to_bits(),
        item.first.to_bits(),
        item.second.to_bits(),
    ]
}
