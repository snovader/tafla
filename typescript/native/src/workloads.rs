//! The functions of the benchmarks' schema, `workloads.tafla`, written by
//! hand as tafla-gen might generate them for an addon, to time them before
//! it does. They read and write the items of an `ItemList` as a column of
//! each of an item's fields, through the code that tafla-gen generates for
//! Rust, which knows where each field lies.

use napi::bindgen_prelude::{JsObjectValue, Object, Result};
use napi::sys;

use crate::call::{Arguments, Elements, Failure, respond};
use crate::column::Column;

/// The code that tafla-gen generates from the schema for Rust's benchmarks
#[allow(dead_code, clippy::all, clippy::pedantic, clippy::nursery)]
#[path = "../../../rust/benchmarks/src/codecs/tafla_gen/workloads.rs"]
mod generated;

use generated::{ItemFields, ItemList, ItemListFields, PairFields};

/// `readItemListItems(bytes, value, first, second)`: reads the fields of
/// every item of the `ItemList` in `bytes` into the `Float32Array`s, and
/// returns the number of items
unsafe extern "C" fn read_item_list_items(
    env: sys::napi_env,
    info: sys::napi_callback_info,
) -> sys::napi_value {
    // SAFETY: Node-API calls this with its own `env` and `info`
    let arguments = unsafe { Arguments::<4>::new(env, info) };

    respond(env, read_items(&arguments).map(Some))
}

fn read_items(arguments: &Arguments<4>) -> std::result::Result<u32, Failure> {
    let bytes = arguments.bytes(0)?;
    let mut value = arguments.elements(1, &f32::TYPE, bytes)?;
    let mut first = arguments.elements(2, &f32::TYPE, bytes)?;
    let mut second = arguments.elements(3, &f32::TYPE, bytes)?;

    let items = ItemList::new(bytes).items()?;
    let room = value.len().min(first.len()).min(second.len());
    if items.len() > room {
        return Err(Failure::Range(format!(
            "The list has {} items, and the columns have room for {room}",
            items.len()
        )));
    }

    for (index, item) in items.iter().enumerate() {
        let item = item?;
        let pair = item.pair()?;

        value.set_sized::<4>(index, &item.value()?.to_le_bytes());
        first.set_sized::<4>(index, &pair.first()?.to_le_bytes());
        second.set_sized::<4>(index, &pair.second()?.to_le_bytes());
    }

    // A vector's count is read from an offset, which has at most 32 bits
    Ok(u32::try_from(items.len()).unwrap_or(u32::MAX))
}

/// `writeItemListItems(count, value, first, second, target)`: writes an
/// `ItemList` of `count` items, from the first `count` elements of the
/// `Float32Array`s, to the start of `target`, and returns the table's
/// length. Writes nothing if `target` is shorter than that.
unsafe extern "C" fn write_item_list_items(
    env: sys::napi_env,
    info: sys::napi_callback_info,
) -> sys::napi_value {
    // SAFETY: Node-API calls this with its own `env` and `info`
    let arguments = unsafe { Arguments::<5>::new(env, info) };

    respond(env, write_items(&arguments).map(Some))
}

fn write_items(arguments: &Arguments<5>) -> std::result::Result<u32, Failure> {
    // The columns are read through their pointers, so the target may share
    // their bytes, which then holds what was written last
    let count = arguments.position(0)?;
    let value = arguments.elements(1, &f32::TYPE, &[])?;
    let first = arguments.elements(2, &f32::TYPE, &[])?;
    let second = arguments.elements(3, &f32::TYPE, &[])?;
    let mut target = arguments.elements(4, &u8::TYPE, &[])?;

    let held = value.len().min(first.len()).min(second.len());
    if count > held {
        return Err(Failure::Range(format!(
            "Expected the columns to hold {count} items, and they hold {held}"
        )));
    }

    let float =
        |column: &Elements<'_>, index| column.get_sized::<4>(index).map_or(0.0, f32::from_le_bytes);
    let items: Vec<ItemFields> = (0..count)
        .map(|index| ItemFields {
            value: float(&value, index),
            pair: PairFields {
                first: float(&first, index),
                second: float(&second, index),
            },
        })
        .collect();

    let bytes = ItemListFields { items: &items }.serialize()?;
    if bytes.len() <= target.len() {
        target.copy_from(&bytes);
    }

    // A table of the default layout is at most 4 GiB long
    Ok(u32::try_from(bytes.len()).unwrap_or(u32::MAX))
}

/// Adds the schema's functions to the addon's module
///
/// # Errors
///
/// Node-API's error, if it can't create a function.
pub fn register(exports: &mut Object) -> Result<()> {
    exports.create_named_method("readItemListItems", read_item_list_items)?;
    exports.create_named_method("writeItemListItems", write_item_list_items)?;

    Ok(())
}
