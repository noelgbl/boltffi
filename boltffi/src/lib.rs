pub use boltffi_core::{
    CallbackForeignType, CallbackHandle, CustomFfiConvertible, CustomTypeConversionError, Data,
    EventSubscription, FfiType, FromCallbackHandle, StreamProducer, UnexpectedFfiCallbackError,
    custom_ffi, custom_type, data, default, error, export, ffi_stream, name, skip,
};

#[doc(hidden)]
pub mod __private {
    pub use boltffi_core::{
        CallbackForeignType, CallbackHandle, EventSubscription, FfiBuf, FfiStatus,
        FromCallbackHandle, RustFutureContinuationCallback, RustFutureHandle,
        StreamContinuationCallback, StreamPollResult, SubscriptionHandle, WaitResult,
        rustfuture, set_last_error, wire,
    };
    #[cfg(target_arch = "wasm32")]
    pub use boltffi_core::{
        WasmCallbackOutBuf, rust_future_panic_message, rust_future_poll_sync,
    };
}
