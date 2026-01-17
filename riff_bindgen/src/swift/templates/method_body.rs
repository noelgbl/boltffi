use askama::Template;
use riff_ffi_rules::naming;

use crate::model::{Class, Method, Module};

use super::super::conversion::{CallbackInfo, ParamsInfo};
use super::super::marshal::ReturnAbi;
use super::super::names::NamingConvention;
use super::super::types::TypeMapper;
use super::MethodContext;

#[derive(Template)]
#[template(path = "swift/method_sync.txt", escape = "none")]
pub struct SyncMethodBodyTemplate {
    pub ffi_name: String,
    pub has_return: bool,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
}

impl SyncMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let ctx = MethodContext::from_method(method, class, module, true);
        Self {
            ffi_name: ctx.ffi_name,
            has_return: method.returns.has_return_value(),
            wrappers_open: ctx.wrappers_open,
            wrappers_close: ctx.wrappers_close,
            ffi_args: ctx.ffi_args,
        }
    }

    pub fn has_wrappers(&self) -> bool {
        !self.wrappers_open.is_empty()
    }
}

#[derive(Template)]
#[template(path = "swift/method_callback.txt", escape = "none")]
pub struct CallbackMethodBodyTemplate {
    pub ffi_name: String,
    pub has_return: bool,
    pub callbacks: Vec<CallbackInfo>,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
    pub callback_args: String,
}

impl CallbackMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let ctx = MethodContext::from_method(method, class, module, true);
        let params_info = ParamsInfo::from_inputs(
            method
                .inputs
                .iter()
                .map(|p| (p.name.as_str(), &p.param_type)),
            &NamingConvention::class_name(&method.name),
        );
        let callback_args = params_info
            .callbacks
            .iter()
            .map(|cb| format!("{}, {}", cb.trampoline_name, cb.ptr_name))
            .collect::<Vec<_>>()
            .join(", ");

        Self {
            ffi_name: ctx.ffi_name,
            has_return: method.returns.has_return_value(),
            callbacks: params_info.callbacks,
            wrappers_open: ctx.wrappers_open,
            wrappers_close: ctx.wrappers_close,
            ffi_args: ctx.ffi_args,
            callback_args,
        }
    }

    pub fn has_wrappers(&self) -> bool {
        !self.wrappers_open.is_empty()
    }
}

#[derive(Template)]
#[template(path = "swift/method_throwing.txt", escape = "none")]
pub struct ThrowingMethodBodyTemplate {
    pub ffi_name: String,
    pub prefix: String,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
    pub decode_expr: String,
}

impl ThrowingMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let ctx = MethodContext::from_method_all_params(method, class, module);
        let return_abi = ReturnAbi::from_return_type(&method.returns, module);
        Self {
            ffi_name: ctx.ffi_name,
            prefix: naming::ffi_prefix().to_string(),
            wrappers_open: ctx.wrappers_open,
            wrappers_close: ctx.wrappers_close,
            ffi_args: ctx.ffi_args,
            decode_expr: return_abi.decode_expr().to_string(),
        }
    }

    pub fn has_wrappers(&self) -> bool {
        !self.wrappers_open.is_empty()
    }
}

#[derive(Template)]
#[template(path = "swift/method_async.txt", escape = "none")]
pub struct AsyncMethodBodyTemplate {
    pub ffi_name: String,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub prefix: String,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
    pub return_type: String,
    pub decode_expr: String,
}

impl AsyncMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let ctx = MethodContext::from_method(method, class, module, true);
        let return_abi = ReturnAbi::from_return_type(&method.returns, module);
        Self {
            ffi_name: ctx.ffi_name,
            ffi_poll: naming::method_ffi_poll(&class.name, &method.name),
            ffi_complete: naming::method_ffi_complete(&class.name, &method.name),
            ffi_cancel: naming::method_ffi_cancel(&class.name, &method.name),
            ffi_free: naming::method_ffi_free(&class.name, &method.name),
            prefix: naming::ffi_prefix().to_string(),
            wrappers_open: ctx.wrappers_open,
            wrappers_close: ctx.wrappers_close,
            ffi_args: ctx.ffi_args,
            return_type: method
                .returns
                .ok_type()
                .map(TypeMapper::map_type)
                .unwrap_or_else(|| "Void".into()),
            decode_expr: return_abi.decode_expr().to_string(),
        }
    }

    pub fn has_wrappers(&self) -> bool {
        !self.wrappers_open.is_empty()
    }
}

#[derive(Template)]
#[template(path = "swift/method_async_throwing.txt", escape = "none")]
pub struct AsyncThrowingMethodBodyTemplate {
    pub ffi_name: String,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub prefix: String,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
    pub return_type: String,
    pub decode_expr: String,
}

impl AsyncThrowingMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let ctx = MethodContext::from_method(method, class, module, true);
        let return_abi = ReturnAbi::from_return_type(&method.returns, module);
        Self {
            ffi_name: ctx.ffi_name,
            ffi_poll: naming::method_ffi_poll(&class.name, &method.name),
            ffi_complete: naming::method_ffi_complete(&class.name, &method.name),
            ffi_cancel: naming::method_ffi_cancel(&class.name, &method.name),
            ffi_free: naming::method_ffi_free(&class.name, &method.name),
            prefix: naming::ffi_prefix().to_string(),
            wrappers_open: ctx.wrappers_open,
            wrappers_close: ctx.wrappers_close,
            ffi_args: ctx.ffi_args,
            return_type: method
                .returns
                .ok_type()
                .map(TypeMapper::map_type)
                .unwrap_or_else(|| "Void".into()),
            decode_expr: return_abi.decode_expr().to_string(),
        }
    }

    pub fn has_wrappers(&self) -> bool {
        !self.wrappers_open.is_empty()
    }
}
