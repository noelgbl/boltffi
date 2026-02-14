use crate::wire::{WireBuffer, WireEncode};
use core::mem::ManuallyDrop;

#[repr(C)]
pub struct FfiBuf<T> {
    ptr: *mut T,
    len: usize,
    cap: usize,
}

impl<T> FfiBuf<T> {
    pub const fn empty() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            len: 0,
            cap: 0,
        }
    }

    pub fn from_vec(vec: Vec<T>) -> Self {
        let mut vec = ManuallyDrop::new(vec);
        Self {
            ptr: vec.as_mut_ptr(),
            len: vec.len(),
            cap: vec.capacity(),
        }
    }

    pub fn into_vec(self) -> Vec<T> {
        if self.ptr.is_null() {
            return Vec::new();
        }
        let vec = unsafe { Vec::from_raw_parts(self.ptr, self.len, self.cap) };
        core::mem::forget(self);
        vec
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr
    }

    pub unsafe fn as_slice(&self) -> &[T] {
        if self.ptr.is_null() || self.len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
        }
    }
}

impl<T> Drop for FfiBuf<T> {
    fn drop(&mut self) {
        if !self.ptr.is_null() && self.cap > 0 {
            unsafe {
                let _ = Vec::from_raw_parts(self.ptr, self.len, self.cap);
            }
        }
    }
}

impl<T> From<Vec<T>> for FfiBuf<T> {
    fn from(vec: Vec<T>) -> Self {
        Self::from_vec(vec)
    }
}

impl<T> Default for FfiBuf<T> {
    fn default() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            len: 0,
            cap: 0,
        }
    }
}

impl FfiBuf<u8> {
    pub fn wire_encode<V: WireEncode>(value: &V) -> Self {
        Self::from_vec(WireBuffer::new(value).into_bytes())
    }

    pub fn from_raw_vec<T>(vec: Vec<T>) -> Self {
        let mut vec = vec;
        vec.shrink_to_fit();
        let ptr = vec.as_mut_ptr() as *mut u8;
        let byte_len = vec.len() * core::mem::size_of::<T>();
        let byte_cap = vec.capacity() * core::mem::size_of::<T>();
        core::mem::forget(vec);
        Self {
            ptr,
            len: byte_len,
            cap: byte_cap,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn into_packed(self) -> u64 {
        let ptr = self.ptr;
        let len = self.len;
        core::mem::forget(self);
        if len == 0 {
            return 0;
        }
        ((len as u64) << 32) | (ptr as u64)
    }
}

#[macro_export]
macro_rules! define_ffi_buf_free {
    ($($ty:ty => $name:ident),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub extern "C" fn $name(buf: $crate::FfiBuf<$ty>) {
                drop(buf);
            }
        )*
    };
}

define_ffi_buf_free! {
    i8 => boltffi_free_buf_i8,
    i16 => boltffi_free_buf_i16,
    i32 => boltffi_free_buf_i32,
    i64 => boltffi_free_buf_i64,
    u8 => boltffi_free_buf_u8,
    u16 => boltffi_free_buf_u16,
    u32 => boltffi_free_buf_u32,
    u64 => boltffi_free_buf_u64,
    f32 => boltffi_free_buf_f32,
    f64 => boltffi_free_buf_f64,
    crate::FfiString => boltffi_free_buf_FfiString,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buf_roundtrip() {
        let data = vec![1u32, 2, 3, 4, 5];
        let ffi_buf = FfiBuf::from_vec(data.clone());
        assert_eq!(ffi_buf.len(), 5);
        let recovered = ffi_buf.into_vec();
        assert_eq!(recovered, data);
    }

    #[test]
    fn buf_drop() {
        let data = vec![1u8, 2, 3];
        let ffi_buf = FfiBuf::from_vec(data);
        drop(ffi_buf);
    }
}
