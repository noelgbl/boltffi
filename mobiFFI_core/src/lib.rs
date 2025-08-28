#![allow(unused)]

pub mod handle;
pub mod safety;
pub mod status;
pub mod types;

pub use handle::HandleBox;
pub use mobiFFI_macros::{ffi_export, FfiType};
pub use safety::catch_ffi_panic;
pub use status::{clear_last_error, set_last_error, take_last_error, FfiStatus};
pub use types::{FfiBuf, FfiOption, FfiSlice, FfiString};

unsafe fn read_input_str<'a>(ptr: *const u8, len: usize) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    let bytes = core::slice::from_raw_parts(ptr, len);
    core::str::from_utf8(bytes).ok()
}

pub const VERSION_MAJOR: u32 = 0;
pub const VERSION_MINOR: u32 = 1;
pub const VERSION_PATCH: u32 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn mffi_version_major() -> u32 {
    VERSION_MAJOR
}

#[unsafe(no_mangle)]
pub extern "C" fn mffi_version_minor() -> u32 {
    VERSION_MINOR
}

#[unsafe(no_mangle)]
pub extern "C" fn mffi_version_patch() -> u32 {
    VERSION_PATCH
}

#[unsafe(no_mangle)]
pub extern "C" fn mffi_free_buf_u8(buf: FfiBuf<u8>) {
    drop(buf);
}

#[unsafe(no_mangle)]
pub extern "C" fn mffi_free_string(string: FfiString) {
    drop(string);
}

#[unsafe(no_mangle)]
pub extern "C" fn mffi_last_error_message(out: *mut FfiString) -> FfiStatus {
    if out.is_null() {
        return FfiStatus::NULL_POINTER;
    }
    
    match take_last_error() {
        Some(message) => {
            unsafe { *out = FfiString::from(message) };
            FfiStatus::OK
        }
        None => {
            unsafe { *out = FfiString::from("") };
            FfiStatus::OK
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mffi_clear_last_error() {
    clear_last_error();
}

fn fail_with_error(status: FfiStatus, message: impl Into<String>) -> FfiStatus {
    set_last_error(message);
    status
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_greeting(
    name_ptr: *const u8,
    name_len: usize,
    out: *mut FfiString,
) -> FfiStatus {
    if out.is_null() {
        return fail_with_error(FfiStatus::NULL_POINTER, "output pointer is null");
    }

    let name = match read_input_str(name_ptr, name_len) {
        Some(name) => name,
        None => return fail_with_error(FfiStatus::INVALID_ARG, "name is not valid UTF-8"),
    };

    let greeting = format!("Hello, {}!", name);
    *out = FfiString::from(greeting);

    FfiStatus::OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_concat(
    first_ptr: *const u8,
    first_len: usize,
    second_ptr: *const u8,
    second_len: usize,
    out: *mut FfiString,
) -> FfiStatus {
    if out.is_null() {
        return fail_with_error(FfiStatus::NULL_POINTER, "output pointer is null");
    }

    let first = match read_input_str(first_ptr, first_len) {
        Some(string) => string,
        None => return fail_with_error(FfiStatus::INVALID_ARG, "first string is not valid UTF-8"),
    };

    let second = match read_input_str(second_ptr, second_len) {
        Some(string) => string,
        None => return fail_with_error(FfiStatus::INVALID_ARG, "second string is not valid UTF-8"),
    };

    let result = format!("{}{}", first, second);
    *out = FfiString::from(result);

    FfiStatus::OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_copy_bytes(
    src: *const u8,
    src_len: usize,
    dst: *mut u8,
    dst_cap: usize,
    written: *mut usize,
) -> FfiStatus {
    if src.is_null() || dst.is_null() || written.is_null() {
        return FfiStatus::NULL_POINTER;
    }

    if src_len > dst_cap {
        return FfiStatus::BUFFER_TOO_SMALL;
    }

    core::ptr::copy_nonoverlapping(src, dst, src_len);
    *written = src_len;

    FfiStatus::OK
}

struct Counter {
    value: u64,
}

#[unsafe(no_mangle)]
pub extern "C" fn mffi_counter_new(initial: u64) -> *mut Counter {
    HandleBox::new(Counter { value: initial }).into_raw()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_counter_increment(handle: *mut Counter) -> FfiStatus {
    match HandleBox::from_raw(handle) {
        Some(mut counter) => {
            counter.as_mut().value += 1;
            core::mem::forget(counter);
            FfiStatus::OK
        }
        None => FfiStatus::NULL_POINTER,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_counter_get(handle: *mut Counter, out: *mut u64) -> FfiStatus {
    if out.is_null() {
        return FfiStatus::NULL_POINTER;
    }
    match HandleBox::from_raw(handle) {
        Some(counter) => {
            *out = counter.as_ref().value;
            core::mem::forget(counter);
            FfiStatus::OK
        }
        None => FfiStatus::NULL_POINTER,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_counter_free(handle: *mut Counter) {
    if let Some(counter) = HandleBox::from_raw(handle) {
        drop(counter);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DataPoint {
    pub x: f64,
    pub y: f64,
    pub timestamp: i64,
}

struct DataStore {
    items: Vec<DataPoint>,
}

#[unsafe(no_mangle)]
pub extern "C" fn mffi_datastore_new() -> *mut DataStore {
    HandleBox::new(DataStore { items: Vec::new() }).into_raw()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_datastore_add(handle: *mut DataStore, point: DataPoint) -> FfiStatus {
    match HandleBox::from_raw(handle) {
        Some(mut store) => {
            store.as_mut().items.push(point);
            core::mem::forget(store);
            FfiStatus::OK
        }
        None => FfiStatus::NULL_POINTER,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_datastore_len(handle: *mut DataStore) -> usize {
    match HandleBox::from_raw(handle) {
        Some(store) => {
            let len = store.as_ref().items.len();
            core::mem::forget(store);
            len
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_datastore_copy_into(
    handle: *mut DataStore,
    dst: *mut DataPoint,
    dst_cap: usize,
    written: *mut usize,
) -> FfiStatus {
    if dst.is_null() || written.is_null() {
        return FfiStatus::NULL_POINTER;
    }

    match HandleBox::from_raw(handle) {
        Some(store) => {
            let items = &store.as_ref().items;
            let items_len = items.len();
            let copy_count = items_len.min(dst_cap);

            core::ptr::copy_nonoverlapping(items.as_ptr(), dst, copy_count);
            *written = copy_count;

            let status = if items_len > dst_cap {
                FfiStatus::BUFFER_TOO_SMALL
            } else {
                FfiStatus::OK
            };

            core::mem::forget(store);
            status
        }
        None => FfiStatus::NULL_POINTER,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_datastore_free(handle: *mut DataStore) {
    if let Some(store) = HandleBox::from_raw(handle) {
        drop(store);
    }
}

pub type DataPointCallback = extern "C" fn(user_data: *mut core::ffi::c_void, point: DataPoint);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_datastore_foreach(
    handle: *mut DataStore,
    callback: DataPointCallback,
    user_data: *mut core::ffi::c_void,
) -> FfiStatus {
    match HandleBox::from_raw(handle) {
        Some(store) => {
            store.as_ref().items.iter().for_each(|point| {
                callback(user_data, *point);
            });
            core::mem::forget(store);
            FfiStatus::OK
        }
        None => fail_with_error(FfiStatus::NULL_POINTER, "datastore handle is null"),
    }
}

pub type SumCallback = extern "C" fn(user_data: *mut core::ffi::c_void, sum: f64);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_datastore_sum_async(
    handle: *mut DataStore,
    callback: SumCallback,
    user_data: *mut core::ffi::c_void,
) -> FfiStatus {
    match HandleBox::from_raw(handle) {
        Some(store) => {
            let sum: f64 = store.as_ref().items.iter().map(|p| p.x + p.y).sum();
            core::mem::forget(store);
            callback(user_data, sum);
            FfiStatus::OK
        }
        None => fail_with_error(FfiStatus::NULL_POINTER, "datastore handle is null"),
    }
}

#[ffi_export]
pub fn add_numbers(first: i32, second: i32) -> i32 {
    first + second
}

#[ffi_export]
pub fn multiply_floats(first: f64, second: f64) -> f64 {
    first * second
}

#[ffi_export]
pub fn make_greeting(name_ptr: *const u8, name_len: usize) -> String {
    let name = unsafe { 
        if name_ptr.is_null() {
            "World"
        } else {
            let bytes = core::slice::from_raw_parts(name_ptr, name_len);
            core::str::from_utf8(bytes).unwrap_or("World")
        }
    };
    format!("Hello, {}!", name)
}

#[ffi_export]
pub fn safe_divide(numerator: i32, denominator: i32) -> Result<i32, &'static str> {
    if denominator == 0 {
        Err("division by zero")
    } else {
        Ok(numerator / denominator)
    }
}
