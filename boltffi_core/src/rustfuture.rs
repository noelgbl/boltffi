use std::future::Future;
use std::pin::Pin;
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmPollStatus {
    Pending = 0,
    Ready = 1,
    Cancelled = -1,
    Panicked = -2,
}

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    fn __boltffi_wake(handle: u32);
}

#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustFuturePoll {
    Ready = 0,
    MaybeReady = 1,
}

pub type RustFutureContinuationCallback = extern "C" fn(callback_data: u64, RustFuturePoll);

#[derive(Clone, Copy)]
struct ContinuationCallback(RustFutureContinuationCallback);

impl ContinuationCallback {
    fn from_raw_ptr(ptr: *mut ()) -> Option<Self> {
        (!ptr.is_null()).then(|| {
            Self(unsafe { std::mem::transmute::<*mut (), RustFutureContinuationCallback>(ptr) })
        })
    }

    fn into_raw_ptr(self) -> *mut () {
        self.0 as *mut ()
    }

    fn invoke(self, callback_data: ContinuationData, poll_result: RustFuturePoll) {
        (self.0)(callback_data.into_raw(), poll_result)
    }
}

#[derive(Clone, Copy, Default)]
struct ContinuationData(u64);

impl ContinuationData {
    fn from_raw(value: u64) -> Self {
        Self(value)
    }

    fn into_raw(self) -> u64 {
        self.0
    }
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SchedulerStateTag {
    Empty = 0,
    Waked = 1,
    Cancelled = 2,
    ContinuationStored = 3,
}

impl SchedulerStateTag {
    fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Empty,
            1 => Self::Waked,
            2 => Self::Cancelled,
            3 => Self::ContinuationStored,
            _ => Self::Empty,
        }
    }

    fn into_raw(self) -> u8 {
        self as u8
    }
}

struct AtomicContinuationScheduler {
    state_tag: AtomicU8,
    stored_callback_data: AtomicU64,
    stored_callback_ptr: AtomicPtr<()>,
}

impl AtomicContinuationScheduler {
    fn new() -> Self {
        Self {
            state_tag: AtomicU8::new(SchedulerStateTag::Empty.into_raw()),
            stored_callback_data: AtomicU64::new(0),
            stored_callback_ptr: AtomicPtr::new(ptr::null_mut()),
        }
    }

    fn current_state(&self) -> SchedulerStateTag {
        SchedulerStateTag::from_raw(self.state_tag.load(Ordering::Acquire))
    }

    fn try_transition(&self, from: SchedulerStateTag, to: SchedulerStateTag) -> bool {
        self.state_tag
            .compare_exchange(
                from.into_raw(),
                to.into_raw(),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn load_stored_continuation(&self) -> (Option<ContinuationCallback>, ContinuationData) {
        let callback_ptr = self.stored_callback_ptr.load(Ordering::Acquire);
        let callback_data =
            ContinuationData::from_raw(self.stored_callback_data.load(Ordering::Acquire));
        (
            ContinuationCallback::from_raw_ptr(callback_ptr),
            callback_data,
        )
    }

    fn write_continuation(&self, callback: ContinuationCallback, callback_data: ContinuationData) {
        self.stored_callback_data
            .store(callback_data.into_raw(), Ordering::Release);
        self.stored_callback_ptr
            .store(callback.into_raw_ptr(), Ordering::Release);
    }

    fn invoke_stored_continuation(&self, poll_result: RustFuturePoll) {
        let (callback, callback_data) = self.load_stored_continuation();
        if let Some(continuation_callback) = callback {
            continuation_callback.invoke(callback_data, poll_result);
        }
    }

    fn store_continuation(
        &self,
        continuation_callback: ContinuationCallback,
        callback_data: ContinuationData,
    ) {
        loop {
            match self.current_state() {
                SchedulerStateTag::Empty => {
                    self.write_continuation(continuation_callback, callback_data);
                    if self.try_transition(
                        SchedulerStateTag::Empty,
                        SchedulerStateTag::ContinuationStored,
                    ) {
                        return;
                    }
                }
                SchedulerStateTag::ContinuationStored => {
                    self.invoke_stored_continuation(RustFuturePoll::Ready);
                    self.write_continuation(continuation_callback, callback_data);
                    return;
                }
                SchedulerStateTag::Waked => {
                    if self.try_transition(SchedulerStateTag::Waked, SchedulerStateTag::Empty) {
                        continuation_callback.invoke(callback_data, RustFuturePoll::MaybeReady);
                        return;
                    }
                }
                SchedulerStateTag::Cancelled => {
                    continuation_callback.invoke(callback_data, RustFuturePoll::Ready);
                    return;
                }
            }
        }
    }

    fn wake_continuation(&self) {
        loop {
            match self.current_state() {
                SchedulerStateTag::ContinuationStored => {
                    if self.try_transition(
                        SchedulerStateTag::ContinuationStored,
                        SchedulerStateTag::Empty,
                    ) {
                        self.invoke_stored_continuation(RustFuturePoll::MaybeReady);
                        return;
                    }
                }
                SchedulerStateTag::Empty => {
                    if self.try_transition(SchedulerStateTag::Empty, SchedulerStateTag::Waked) {
                        return;
                    }
                }
                SchedulerStateTag::Waked | SchedulerStateTag::Cancelled => return,
            }
        }
    }

    fn mark_cancelled(&self) {
        loop {
            let current_state = self.current_state();
            match current_state {
                SchedulerStateTag::ContinuationStored => {
                    if self.try_transition(
                        SchedulerStateTag::ContinuationStored,
                        SchedulerStateTag::Cancelled,
                    ) {
                        self.invoke_stored_continuation(RustFuturePoll::Ready);
                        return;
                    }
                }
                _ => {
                    if self.try_transition(current_state, SchedulerStateTag::Cancelled) {
                        return;
                    }
                }
            }
        }
    }

    fn is_cancelled(&self) -> bool {
        self.current_state() == SchedulerStateTag::Cancelled
    }
}

unsafe impl Send for AtomicContinuationScheduler {}
unsafe impl Sync for AtomicContinuationScheduler {}

#[derive(Debug)]
pub enum TerminalState {
    Ready,
    Cancelled,
    Panicked(String),
}

#[allow(dead_code)]
enum FutureExecutionState<T> {
    Running(Pin<Box<dyn Future<Output = T> + Send + 'static>>),
    Complete(T),
    Panicked(String),
    Consumed,
}

impl<T> FutureExecutionState<T> {
    fn is_finished(&self) -> bool {
        matches!(self, Self::Complete(_) | Self::Panicked(_) | Self::Consumed)
    }

    #[cfg(target_arch = "wasm32")]
    fn is_panicked(&self) -> bool {
        matches!(self, Self::Panicked(_))
    }

    fn take_result(&mut self) -> Option<T> {
        match std::mem::replace(self, Self::Consumed) {
            Self::Complete(result) => Some(result),
            other => {
                *self = other;
                None
            }
        }
    }

    fn take_panic_message(&mut self) -> Option<String> {
        match std::mem::replace(self, Self::Consumed) {
            Self::Panicked(msg) => Some(msg),
            other => {
                *self = other;
                None
            }
        }
    }
}

pub struct RustFuture<T: Send + 'static> {
    future_execution_state: Mutex<FutureExecutionState<T>>,
    continuation_scheduler: AtomicContinuationScheduler,
}

impl<T: Send + 'static> RustFuture<T> {
    pub fn new<F>(future: F) -> Arc<Self>
    where
        F: Future<Output = T> + Send + 'static,
    {
        Arc::new(Self {
            future_execution_state: Mutex::new(FutureExecutionState::Running(Box::pin(future))),
            continuation_scheduler: AtomicContinuationScheduler::new(),
        })
    }

    fn poll_future_once(&self, waker: &Waker) -> bool {
        let mut execution_state_guard = self.future_execution_state.lock().unwrap();

        if execution_state_guard.is_finished() {
            return true;
        }

        let FutureExecutionState::Running(pinned_future) = &mut *execution_state_guard else {
            return true;
        };

        let mut poll_context = Context::from_waker(waker);
        match pinned_future.as_mut().poll(&mut poll_context) {
            Poll::Pending => false,
            Poll::Ready(result) => {
                *execution_state_guard = FutureExecutionState::Complete(result);
                true
            }
        }
    }

    pub fn poll(
        self: &Arc<Self>,
        continuation_callback: RustFutureContinuationCallback,
        callback_data: u64,
    ) {
        let is_cancelled = self.continuation_scheduler.is_cancelled();

        let is_ready = is_cancelled || {
            let waker = self.clone().create_waker();
            self.poll_future_once(&waker)
        };

        if is_ready {
            continuation_callback(callback_data, RustFuturePoll::Ready);
        } else {
            self.continuation_scheduler.store_continuation(
                ContinuationCallback(continuation_callback),
                ContinuationData::from_raw(callback_data),
            );
        }
    }

    pub fn complete(&self) -> Option<T> {
        self.future_execution_state.lock().unwrap().take_result()
    }

    pub fn panic_message(&self) -> Option<String> {
        self.future_execution_state
            .lock()
            .unwrap()
            .take_panic_message()
    }

    pub fn cancel(&self) {
        self.continuation_scheduler.mark_cancelled();
    }

    pub fn free(self: Arc<Self>) {
        self.continuation_scheduler.mark_cancelled();
    }

    #[cfg(target_arch = "wasm32")]
    pub fn poll_sync(self: &Arc<Self>) -> WasmPollStatus {
        if self.continuation_scheduler.is_cancelled() {
            return WasmPollStatus::Cancelled;
        }

        let waker = self.clone().create_wasm_waker();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.poll_future_once(&waker)
        }));

        match result {
            Ok(true) => {
                let state = self.future_execution_state.lock().unwrap();
                if state.is_panicked() {
                    WasmPollStatus::Panicked
                } else {
                    WasmPollStatus::Ready
                }
            }
            Ok(false) => WasmPollStatus::Pending,
            Err(panic_payload) => {
                let message = panic_payload_to_string(panic_payload);
                *self.future_execution_state.lock().unwrap() =
                    FutureExecutionState::Panicked(message);
                WasmPollStatus::Panicked
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn create_wasm_waker(self: Arc<Self>) -> Waker {
        let handle = Arc::into_raw(self) as u32;
        let raw_waker = RawWaker::new(handle as *const (), &WASM_WAKER_VTABLE);
        unsafe { Waker::from_raw(raw_waker) }
    }

    fn create_waker(self: Arc<Self>) -> Waker {
        let raw_waker = RawWaker::new(Arc::into_raw(self) as *const (), &RUST_FUTURE_WAKER_VTABLE);
        unsafe { Waker::from_raw(raw_waker) }
    }
}

const RUST_FUTURE_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    waker_clone_fn::<()>,
    waker_wake_fn::<()>,
    waker_wake_by_ref_fn::<()>,
    waker_drop_fn::<()>,
);

#[cfg(target_arch = "wasm32")]
const WASM_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    wasm_waker_clone,
    wasm_waker_wake,
    wasm_waker_wake_by_ref,
    wasm_waker_drop,
);

#[cfg(target_arch = "wasm32")]
fn wasm_waker_clone(data: *const ()) -> RawWaker {
    let handle = data as u32;
    unsafe { Arc::increment_strong_count(handle as *const RustFuture<()>) };
    RawWaker::new(data, &WASM_WAKER_VTABLE)
}

#[cfg(target_arch = "wasm32")]
fn wasm_waker_wake(data: *const ()) {
    let handle = data as u32;
    unsafe { __boltffi_wake(handle) };
    unsafe { Arc::decrement_strong_count(handle as *const RustFuture<()>) };
}

#[cfg(target_arch = "wasm32")]
fn wasm_waker_wake_by_ref(data: *const ()) {
    let handle = data as u32;
    unsafe { __boltffi_wake(handle) };
}

#[cfg(target_arch = "wasm32")]
fn wasm_waker_drop(data: *const ()) {
    let handle = data as u32;
    unsafe { Arc::decrement_strong_count(handle as *const RustFuture<()>) };
}

#[cfg(target_arch = "wasm32")]
fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "unknown panic".to_string()
}

fn waker_clone_fn<T: Send + 'static>(waker_data_ptr: *const ()) -> RawWaker {
    unsafe { Arc::increment_strong_count(waker_data_ptr as *const RustFuture<T>) };
    RawWaker::new(waker_data_ptr, &RUST_FUTURE_WAKER_VTABLE)
}

fn waker_wake_fn<T: Send + 'static>(waker_data_ptr: *const ()) {
    let rust_future_arc = unsafe { Arc::from_raw(waker_data_ptr as *const RustFuture<T>) };
    rust_future_arc.continuation_scheduler.wake_continuation();
}

fn waker_wake_by_ref_fn<T: Send + 'static>(waker_data_ptr: *const ()) {
    let rust_future_ptr = waker_data_ptr as *const RustFuture<T>;
    unsafe {
        (*rust_future_ptr)
            .continuation_scheduler
            .wake_continuation()
    };
}

fn waker_drop_fn<T: Send + 'static>(waker_data_ptr: *const ()) {
    drop(unsafe { Arc::from_raw(waker_data_ptr as *const RustFuture<T>) });
}

pub type RustFutureHandle = *const core::ffi::c_void;

pub fn rust_future_new<F, T>(future: F) -> RustFutureHandle
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    Arc::into_raw(RustFuture::new(future)) as RustFutureHandle
}

pub unsafe fn rust_future_poll<T: Send + 'static>(
    handle: RustFutureHandle,
    continuation_callback: RustFutureContinuationCallback,
    callback_data: u64,
) {
    let rust_future_arc = unsafe { Arc::from_raw(handle as *const RustFuture<T>) };
    rust_future_arc.poll(continuation_callback, callback_data);
    std::mem::forget(rust_future_arc);
}

pub unsafe fn rust_future_complete<T: Send + 'static>(handle: RustFutureHandle) -> Option<T> {
    let rust_future_arc = unsafe { Arc::from_raw(handle as *const RustFuture<T>) };
    let result = rust_future_arc.complete();
    std::mem::forget(rust_future_arc);
    result
}

pub unsafe fn rust_future_cancel<T: Send + 'static>(handle: RustFutureHandle) {
    let rust_future_arc = unsafe { Arc::from_raw(handle as *const RustFuture<T>) };
    rust_future_arc.cancel();
    std::mem::forget(rust_future_arc);
}

pub unsafe fn rust_future_free<T: Send + 'static>(handle: RustFutureHandle) {
    let rust_future_arc = unsafe { Arc::from_raw(handle as *const RustFuture<T>) };
    rust_future_arc.free();
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn rust_future_poll_sync<T: Send + 'static>(handle: RustFutureHandle) -> i32 {
    let rust_future_arc = unsafe { Arc::from_raw(handle as *const RustFuture<T>) };
    let status = rust_future_arc.poll_sync();
    std::mem::forget(rust_future_arc);
    status as i32
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn rust_future_panic_message<T: Send + 'static>(handle: RustFutureHandle) -> Option<String> {
    let rust_future_arc = unsafe { Arc::from_raw(handle as *const RustFuture<T>) };
    let message = rust_future_arc.panic_message();
    std::mem::forget(rust_future_arc);
    message
}
