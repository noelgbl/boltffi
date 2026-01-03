use super::FfiString;

#[repr(C)]
#[derive(Default)]
pub struct FfiError {
    pub message: FfiString,
}

impl FfiError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: FfiString::from(message.into()),
        }
    }
}
