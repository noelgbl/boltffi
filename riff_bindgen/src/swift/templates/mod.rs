mod callback_trait;
mod class;
mod enums;
mod function;
mod method_body;
mod preamble;
mod record;
mod stream;

pub use callback_trait::CallbackTraitTemplate;
pub use class::ClassTemplate;
pub use enums::{CStyleEnumTemplate, DataEnumTemplate};
pub use function::FunctionTemplate;
pub use method_body::{
    AsyncMethodBodyTemplate, AsyncThrowingMethodBodyTemplate, CallbackMethodBodyTemplate,
    SyncMethodBodyTemplate, ThrowingMethodBodyTemplate,
};
pub use preamble::PreambleTemplate;
pub use record::RecordTemplate;
pub use stream::{
    StreamAsyncBodyTemplate, StreamBatchBodyTemplate, StreamCallbackBodyTemplate,
    StreamCancellableTemplate, StreamSubscriptionTemplate,
};

use riff_ffi_rules::naming;

use crate::model::{Class, Method, Module};

use super::marshal::SyncCallBuilder;

pub(crate) struct MethodContext {
    pub ffi_name: String,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
}

impl MethodContext {
    pub fn from_method(method: &Method, class: &Class, module: &Module, include_handle: bool) -> Self {
        let call_builder = SyncCallBuilder::new(include_handle).with_params(
            method.non_callback_params().map(|p| (p.name.as_str(), &p.param_type)),
            module,
        );
        Self {
            ffi_name: naming::method_ffi_name(&class.name, &method.name),
            wrappers_open: call_builder.build_wrappers_open(),
            wrappers_close: call_builder.build_wrappers_close(),
            ffi_args: call_builder.build_ffi_args(),
        }
    }

    pub fn from_method_all_params(method: &Method, class: &Class, module: &Module) -> Self {
        let call_builder = SyncCallBuilder::new(false).with_params(
            method.inputs.iter().map(|p| (p.name.as_str(), &p.param_type)),
            module,
        );
        Self {
            ffi_name: naming::method_ffi_name(&class.name, &method.name),
            wrappers_open: call_builder.build_wrappers_open(),
            wrappers_close: call_builder.build_wrappers_close(),
            ffi_args: call_builder.build_ffi_args(),
        }
    }
}
