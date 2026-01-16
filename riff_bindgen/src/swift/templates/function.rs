use askama::Template;
use riff_ffi_rules::naming;

use crate::model::{Function, Module};

use super::super::conversion::{CallbackInfo, ParamInfo, ParamsInfo, ReturnInfo};
use super::super::marshal::{ReturnAbi, SyncCallBuilder};
use super::super::names::NamingConvention;

#[derive(Template)]
#[template(path = "swift/function.txt", escape = "none")]
pub struct FunctionTemplate {
    pub prefix: String,
    pub func_name: String,
    pub ffi_name: String,
    pub params: Vec<ParamInfo>,
    pub return_type: Option<String>,
    pub return_abi: ReturnAbi,
    pub direct_call: String,
    pub is_async: bool,
    pub throws: bool,
    pub has_callbacks: bool,
    pub callbacks: Vec<CallbackInfo>,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_free: String,
    pub ffi_cancel: String,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
    pub callback_args: String,
}

impl FunctionTemplate {
    pub fn from_function(function: &Function, module: &Module) -> Self {
        let ret = ReturnInfo::from_return_type(&function.returns);
        let func_name_pascal = NamingConvention::class_name(&function.name);
        let params_info = ParamsInfo::from_inputs(
            function
                .inputs
                .iter()
                .map(|p| (p.name.as_str(), &p.param_type)),
            &func_name_pascal,
        );

        let ffi_name = naming::function_ffi_name(&function.name);
        let call_builder = SyncCallBuilder::new(false).with_params(
            function
                .non_callback_params()
                .map(|p| (p.name.as_str(), &p.param_type)),
            module,
        );

        let callback_args = params_info
            .callbacks
            .iter()
            .map(|cb| format!("{}, {}", cb.trampoline_name, cb.ptr_name))
            .collect::<Vec<_>>()
            .join(", ");

        let return_type = if ret.is_void {
            None
        } else if ret.is_result {
            ret.result_ok_type.clone()
        } else {
            ret.swift_type.clone()
        };

        let return_abi = ReturnAbi::from_return_type(&function.returns, module);
        let direct_call = return_abi.direct_call_expr(&format!("{}({})", ffi_name, call_builder.build_ffi_args()));

        Self {
            prefix: naming::ffi_prefix().to_string(),
            func_name: NamingConvention::method_name(&function.name),
            ffi_name,
            params: params_info.params,
            return_type,
            return_abi,
            direct_call,
            is_async: function.is_async,
            throws: function.throws() || ret.is_result,
            has_callbacks: params_info.has_callbacks,
            callbacks: params_info.callbacks,
            ffi_poll: naming::function_ffi_poll(&function.name),
            ffi_complete: naming::function_ffi_complete(&function.name),
            ffi_free: naming::function_ffi_free(&function.name),
            ffi_cancel: naming::function_ffi_cancel(&function.name),
            wrappers_open: call_builder.build_wrappers_open(),
            wrappers_close: call_builder.build_wrappers_close(),
            ffi_args: call_builder.build_ffi_args(),
            callback_args,
        }
    }

    pub fn has_wrappers(&self) -> bool {
        !self.wrappers_open.is_empty()
    }
}
