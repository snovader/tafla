//! A call of one of the addon's functions: its arguments, as Rust reads
//! them, and what it returns or throws.

use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::ptr;
use std::slice;

use napi::sys;
use tafla::{BuildError, DecodeError};

use crate::column::{Column, ElementType};

/// The `code` of the error that a call throws for a decode error
const DECODE_ERROR_CODE: &CStr = c"TAFLA_DECODE";

/// The `code` of the error that a call throws for a table that can't be
/// written
const BUILD_ERROR_CODE: &CStr = c"TAFLA_BUILD";

/// What a call throws rather than returning
pub enum Failure {
    /// An `Error` with the decode error's message, and the code
    /// `TAFLA_DECODE`
    Decode(DecodeError),
    /// An `Error` with the build error's message, and the code `TAFLA_BUILD`
    Build(BuildError),
    /// A `TypeError`, for an argument of another type than the function's
    Type(&'static str),
    /// A `RangeError`
    Range(String),
}

impl From<DecodeError> for Failure {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

impl From<BuildError> for Failure {
    fn from(error: BuildError) -> Self {
        Self::Build(error)
    }
}

/// The first `N` arguments of a call, which are `undefined` where the caller
/// passed fewer
pub struct Arguments<const N: usize> {
    env: sys::napi_env,
    values: [sys::napi_value; N],
    count: usize,
}

impl<const N: usize> Arguments<N> {
    /// # Safety
    ///
    /// `env` and `info` are those that Node-API called the callback with,
    /// and the arguments aren't used after the callback returns.
    pub unsafe fn new(env: sys::napi_env, info: sys::napi_callback_info) -> Self {
        let mut count = N;
        let mut values = [ptr::null_mut(); N];

        // SAFETY: `values` has room for the `count` values that this asks
        // for, and `count` becomes the arguments that the caller passed. If
        // it fails, the values stay null, which every read of an argument
        // rejects.
        unsafe {
            sys::napi_get_cb_info(
                env,
                info,
                &raw mut count,
                values.as_mut_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
        }

        Self { env, values, count }
    }

    /// How many arguments the caller passed, which can be more than `N`
    pub const fn count(&self) -> usize {
        self.count
    }

    /// The bytes of the `Uint8Array` at `index`, for as long as the call.
    /// The caller must not write to them from another thread meanwhile,
    /// through a `SharedArrayBuffer`.
    ///
    /// # Errors
    ///
    /// [`Failure::Type`] if the argument is anything else.
    pub fn bytes(&self, index: usize) -> Result<&[u8], Failure> {
        let bytes = self.typed_array(index, &u8::TYPE, "Expected bytes to be a Uint8Array")?;

        if bytes.len == 0 {
            return Ok(&[]);
        }

        // SAFETY: Node-API gave the pointer and the length of the array's
        // bytes, which stay where they are while the call holds its
        // arguments, and nothing writes to them during the call: `elements()`
        // rejects an array to write to that overlaps them.
        Ok(unsafe { slice::from_raw_parts(bytes.pointer, bytes.len) })
    }

    /// The typed array of `element`s at `index`, to read values into
    ///
    /// # Errors
    ///
    /// [`Failure::Type`] if the argument is anything else, or if it shares
    /// any of `read`'s bytes, which the call reads while it writes to it.
    pub fn elements(
        &self,
        index: usize,
        element: &ElementType,
        read: &[u8],
    ) -> Result<Elements<'_>, Failure> {
        let elements = self.typed_array(index, element, element.expected)?;

        if elements.overlaps(read) {
            return Err(Failure::Type(
                "Expected out not to share the bytes that are read",
            ));
        }

        Ok(elements)
    }

    /// The `Uint32Array` at `index`, of the indexes of fields
    ///
    /// # Errors
    ///
    /// [`Failure::Type`] if the argument is anything else.
    pub fn path(&self, index: usize) -> Result<Elements<'_>, Failure> {
        self.typed_array(index, &u32::TYPE, "Expected path to be a Uint32Array")
    }

    /// The position or the count at `index`, a number that Node-API converts
    /// to a `u32` as `>>> 0` does. A reader checks any position against its
    /// bytes.
    ///
    /// # Errors
    ///
    /// [`Failure::Type`] if the argument isn't a number.
    pub fn position(&self, index: usize) -> Result<usize, Failure> {
        let mut position = 0;

        // SAFETY: The value is the call's argument, or null, which fails
        let status =
            unsafe { sys::napi_get_value_uint32(self.env, self.values[index], &raw mut position) };
        if status != sys::Status::napi_ok {
            return Err(Failure::Type(
                "Expected a position or a count to be a number",
            ));
        }

        Ok(position as usize)
    }

    fn typed_array(
        &self,
        index: usize,
        element: &ElementType,
        expected: &'static str,
    ) -> Result<Elements<'_>, Failure> {
        let mut array = -1;
        let mut len = 0;
        let mut data = ptr::null_mut();

        // SAFETY: The value is the call's argument, or null, which fails as
        // any value but a typed array does
        let status = unsafe {
            sys::napi_get_typedarray_info(
                self.env,
                self.values[index],
                &raw mut array,
                &raw mut len,
                &raw mut data,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if status != sys::Status::napi_ok || array != element.array {
            return Err(Failure::Type(expected));
        }

        Ok(Elements {
            pointer: data.cast(),
            len,
            size: element.size,
            call: PhantomData,
        })
    }
}

/// The elements of a typed array that a call was passed, for as long as the
/// call. They're read and written as bytes through its pointer, without an
/// alignment that an `ArrayBuffer` of another addon's memory might not have.
pub struct Elements<'call> {
    pointer: *mut u8,
    /// The array's length in elements
    len: usize,
    /// The bytes of an element: 1, 2, 4 or 8
    size: usize,
    call: PhantomData<&'call ()>,
}

impl Elements<'_> {
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Iterates over the elements of a `Uint32Array`
    pub fn indexes(&self) -> impl Iterator<Item = u32> {
        (0..self.len)
            .filter_map(|index| self.get_sized::<4>(index))
            .map(u32::from_le_bytes)
    }

    /// The little-endian bytes of the element at `index`, in an array of
    /// elements of `SIZE` bytes, or `None` past the end
    #[inline]
    pub fn get_sized<const SIZE: usize>(&self, index: usize) -> Option<[u8; SIZE]> {
        debug_assert_eq!(self.size, SIZE);
        if index >= self.len {
            return None;
        }

        // SAFETY: The index is within the array's elements of `SIZE` bytes,
        // which Node-API gave the pointer and the length of
        let mut bytes = unsafe {
            self.pointer
                .add(index * SIZE)
                .cast::<[u8; SIZE]>()
                .read_unaligned()
        };
        if cfg!(target_endian = "big") {
            bytes.reverse();
        }

        Some(bytes)
    }

    /// Copies `bytes` to the start of a `Uint8Array`
    ///
    /// # Panics
    ///
    /// If the array is shorter than `bytes`.
    pub fn copy_from(&mut self, bytes: &[u8]) {
        debug_assert_eq!(self.size, 1);
        assert!(
            bytes.len() <= self.len,
            "The bytes are past the array's end"
        );

        // SAFETY: The array has room for the bytes, which are Rust's own and
        // so don't overlap it
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), self.pointer, bytes.len()) };
    }

    /// Sets the element at `index` to the value of the little-endian bytes
    /// at the start of `value`, as a table stores it
    ///
    /// # Panics
    ///
    /// If `index` is past the end, or `value` is shorter than an element.
    #[inline]
    pub fn set(&mut self, index: usize, value: &[u8]) {
        // A copy of each size, which the compiler makes a load and a store
        match self.size {
            1 => self.set_sized::<1>(index, value),
            2 => self.set_sized::<2>(index, value),
            4 => self.set_sized::<4>(index, value),
            _ => self.set_sized::<8>(index, value),
        }
    }

    /// Sets the element at `index` as [`Elements::set()`] does, in an array
    /// of elements of `SIZE` bytes
    ///
    /// # Panics
    ///
    /// If `index` is past the end, or `value` is shorter than an element.
    #[inline]
    pub fn set_sized<const SIZE: usize>(&mut self, index: usize, value: &[u8]) {
        debug_assert_eq!(self.size, SIZE);
        assert!(index < self.len, "The index is past the array's end");

        let mut bytes: [u8; SIZE] = value[..SIZE].try_into().unwrap_or([0; SIZE]);
        if cfg!(target_endian = "big") {
            bytes.reverse();
        }

        // SAFETY: The index is within the array's elements of `SIZE` bytes,
        // which Node-API gave the pointer and the length of, and no
        // reference borrows them
        unsafe {
            self.pointer
                .add(index * SIZE)
                .cast::<[u8; SIZE]>()
                .write_unaligned(bytes);
        }
    }

    fn overlaps(&self, bytes: &[u8]) -> bool {
        let start = self.pointer.addr();
        let end = start + self.len * self.size;
        let read = bytes.as_ptr_range();

        self.len > 0 && start < read.end.addr() && read.start.addr() < end
    }
}

/// Returns a call's result to JavaScript: `undefined`, or a number. Throws a
/// failure instead, with `undefined` as the value that Node-API ignores.
pub fn respond(env: sys::napi_env, result: Result<Option<u32>, Failure>) -> sys::napi_value {
    let mut value = ptr::null_mut();

    match result {
        Ok(None) => {}
        // SAFETY: `env` is the call's, and `value` is written before it's read
        Ok(Some(number)) => unsafe {
            sys::napi_create_uint32(env, number, &raw mut value);
        },
        Err(failure) => throw(env, &failure),
    }

    value
}

fn throw(env: sys::napi_env, failure: &Failure) {
    let message = |text: &str| CString::new(text).unwrap_or_default();

    // SAFETY: `env` is the call's, and the code and the messages are C
    // strings that outlive the calls, which copy them
    unsafe {
        match failure {
            Failure::Decode(error) => {
                let message = message(&error.to_string());
                sys::napi_throw_error(env, DECODE_ERROR_CODE.as_ptr(), message.as_ptr());
            }
            Failure::Build(error) => {
                let message = message(&error.to_string());
                sys::napi_throw_error(env, BUILD_ERROR_CODE.as_ptr(), message.as_ptr());
            }
            Failure::Type(text) => {
                sys::napi_throw_type_error(env, ptr::null(), message(text).as_ptr());
            }
            Failure::Range(text) => {
                sys::napi_throw_range_error(env, ptr::null(), message(text).as_ptr());
            }
        }
    }
}
