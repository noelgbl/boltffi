pub mod handle;
pub mod pending;
pub mod ringbuffer;
pub mod rustfuture;
pub mod safety;
pub mod status;
pub mod subscription;
pub mod types;

pub use handle::HandleBox;
pub use pending::{CancellationToken, PendingHandle};
pub use riff_macros::{
    Data, FfiType, data, error, export, ffi_class, ffi_export, ffi_stream, ffi_trait, name, skip,
};
pub use ringbuffer::SpscRingBuffer;
pub use rustfuture::{
    RustFuture, RustFutureContinuationCallback, RustFutureHandle, RustFuturePoll,
};
pub use safety::catch_ffi_panic;
pub use status::{FfiStatus, clear_last_error, set_last_error, take_last_error};
pub use subscription::{
    EventSubscription, StreamContinuationCallback, StreamPollResult, StreamProducer,
    SubscriptionHandle, WaitResult,
};
pub use types::{FfiBuf, FfiError, FfiOption, FfiSlice, FfiString};

pub const VERSION_MAJOR: u32 = 0;
pub const VERSION_MINOR: u32 = 1;
pub const VERSION_PATCH: u32 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn riff_version_major() -> u32 {
    VERSION_MAJOR
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_version_minor() -> u32 {
    VERSION_MINOR
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_version_patch() -> u32 {
    VERSION_PATCH
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_free_string(string: FfiString) {
    drop(string);
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_last_error_message(out: *mut FfiString) -> FfiStatus {
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
pub extern "C" fn riff_clear_last_error() {
    clear_last_error();
}

pub fn fail_with_error(status: FfiStatus, message: impl Into<String>) -> FfiStatus {
    set_last_error(message);
    status
}
