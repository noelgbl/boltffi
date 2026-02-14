use std::collections::{HashMap, HashSet};

use boltffi_ffi_rules::naming;

use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiContract, AbiParam, AbiStream, AsyncCall, CallMode,
    ErrorTransport,
};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{
    CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef, EnumRepr,
    FunctionDef, MethodDef, ParamDef, ParamPassing, Receiver, ReturnDef, StreamDef,
};
use crate::ir::ids::{CallbackId, EnumId, ParamName, RecordId};
use crate::ir::ops::SizeExpr;
use crate::ir::plan::AbiType;
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::ir::{AsyncOutputAbi, SyncInputAbi, SyncOutputAbi};
use crate::render::kotlin::{NamingConvention, primitives};

use super::plan::{
    JniArrayReleaseMode, JniAsyncCallbackInvoker, JniAsyncCallbackMethod, JniAsyncCompleteKind,
    JniAsyncFunction, JniCallbackCParam, JniCallbackMethod, JniCallbackReturn, JniCallbackTrait,
    JniClass, JniClosureRecordParam, JniClosureTrampoline, JniFunction, JniInvokerResult,
    JniModule, JniOptionInnerKind, JniOptionView, JniParam, JniParamKind, JniResultVariant,
    JniResultView, JniReturnAbi, JniReturnKind, JniStream, JniWireCtor, JniWireFunction,
    JniWireMethod,
};

pub struct JniLowerer<'a> {
    contract: &'a FfiContract,
    abi: &'a AbiContract,
    package: String,
    class_name: String,
}

impl<'a> JniLowerer<'a> {
    pub fn new(
        contract: &'a FfiContract,
        abi: &'a AbiContract,
        package: String,
        class_name: String,
    ) -> Self {
        Self {
            contract,
            abi,
            package,
            class_name,
        }
    }

    pub fn lower(&self) -> JniModule {
        let prefix = naming::ffi_prefix().to_string();
        let jni_prefix = self.jni_prefix();
        let package_path = self.package.replace('.', "/");
        let module_name = self.contract.package.name.clone();
        let used_callbacks = self.collect_used_callbacks();

        let functions = self
            .contract
            .functions
            .iter()
            .filter(|func| !func.is_async && self.is_primitive_only(func))
            .map(|func| self.lower_function(func, &prefix, &jni_prefix))
            .collect();

        let wire_functions = self
            .contract
            .functions
            .iter()
            .filter(|func| !func.is_async && !self.is_primitive_only(func))
            .map(|func| self.lower_wire_function(func, &jni_prefix))
            .collect();

        let async_functions: Vec<JniAsyncFunction> = self
            .contract
            .functions
            .iter()
            .filter(|func| func.is_async && self.is_supported_async_function(func))
            .map(|func| self.lower_async_function(func, &jni_prefix))
            .collect();

        let classes: Vec<JniClass> = self
            .contract
            .catalog
            .all_classes()
            .map(|class| self.lower_class(class, &jni_prefix, &prefix))
            .collect();

        let callback_index = self
            .abi
            .callbacks
            .iter()
            .map(|callback| (callback.callback_id.clone(), callback))
            .collect::<HashMap<_, _>>();

        let callback_traits: Vec<JniCallbackTrait> = self
            .contract
            .catalog
            .all_callbacks()
            .filter(|callback| !matches!(callback.kind, CallbackKind::Closure))
            .filter(|callback| !callback.methods.is_empty())
            .filter_map(|callback| {
                callback_index.get(&callback.id).map(|abi_callback| {
                    self.lower_callback_trait(callback, abi_callback, &package_path, &jni_prefix)
                })
            })
            .collect();

        let has_async_callbacks = callback_traits
            .iter()
            .any(|callback| !callback.async_methods.is_empty());

        let async_callback_invokers = self.collect_async_invokers(&callback_traits, &jni_prefix);

        let closure_trampolines = self.collect_closure_trampolines(&package_path, &used_callbacks);

        let has_async = !async_functions.is_empty()
            || classes.iter().any(|class| !class.async_methods.is_empty())
            || classes.iter().any(|class| !class.streams.is_empty())
            || !callback_traits.is_empty();

        JniModule {
            prefix,
            jni_prefix,
            package_path,
            module_name,
            class_name: self.class_name.clone(),
            has_async,
            has_async_callbacks,
            functions,
            wire_functions,
            async_functions,
            classes,
            callback_traits,
            async_callback_invokers,
            closure_trampolines,
        }
    }

    fn jni_prefix(&self) -> String {
        self.package
            .replace('_', "_1")
            .replace('.', "_")
            .replace('-', "_1")
    }

    fn collect_used_callbacks(&self) -> HashSet<CallbackId> {
        let mut used = HashSet::new();

        self.abi
            .calls
            .iter()
            .for_each(|call| self.collect_used_from_call(call, &mut used));

        used
    }

    fn collect_used_from_call(&self, call: &AbiCall, used: &mut HashSet<CallbackId>) {
        call.params
            .iter()
            .for_each(|param| self.collect_used_from_param(param, used));
        self.collect_used_from_return(&call.output_shape, used);
        self.collect_used_from_error(&call.error, used);
        match &call.mode {
            CallMode::Sync => {}
            CallMode::Async(async_call) => self.collect_used_from_async(async_call.as_ref(), used),
        }
    }

    fn collect_used_from_param(&self, param: &AbiParam, used: &mut HashSet<CallbackId>) {
        if let Some(SyncInputAbi::CallbackHandle { callback_id, .. }) =
            SyncInputAbi::from_abi_param(param)
        {
            used.insert(callback_id);
        }
    }

    fn collect_used_from_return(
        &self,
        output_shape: &crate::ir::abi::OutputShape,
        used: &mut HashSet<CallbackId>,
    ) {
        if let SyncOutputAbi::CallbackHandle { callback_id, .. } =
            SyncOutputAbi::from_output_shape(output_shape)
        {
            used.insert(callback_id);
        }
    }

    fn collect_used_from_async(&self, async_call: &AsyncCall, used: &mut HashSet<CallbackId>) {
        if let AsyncOutputAbi::CallbackHandle { callback_id, .. } =
            AsyncOutputAbi::from_result_shape(&async_call.result_shape)
        {
            used.insert(callback_id);
        }
        self.collect_used_from_error(&async_call.error, used);
    }

    fn collect_used_from_error(&self, _error: &ErrorTransport, _used: &mut HashSet<CallbackId>) {}

    fn is_primitive_only(&self, func: &FunctionDef) -> bool {
        let returns_ok = matches!(
            &func.returns,
            ReturnDef::Void | ReturnDef::Value(TypeExpr::Void | TypeExpr::Primitive(_))
        );

        let params_ok = func
            .params
            .iter()
            .all(|param| matches!(param.type_expr, TypeExpr::Primitive(_)));

        returns_ok && params_ok
    }

    fn is_supported_async_function(&self, func: &FunctionDef) -> bool {
        let output_ok = match &func.returns {
            ReturnDef::Void => true,
            ReturnDef::Value(ty) => self.is_supported_async_type(ty),
            ReturnDef::Result { ok, .. } => self.is_supported_async_type(ok),
        };

        let inputs_ok = func
            .params
            .iter()
            .all(|param| self.is_supported_async_param(&param.type_expr));

        output_ok && inputs_ok
    }

    fn is_supported_async_param(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Primitive(_) | TypeExpr::String => true,
            TypeExpr::Vec(inner) => matches!(inner.as_ref(), TypeExpr::Primitive(_)),
            TypeExpr::Record(id) => self.is_record_blittable(id),
            _ => false,
        }
    }

    fn is_supported_async_type(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Void | TypeExpr::Primitive(_) | TypeExpr::String => true,
            TypeExpr::Vec(inner) => matches!(inner.as_ref(), TypeExpr::Primitive(_)),
            TypeExpr::Record(id) => self.is_record_blittable(id),
            _ => false,
        }
    }

    fn is_record_blittable(&self, record_id: &RecordId) -> bool {
        self.contract
            .catalog
            .resolve_record(record_id)
            .map(|record| record.is_blittable())
            .unwrap_or(false)
    }

    fn record_struct_size(&self, record_id: &RecordId) -> usize {
        self.abi
            .records
            .iter()
            .find(|record| record.id == *record_id)
            .and_then(|record| record.size)
            .unwrap_or(0)
    }

    fn lower_function(&self, func: &FunctionDef, prefix: &str, jni_prefix: &str) -> JniFunction {
        let ffi_name = format!("{}_{}", prefix, func.id.as_str());
        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let params: Vec<JniParam> = func
            .params
            .iter()
            .map(|param| self.lower_param(param))
            .collect();

        let return_kind = self.return_kind(&func.returns, func.id.as_str());

        let jni_return = self.return_kind_jni_return(&return_kind);
        let jni_params = self.format_jni_params(&params);

        JniFunction {
            ffi_name,
            jni_name,
            jni_return,
            jni_params,
            return_kind,
            params,
        }
    }

    fn lower_wire_function(&self, func: &FunctionDef, jni_prefix: &str) -> JniWireFunction {
        let ffi_name = naming::function_ffi_name(func.id.as_str()).into_string();
        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let params: Vec<JniParam> = func
            .params
            .iter()
            .map(|param| self.lower_param(param))
            .collect();

        let jni_params = self.format_jni_params(&params);
        let return_abi = self.return_abi(&func.returns);

        JniWireFunction {
            ffi_name,
            jni_name,
            jni_params,
            params,
            return_abi,
        }
    }

    fn lower_class(&self, class: &ClassDef, jni_prefix: &str, _prefix: &str) -> JniClass {
        let ffi_prefix = naming::class_ffi_prefix(class.id.as_str()).into_string();

        let ctors = class
            .constructors
            .iter()
            .filter(|ctor| self.constructor_supported(ctor))
            .map(|ctor| self.lower_ctor(class, ctor, jni_prefix))
            .collect();

        let wire_methods = class
            .methods
            .iter()
            .filter(|method| !method.is_async && self.method_supported(method))
            .map(|method| self.lower_method(class, method, jni_prefix))
            .collect();

        let async_methods = class
            .methods
            .iter()
            .filter(|method| self.is_supported_async_method(method))
            .map(|method| self.lower_async_method(class, method, jni_prefix))
            .collect();
        let streams = class
            .streams
            .iter()
            .map(|stream| self.lower_stream(class, stream, jni_prefix))
            .collect();

        JniClass {
            ffi_prefix: ffi_prefix.clone(),
            jni_ffi_prefix: ffi_prefix.replace('_', "_1"),
            jni_prefix: jni_prefix.to_string(),
            ctors,
            wire_methods,
            async_methods,
            streams,
        }
    }

    fn lower_stream(&self, class: &ClassDef, stream: &StreamDef, jni_prefix: &str) -> JniStream {
        let abi_stream = self.abi_stream(class, stream);
        let subscribe_ffi = abi_stream.subscribe.as_str().to_string();
        let poll_ffi = abi_stream.poll.as_str().to_string();
        let pop_batch_ffi = abi_stream.pop_batch.as_str().to_string();
        let wait_ffi = abi_stream.wait.as_str().to_string();
        let unsubscribe_ffi = abi_stream.unsubscribe.as_str().to_string();
        let free_ffi = abi_stream.free.as_str().to_string();
        let subscribe_jni = format!(
            "Java_{}_Native_{}",
            jni_prefix,
            subscribe_ffi.replace('_', "_1")
        );
        let poll_jni = format!("Java_{}_Native_{}", jni_prefix, poll_ffi.replace('_', "_1"));
        let pop_batch_jni = format!(
            "Java_{}_Native_{}",
            jni_prefix,
            pop_batch_ffi.replace('_', "_1")
        );
        let wait_jni = format!("Java_{}_Native_{}", jni_prefix, wait_ffi.replace('_', "_1"));
        let unsubscribe_jni = format!(
            "Java_{}_Native_{}",
            jni_prefix,
            unsubscribe_ffi.replace('_', "_1")
        );
        let free_jni = format!("Java_{}_Native_{}", jni_prefix, free_ffi.replace('_', "_1"));
        JniStream {
            subscribe_ffi,
            subscribe_jni,
            poll_ffi,
            poll_jni,
            pop_batch_ffi,
            pop_batch_jni,
            wait_ffi,
            wait_jni,
            unsubscribe_ffi,
            unsubscribe_jni,
            free_ffi,
            free_jni,
        }
    }

    fn abi_stream<'b>(&'b self, class: &ClassDef, stream: &StreamDef) -> &'b AbiStream {
        self.abi
            .streams
            .iter()
            .find(|item| item.class_id == class.id && item.stream_id == stream.id)
            .expect("abi stream")
    }

    fn constructor_supported(&self, ctor: &ConstructorDef) -> bool {
        let params = ctor.params();
        params.iter().all(|param| self.supports_param(param))
    }

    fn method_supported(&self, method: &MethodDef) -> bool {
        let params_ok = method.params.iter().all(|param| self.supports_param(param));

        params_ok && self.supports_return_type(&method.returns)
    }

    fn supports_param(&self, param: &ParamDef) -> bool {
        match &param.type_expr {
            TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Enum(_) => true,
            TypeExpr::Handle(_) | TypeExpr::Callback(_) => true,
            TypeExpr::Record(id) => self.is_record_blittable(id),
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(_) => true,
                TypeExpr::Record(id) => {
                    matches!(param.passing, ParamPassing::Value) && self.is_record_blittable(id)
                }
                _ => false,
            },
            TypeExpr::Option(inner) => self.supports_option_inner(inner),
            _ => false,
        }
    }

    fn supports_param_type(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Enum(_) => true,
            TypeExpr::Record(id) => self.is_record_blittable(id),
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(_) => true,
                TypeExpr::Record(id) => self.is_record_blittable(id),
                _ => false,
            },
            TypeExpr::Option(inner) => self.supports_option_inner(inner),
            _ => false,
        }
    }

    fn supports_option_inner(&self, inner: &TypeExpr) -> bool {
        match inner {
            TypeExpr::Primitive(_)
            | TypeExpr::String
            | TypeExpr::Enum(_)
            | TypeExpr::Handle(_)
            | TypeExpr::Callback(_) => true,
            TypeExpr::Record(id) => self.is_record_blittable(id),
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(_) => true,
                TypeExpr::Record(id) => self.is_record_blittable(id),
                _ => false,
            },
            _ => false,
        }
    }

    fn supports_return_type(&self, returns: &ReturnDef) -> bool {
        match returns {
            ReturnDef::Void => true,
            ReturnDef::Value(ty) => self.supports_param_type(ty),
            ReturnDef::Result { ok, .. } => self.supports_option_inner(ok),
        }
    }

    fn is_supported_async_method(&self, method: &MethodDef) -> bool {
        method.is_async
            && method
                .params
                .iter()
                .all(|param| self.is_supported_async_param(&param.type_expr))
            && match &method.returns {
                ReturnDef::Void => true,
                ReturnDef::Value(ty) => self.is_supported_async_type(ty),
                ReturnDef::Result { ok, .. } => self.is_supported_async_type(ok),
            }
    }

    fn lower_method(
        &self,
        class: &ClassDef,
        method: &MethodDef,
        jni_prefix: &str,
    ) -> JniWireMethod {
        let ffi_name = naming::method_ffi_name(class.id.as_str(), method.id.as_str()).into_string();
        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let params: Vec<JniParam> = method
            .params
            .iter()
            .map(|param| self.lower_param(param))
            .collect();

        let jni_params = self.format_jni_params(&params);
        let return_abi = self.return_abi(&method.returns);

        JniWireMethod {
            ffi_name,
            jni_name,
            jni_params,
            params,
            return_abi,
            include_handle: !matches!(method.receiver, Receiver::Static),
        }
    }

    fn lower_ctor(&self, class: &ClassDef, ctor: &ConstructorDef, jni_prefix: &str) -> JniWireCtor {
        let ffi_prefix = naming::class_ffi_prefix(class.id.as_str());
        let ffi_name = match ctor.name() {
            None => format!("{}_new", ffi_prefix),
            Some(name) => naming::method_ffi_name(class.id.as_str(), name.as_str()).into_string(),
        };

        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let params: Vec<JniParam> = ctor
            .params()
            .into_iter()
            .map(|param| self.lower_param(param))
            .collect();

        let jni_params = self.format_jni_params(&params);

        JniWireCtor {
            ffi_name,
            jni_name,
            jni_params,
            params,
        }
    }

    fn lower_async_function(&self, func: &FunctionDef, jni_prefix: &str) -> JniAsyncFunction {
        let ffi_name = naming::function_ffi_name(func.id.as_str()).into_string();
        let jni_func_name = ffi_name.replace('_', "_1");

        let params: Vec<JniParam> = func
            .params
            .iter()
            .map(|param| self.lower_param(param))
            .collect();

        let jni_params = self.format_jni_params(&params);

        let return_abi = self.return_abi(&func.returns);
        let complete_kind = self.async_complete_kind(&return_abi);

        JniAsyncFunction {
            ffi_name: ffi_name.clone(),
            ffi_poll: naming::function_ffi_poll(func.id.as_str()).into_string(),
            ffi_complete: naming::function_ffi_complete(func.id.as_str()).into_string(),
            ffi_cancel: naming::function_ffi_cancel(func.id.as_str()).into_string(),
            ffi_free: naming::function_ffi_free(func.id.as_str()).into_string(),
            jni_create_name: format!("Java_{}_Native_{}", jni_prefix, jni_func_name),
            jni_poll_name: format!("Java_{}_Native_{}_1poll", jni_prefix, jni_func_name),
            jni_complete_name: format!("Java_{}_Native_{}_1complete", jni_prefix, jni_func_name),
            jni_cancel_name: format!("Java_{}_Native_{}_1cancel", jni_prefix, jni_func_name),
            jni_free_name: format!("Java_{}_Native_{}_1free", jni_prefix, jni_func_name),
            jni_params,
            complete_kind,
            params,
        }
    }

    fn lower_async_method(
        &self,
        class: &ClassDef,
        method: &MethodDef,
        jni_prefix: &str,
    ) -> JniAsyncFunction {
        let ffi_name = naming::method_ffi_name(class.id.as_str(), method.id.as_str()).into_string();
        let jni_func_name = ffi_name.replace('_', "_1");

        let params: Vec<JniParam> = method
            .params
            .iter()
            .map(|param| self.lower_param(param))
            .collect();

        let jni_params = self.format_jni_params(&params);

        let return_abi = self.return_abi(&method.returns);
        let complete_kind = self.async_complete_kind(&return_abi);

        JniAsyncFunction {
            ffi_name: ffi_name.clone(),
            ffi_poll: naming::method_ffi_poll(class.id.as_str(), method.id.as_str()).into_string(),
            ffi_complete: naming::method_ffi_complete(class.id.as_str(), method.id.as_str())
                .into_string(),
            ffi_cancel: naming::method_ffi_cancel(class.id.as_str(), method.id.as_str())
                .into_string(),
            ffi_free: naming::method_ffi_free(class.id.as_str(), method.id.as_str()).into_string(),
            jni_create_name: format!("Java_{}_Native_{}", jni_prefix, jni_func_name),
            jni_poll_name: format!("Java_{}_Native_{}_1poll", jni_prefix, jni_func_name),
            jni_complete_name: format!("Java_{}_Native_{}_1complete", jni_prefix, jni_func_name),
            jni_cancel_name: format!("Java_{}_Native_{}_1cancel", jni_prefix, jni_func_name),
            jni_free_name: format!("Java_{}_Native_{}_1free", jni_prefix, jni_func_name),
            jni_params,
            complete_kind,
            params,
        }
    }

    fn lower_param(&self, param: &ParamDef) -> JniParam {
        let name = param.name.as_str().to_string();
        let is_string = matches!(param.type_expr, TypeExpr::String);
        let (array_primitive, array_is_mutable) = match &param.type_expr {
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(p) => (Some(*p), matches!(param.passing, ParamPassing::RefMut)),
                _ => (None, false),
            },
            _ => (None, false),
        };

        let record_info = self.record_param_info(&param.type_expr);
        let data_enum_info = self.data_enum_param_info(&param.type_expr);

        let is_wire_param = self.needs_wire_encoding(&param.type_expr);
        let is_closure = matches!(
            param.type_expr,
            TypeExpr::Callback(ref callback_id) if self.is_closure_callback(callback_id)
        );
        let jni_type = self.param_jni_type(
            &param.type_expr,
            is_wire_param,
            data_enum_info.is_some(),
            array_primitive.is_some(),
            is_closure,
        );

        let jni_decl = format!("{} {}", jni_type, name);

        let ffi_arg = self.param_ffi_arg(
            &name,
            &param.type_expr,
            array_primitive,
            array_is_mutable,
            is_wire_param,
            record_info.clone(),
            data_enum_info.clone(),
        );

        let kind = if is_closure {
            JniParamKind::Closure
        } else if is_string {
            JniParamKind::String
        } else if let Some(primitive) = array_primitive {
            let c_type = self.primitive_c_type(primitive);
            let release_mode = if array_is_mutable {
                JniArrayReleaseMode::Commit
            } else {
                JniArrayReleaseMode::Abort
            };
            JniParamKind::PrimitiveArray {
                c_type,
                release_mode,
            }
        } else if is_wire_param || data_enum_info.is_some() {
            JniParamKind::Buffer
        } else {
            JniParamKind::Primitive
        };

        JniParam {
            name,
            ffi_arg,
            jni_decl,
            kind,
        }
    }

    fn param_jni_type(
        &self,
        ty: &TypeExpr,
        is_wire_param: bool,
        is_data_enum: bool,
        is_array: bool,
        is_closure: bool,
    ) -> String {
        if is_closure {
            return "jlong".to_string();
        }

        if is_data_enum || is_wire_param {
            return "jobject".to_string();
        }

        if is_array {
            return self.array_jni_type(ty).to_string();
        }

        match ty {
            TypeExpr::Primitive(p) => self.primitive_jni_type(*p).to_string(),
            TypeExpr::String => "jstring".to_string(),
            TypeExpr::Bytes => "jbyteArray".to_string(),
            TypeExpr::Handle(_) | TypeExpr::Callback(_) => "jlong".to_string(),
            TypeExpr::Enum(_) => "jint".to_string(),
            _ => "jlong".to_string(),
        }
    }

    fn array_jni_type(&self, ty: &TypeExpr) -> &str {
        match ty {
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(PrimitiveType::I8 | PrimitiveType::U8) => "jbyteArray",
                TypeExpr::Primitive(PrimitiveType::I16 | PrimitiveType::U16) => "jshortArray",
                TypeExpr::Primitive(PrimitiveType::I32 | PrimitiveType::U32) => "jintArray",
                TypeExpr::Primitive(
                    PrimitiveType::I64
                    | PrimitiveType::U64
                    | PrimitiveType::ISize
                    | PrimitiveType::USize,
                ) => "jlongArray",
                TypeExpr::Primitive(PrimitiveType::F32) => "jfloatArray",
                TypeExpr::Primitive(PrimitiveType::F64) => "jdoubleArray",
                TypeExpr::Primitive(PrimitiveType::Bool) => "jbooleanArray",
                _ => "jobject",
            },
            TypeExpr::Bytes => "jbyteArray",
            _ => "jobject",
        }
    }

    fn primitive_c_type(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::I8 => "int8_t".to_string(),
            PrimitiveType::U8 => "uint8_t".to_string(),
            PrimitiveType::I16 => "int16_t".to_string(),
            PrimitiveType::U16 => "uint16_t".to_string(),
            PrimitiveType::I32 => "int32_t".to_string(),
            PrimitiveType::U32 => "uint32_t".to_string(),
            PrimitiveType::I64 | PrimitiveType::ISize => "int64_t".to_string(),
            PrimitiveType::U64 | PrimitiveType::USize => "uint64_t".to_string(),
            PrimitiveType::F32 => "float".to_string(),
            PrimitiveType::F64 => "double".to_string(),
        }
    }

    fn primitive_jni_type(&self, primitive: PrimitiveType) -> &'static str {
        let model_primitive = self.to_model_primitive(primitive);
        primitives::info(model_primitive).jni_type
    }

    fn primitive_signature(&self, primitive: PrimitiveType) -> String {
        let model_primitive = self.to_model_primitive(primitive);
        primitives::info(model_primitive).signature.to_string()
    }

    fn to_model_primitive(&self, primitive: PrimitiveType) -> crate::model::Primitive {
        use crate::model::Primitive as ModelPrimitive;

        match primitive {
            PrimitiveType::Bool => ModelPrimitive::Bool,
            PrimitiveType::I8 => ModelPrimitive::I8,
            PrimitiveType::U8 => ModelPrimitive::U8,
            PrimitiveType::I16 => ModelPrimitive::I16,
            PrimitiveType::U16 => ModelPrimitive::U16,
            PrimitiveType::I32 => ModelPrimitive::I32,
            PrimitiveType::U32 => ModelPrimitive::U32,
            PrimitiveType::I64 => ModelPrimitive::I64,
            PrimitiveType::U64 => ModelPrimitive::U64,
            PrimitiveType::ISize => ModelPrimitive::Isize,
            PrimitiveType::USize => ModelPrimitive::Usize,
            PrimitiveType::F32 => ModelPrimitive::F32,
            PrimitiveType::F64 => ModelPrimitive::F64,
        }
    }

    fn needs_wire_encoding(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Builtin(_)
            | TypeExpr::Record(_)
            | TypeExpr::Enum(_)
            | TypeExpr::Custom(_) => true,
            TypeExpr::Vec(inner) => !matches!(inner.as_ref(), TypeExpr::Primitive(_)),
            TypeExpr::Option(inner) => {
                !matches!(inner.as_ref(), TypeExpr::Handle(_) | TypeExpr::Callback(_))
            }
            _ => false,
        }
    }

    fn record_param_info(&self, ty: &TypeExpr) -> Option<RecordParamInfo> {
        match ty {
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Record(id) => {
                    let struct_size = self.record_struct_size(id);
                    Some(RecordParamInfo {
                        id: id.clone(),
                        struct_size,
                    })
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn data_enum_param_info(&self, ty: &TypeExpr) -> Option<DataEnumParamInfo> {
        match ty {
            TypeExpr::Enum(id) => self
                .contract
                .catalog
                .resolve_enum(id)
                .filter(|enum_def| matches!(enum_def.repr, EnumRepr::Data { .. }))
                .map(|_| DataEnumParamInfo { id: id.clone() }),
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn param_ffi_arg(
        &self,
        name: &str,
        ty: &TypeExpr,
        array_primitive: Option<PrimitiveType>,
        array_is_mutable: bool,
        is_wire_param: bool,
        record_info: Option<RecordParamInfo>,
        data_enum_info: Option<DataEnumParamInfo>,
    ) -> String {
        if matches!(ty, TypeExpr::String) {
            return format!(
                "(const uint8_t*)_{}_c, {} ? strlen(_{}_c) : 0",
                name, name, name
            );
        }

        if record_info.is_some() || data_enum_info.is_some() {
            return format!("(const uint8_t*)_{}_ptr, (uintptr_t)_{}_len", name, name);
        }

        if let Some(primitive) = array_primitive {
            let c_type = self.primitive_c_type(primitive);
            let ptr_type = if array_is_mutable {
                format!("{}*", c_type)
            } else {
                format!("const {}*", c_type)
            };
            return format!("({})_{}_ptr, (uintptr_t)_{}_len", ptr_type, name, name);
        }

        if is_wire_param {
            return format!("(const uint8_t*)_{}_ptr, (uintptr_t)_{}_len", name, name);
        }

        match ty {
            TypeExpr::Handle(_) => format!("(void*){}", name),
            TypeExpr::Callback(callback_id) => {
                if self.is_closure_callback(callback_id) {
                    let trampoline = self.closure_trampoline_name(callback_id);
                    format!("{}, (void*){}", trampoline, name)
                } else {
                    let create_fn = naming::callback_create_fn(callback_id.as_str()).into_string();
                    format!("{}((uint64_t){})", create_fn, name)
                }
            }
            _ => name.to_string(),
        }
    }

    fn is_closure_callback(&self, callback_id: &CallbackId) -> bool {
        self.contract
            .catalog
            .resolve_callback(callback_id)
            .map(|callback| matches!(callback.kind, CallbackKind::Closure))
            .unwrap_or(false)
    }

    fn closure_trampoline_name(&self, callback_id: &CallbackId) -> String {
        let signature_id = callback_id
            .as_str()
            .strip_prefix("__Closure_")
            .unwrap_or(callback_id.as_str());
        format!("trampoline_{}", signature_id)
    }

    fn format_jni_params(&self, params: &[JniParam]) -> String {
        if params.is_empty() {
            String::new()
        } else {
            let decls = params
                .iter()
                .map(|param| param.jni_param_decl().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(", {}", decls)
        }
    }

    fn return_kind(&self, returns: &ReturnDef, func_name: &str) -> JniReturnKind {
        match returns {
            ReturnDef::Void => JniReturnKind::Void,
            ReturnDef::Result { ok, err } => {
                JniReturnKind::Result(self.result_view(ok, err, func_name))
            }
            ReturnDef::Value(ty) => self.return_kind_from_type(ty, func_name),
        }
    }

    fn return_kind_from_type(&self, ty: &TypeExpr, func_name: &str) -> JniReturnKind {
        match ty {
            TypeExpr::Void => JniReturnKind::Void,
            TypeExpr::Primitive(p) => JniReturnKind::Primitive {
                jni_type: self.primitive_return_jni_type(*p),
            },
            TypeExpr::String => JniReturnKind::String {
                ffi_name: naming::function_ffi_name(func_name).into_string(),
            },
            TypeExpr::Vec(_) => JniReturnKind::Vec {
                len_fn: naming::function_ffi_vec_len(func_name).into_string(),
                copy_fn: naming::function_ffi_vec_copy_into(func_name).into_string(),
            },
            TypeExpr::Enum(id) => self
                .contract
                .catalog
                .resolve_enum(id)
                .filter(|enum_def| matches!(enum_def.repr, EnumRepr::Data { .. }))
                .map(|_| {
                    let struct_size = self.data_enum_struct_size(id);
                    let enum_name = NamingConvention::class_name(id.as_str());
                    JniReturnKind::DataEnum {
                        enum_name,
                        struct_size,
                    }
                })
                .unwrap_or(JniReturnKind::CStyleEnum),
            TypeExpr::Option(inner) => {
                let opt = self.option_view(inner);
                JniReturnKind::Option(opt)
            }
            _ => JniReturnKind::Void,
        }
    }

    fn return_kind_jni_return(&self, kind: &JniReturnKind) -> String {
        match kind {
            JniReturnKind::Void => "void".to_string(),
            JniReturnKind::Primitive { jni_type } => jni_type.clone(),
            JniReturnKind::String { .. } => "jstring".to_string(),
            JniReturnKind::Vec { .. } => "jobject".to_string(),
            JniReturnKind::CStyleEnum => "jint".to_string(),
            JniReturnKind::DataEnum { .. } => "jobject".to_string(),
            JniReturnKind::Option(_) => "jobject".to_string(),
            JniReturnKind::Result(_) => "jobject".to_string(),
        }
    }

    fn return_abi(&self, returns: &ReturnDef) -> JniReturnAbi {
        match returns {
            ReturnDef::Void => JniReturnAbi::Unit,
            ReturnDef::Result { .. } => JniReturnAbi::WireEncoded,
            ReturnDef::Value(ty) => match ty {
                TypeExpr::Void => JniReturnAbi::Unit,
                TypeExpr::Primitive(p) => JniReturnAbi::Direct {
                    jni_return_type: self.primitive_return_jni_type(*p),
                    jni_c_return_type: self.primitive_c_type(*p),
                    jni_result_cast: self.primitive_return_cast(*p),
                },
                TypeExpr::String
                | TypeExpr::Record(_)
                | TypeExpr::Enum(_)
                | TypeExpr::Vec(_)
                | TypeExpr::Option(_)
                | TypeExpr::Bytes => JniReturnAbi::WireEncoded,
                _ => JniReturnAbi::Direct {
                    jni_return_type: "jlong".to_string(),
                    jni_c_return_type: "int64_t".to_string(),
                    jni_result_cast: "".to_string(),
                },
            },
        }
    }

    fn async_complete_kind(&self, return_abi: &JniReturnAbi) -> JniAsyncCompleteKind {
        match return_abi {
            JniReturnAbi::Unit => JniAsyncCompleteKind::Void,
            JniReturnAbi::WireEncoded => JniAsyncCompleteKind::WireEncoded,
            JniReturnAbi::Direct {
                jni_return_type,
                jni_c_return_type,
                ..
            } => JniAsyncCompleteKind::Direct {
                jni_return: jni_return_type.clone(),
                c_type: jni_c_return_type.clone(),
            },
        }
    }

    fn primitive_return_jni_type(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "jboolean".to_string(),
            PrimitiveType::I8 | PrimitiveType::U8 => "jbyte".to_string(),
            PrimitiveType::I16 | PrimitiveType::U16 => "jshort".to_string(),
            PrimitiveType::I32 | PrimitiveType::U32 => "jint".to_string(),
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "jlong".to_string(),
            PrimitiveType::F32 => "jfloat".to_string(),
            PrimitiveType::F64 => "jdouble".to_string(),
        }
    }

    fn primitive_return_cast(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "(jboolean)".to_string(),
            PrimitiveType::U8 => "(jbyte)".to_string(),
            PrimitiveType::U16 => "(jshort)".to_string(),
            PrimitiveType::U32 => "(jint)".to_string(),
            PrimitiveType::U64 | PrimitiveType::USize => "(jlong)".to_string(),
            _ => String::new(),
        }
    }

    fn result_view(&self, ok: &TypeExpr, err: &TypeExpr, func_name: &str) -> JniResultView {
        let len_fn = naming::function_ffi_vec_len(func_name).into_string();
        let copy_fn = naming::function_ffi_vec_copy_into(func_name).into_string();

        let ok_variant = self.result_variant(ok, &len_fn, &copy_fn);
        let err_variant = self.result_variant(err, &len_fn, &copy_fn);

        JniResultView {
            ok: ok_variant,
            err: err_variant,
        }
    }

    fn result_variant(&self, ty: &TypeExpr, len_fn: &str, copy_fn: &str) -> JniResultVariant {
        match ty {
            TypeExpr::Void => JniResultVariant::Void,
            TypeExpr::Primitive(p) => JniResultVariant::Primitive {
                c_type: self.primitive_c_type(*p),
                jni_type: self.primitive_return_jni_type(*p),
            },
            TypeExpr::String => JniResultVariant::String,
            TypeExpr::Record(id) => JniResultVariant::Record {
                c_type: NamingConvention::class_name(id.as_str()),
                jni_type: "jobject".to_string(),
                struct_size: self.record_struct_size(id),
            },
            TypeExpr::Enum(id) => {
                let enum_def = self.contract.catalog.resolve_enum(id);
                let is_data_enum = enum_def
                    .as_ref()
                    .map(|def| matches!(def.repr, EnumRepr::Data { .. }) || def.is_error)
                    .unwrap_or(false);
                if is_data_enum {
                    JniResultVariant::DataEnum {
                        jni_type: "jobject".to_string(),
                        struct_size: self.data_enum_struct_size(id),
                    }
                } else {
                    JniResultVariant::Enum {
                        jni_type: "jint".to_string(),
                    }
                }
            }
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(p) => JniResultVariant::VecPrimitive {
                    info: self.vec_primitive_info(*p),
                    len_fn: len_fn.to_string(),
                    copy_fn: copy_fn.to_string(),
                },
                TypeExpr::Record(id) => JniResultVariant::VecRecord {
                    len_fn: len_fn.to_string(),
                    copy_fn: copy_fn.to_string(),
                    struct_size: self.record_struct_size(id),
                },
                _ => JniResultVariant::Void,
            },
            _ => JniResultVariant::Void,
        }
    }

    fn vec_primitive_info(&self, primitive: PrimitiveType) -> super::plan::JniVecPrimitive {
        let model_primitive = self.to_model_primitive(primitive);
        let info = primitives::info(model_primitive);
        super::plan::JniVecPrimitive {
            c_type_name: info.c_type.to_string(),
            jni_array_type: info.array_type.to_string(),
        }
    }

    fn option_view(&self, inner: &TypeExpr) -> JniOptionView {
        let is_vec = matches!(inner, TypeExpr::Vec(_));
        let is_data_enum = self.is_data_enum(inner);
        let struct_size = match inner {
            TypeExpr::Record(id) => self.record_struct_size(id),
            TypeExpr::Enum(id) if is_data_enum => self.data_enum_struct_size(id),
            TypeExpr::Vec(vec_inner) => match vec_inner.as_ref() {
                TypeExpr::Record(id) => self.record_struct_size(id),
                TypeExpr::Enum(id) if is_data_enum => self.data_enum_struct_size(id),
                _ => 0,
            },
            _ => 0,
        };

        let inner_kind = self.option_inner_kind(inner, is_data_enum);

        JniOptionView {
            ffi_type: self.option_ffi_type(inner, is_vec, is_data_enum),
            struct_size,
            inner_kind,
        }
    }

    fn option_inner_kind(&self, inner: &TypeExpr, is_data_enum: bool) -> JniOptionInnerKind {
        match inner {
            TypeExpr::Primitive(p) => {
                if self.primitive_is_large(*p) {
                    JniOptionInnerKind::PrimitiveLarge
                } else {
                    JniOptionInnerKind::Primitive32
                }
            }
            TypeExpr::String => JniOptionInnerKind::String,
            TypeExpr::Record(_) => JniOptionInnerKind::Record,
            TypeExpr::Enum(_) if is_data_enum => JniOptionInnerKind::Enum,
            TypeExpr::Enum(_) => JniOptionInnerKind::Enum,
            TypeExpr::Vec(vec_inner) => match vec_inner.as_ref() {
                TypeExpr::Primitive(_) => JniOptionInnerKind::VecPrimitive,
                TypeExpr::Record(_) => JniOptionInnerKind::VecRecord,
                TypeExpr::String => JniOptionInnerKind::VecString,
                TypeExpr::Enum(_) => JniOptionInnerKind::VecEnum,
                _ => JniOptionInnerKind::VecPrimitive,
            },
            _ => JniOptionInnerKind::Record,
        }
    }

    fn option_ffi_type(&self, inner: &TypeExpr, is_vec: bool, is_data_enum: bool) -> String {
        if is_vec {
            match inner {
                TypeExpr::Vec(vec_inner) => match vec_inner.as_ref() {
                    TypeExpr::Primitive(p) => format!("FfiOption_{}", self.primitive_c_type(*p)),
                    TypeExpr::Record(id) => {
                        format!("FfiOption_{}", NamingConvention::class_name(id.as_str()))
                    }
                    TypeExpr::String => "FfiOption_FfiString".to_string(),
                    TypeExpr::Enum(id) if is_data_enum => {
                        format!("FfiOption_{}", NamingConvention::class_name(id.as_str()))
                    }
                    TypeExpr::Enum(_) => "FfiOption_int32_t".to_string(),
                    _ => "FfiOption_void".to_string(),
                },
                _ => "FfiOption_void".to_string(),
            }
        } else {
            match inner {
                TypeExpr::Primitive(p) => format!("FfiOption_{}", self.primitive_c_type(*p)),
                TypeExpr::String => "FfiOption_FfiString".to_string(),
                TypeExpr::Record(id) => {
                    format!("FfiOption_{}", NamingConvention::class_name(id.as_str()))
                }
                TypeExpr::Enum(id) if is_data_enum => {
                    format!("FfiOption_{}", NamingConvention::class_name(id.as_str()))
                }
                TypeExpr::Enum(_) => "FfiOption_int32_t".to_string(),
                _ => "FfiOption_void".to_string(),
            }
        }
    }

    fn primitive_is_large(&self, primitive: PrimitiveType) -> bool {
        matches!(
            primitive,
            PrimitiveType::I64
                | PrimitiveType::U64
                | PrimitiveType::ISize
                | PrimitiveType::USize
                | PrimitiveType::F64
        )
    }

    fn is_data_enum(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Enum(id) => self
                .contract
                .catalog
                .resolve_enum(id)
                .map(|enum_def| matches!(enum_def.repr, EnumRepr::Data { .. }))
                .unwrap_or(false),
            _ => false,
        }
    }

    fn fixed_size(&self, size: &SizeExpr) -> Option<usize> {
        match size {
            SizeExpr::Fixed(value) => Some(*value),
            SizeExpr::Sum(parts) => parts
                .iter()
                .map(|part| self.fixed_size(part))
                .collect::<Option<Vec<_>>>()
                .map(|sizes| sizes.into_iter().sum()),
            _ => None,
        }
    }

    fn data_enum_struct_size(&self, enum_id: &EnumId) -> usize {
        self.abi
            .enums
            .iter()
            .find(|enum_def| enum_def.id == *enum_id)
            .and_then(|enum_def| self.fixed_size(&enum_def.encode_ops.size))
            .unwrap_or(0)
    }

    fn lower_callback_trait(
        &self,
        callback: &CallbackTraitDef,
        abi_callback: &AbiCallbackInvocation,
        package_path: &str,
        jni_prefix: &str,
    ) -> JniCallbackTrait {
        let trait_name = NamingConvention::class_name(callback.id.as_str());
        let callbacks_class = format!("{}Callbacks", trait_name);

        let sync_methods = callback
            .methods
            .iter()
            .filter(|method| !method.is_async)
            .filter(|method| self.is_supported_callback_method(method))
            .map(|method| self.lower_sync_callback_method(method))
            .collect();

        let async_methods = callback
            .methods
            .iter()
            .filter(|method| method.is_async)
            .filter(|method| self.is_supported_callback_method(method))
            .map(|method| self.lower_async_callback_method(method, jni_prefix))
            .collect();

        JniCallbackTrait {
            trait_name,
            vtable_type: abi_callback.vtable_type.as_str().to_string(),
            register_fn: abi_callback.register_fn.as_str().to_string(),
            callbacks_class: format!("{}/{}", package_path, callbacks_class),
            sync_methods,
            async_methods,
        }
    }

    fn is_supported_callback_method(&self, method: &CallbackMethodDef) -> bool {
        let supported_return = match &method.returns {
            ReturnDef::Void => true,
            ReturnDef::Value(TypeExpr::Void) => true,
            ReturnDef::Value(TypeExpr::Primitive(_)) => true,
            ReturnDef::Result { ok, .. } => {
                matches!(ok, TypeExpr::Void | TypeExpr::Primitive(_))
            }
            _ => false,
        };

        let supported_params = method
            .params
            .iter()
            .all(|param| self.is_supported_callback_param(&param.type_expr));

        supported_return && supported_params
    }

    fn is_supported_callback_param(&self, ty: &TypeExpr) -> bool {
        matches!(
            ty,
            TypeExpr::Primitive(_)
                | TypeExpr::String
                | TypeExpr::Bytes
                | TypeExpr::Record(_)
                | TypeExpr::Enum(_)
                | TypeExpr::Vec(_)
                | TypeExpr::Option(_)
                | TypeExpr::Result { .. }
        )
    }

    fn lower_sync_callback_method(&self, method: &CallbackMethodDef) -> JniCallbackMethod {
        let return_info = if matches!(method.returns, ReturnDef::Void) {
            None
        } else {
            let return_type = self.callback_return_type(&method.returns);
            Some(JniCallbackReturn {
                jni_type: self.jni_call_return_type(return_type),
                jni_call_type: self.jni_call_method_suffix(return_type),
                c_type: self.c_type_for_callback(return_type),
            })
        };

        let lowered_params: Vec<LoweredCallbackParam> = method
            .params
            .iter()
            .map(|param| self.lower_callback_param(&param.name, &param.type_expr, false))
            .collect();

        let c_params = lowered_params
            .iter()
            .flat_map(|param| param.c_params.iter().cloned())
            .collect();

        let setup_lines = lowered_params
            .iter()
            .flat_map(|param| param.setup_lines.iter().cloned())
            .collect();

        let jni_args = lowered_params
            .iter()
            .map(|param| param.jni_arg.clone())
            .collect();

        let ffi_name = naming::vtable_field_name(method.id.as_str()).into_string();

        JniCallbackMethod {
            ffi_name: ffi_name.clone(),
            jni_method_name: ffi_name,
            jni_signature: self.build_callback_jni_signature(&method.params, &method.returns),
            c_params,
            setup_lines,
            jni_args,
            return_info,
        }
    }

    fn lower_async_callback_method(
        &self,
        method: &CallbackMethodDef,
        jni_prefix: &str,
    ) -> JniAsyncCallbackMethod {
        let invoker_suffix = self.async_invoker_suffix(&method.returns);

        let return_c_type = if matches!(method.returns, ReturnDef::Void) {
            None
        } else if invoker_suffix == "Wire" {
            Some("wire".to_string())
        } else {
            let return_type = self.callback_return_type(&method.returns);
            Some(self.c_type_for_callback(return_type))
        };

        let lowered_params: Vec<LoweredCallbackParam> = method
            .params
            .iter()
            .map(|param| self.lower_callback_param(&param.name, &param.type_expr, true))
            .collect();

        let c_params = lowered_params
            .iter()
            .flat_map(|param| param.c_params.iter().cloned())
            .collect();

        let setup_lines = lowered_params
            .iter()
            .flat_map(|param| param.setup_lines.iter().cloned())
            .collect();

        let jni_args = lowered_params
            .iter()
            .map(|param| param.jni_arg.clone())
            .collect();
        let ffi_name = naming::vtable_field_name(method.id.as_str()).into_string();

        JniAsyncCallbackMethod {
            ffi_name: ffi_name.clone(),
            jni_method_name: ffi_name,
            jni_signature: self.build_async_callback_jni_signature(&method.params),
            c_params,
            setup_lines,
            jni_args,
            return_c_type,
            invoker_jni_name: format!(
                "Java_{}_Native_invokeAsyncCallback{}",
                jni_prefix, invoker_suffix
            ),
            invoker_native_name: format!("invokeAsyncCallback{}", invoker_suffix),
        }
    }

    fn build_callback_jni_signature(&self, params: &[ParamDef], returns: &ReturnDef) -> String {
        let params_sig = std::iter::once("J".to_string())
            .chain(
                params
                    .iter()
                    .map(|param| self.type_expr_jni_signature(&param.type_expr)),
            )
            .collect::<Vec<_>>()
            .join("");

        let return_sig = match returns {
            ReturnDef::Void => "V".to_string(),
            ReturnDef::Value(ty) => self.type_expr_jni_signature(ty),
            ReturnDef::Result { ok, .. } => self.type_expr_jni_signature(ok),
        };

        format!("({}){}", params_sig, return_sig)
    }

    fn build_async_callback_jni_signature(&self, params: &[ParamDef]) -> String {
        let params_sig = std::iter::once("J".to_string())
            .chain(
                params
                    .iter()
                    .map(|param| self.type_expr_jni_signature(&param.type_expr)),
            )
            .chain(["J".to_string(), "J".to_string()])
            .collect::<Vec<_>>()
            .join("");

        format!("({})V", params_sig)
    }

    fn callback_return_type<'b>(&self, returns: &'b ReturnDef) -> Option<&'b TypeExpr> {
        match returns {
            ReturnDef::Void => None,
            ReturnDef::Value(ty) => Some(ty),
            ReturnDef::Result { ok, .. } => Some(ok),
        }
    }

    fn jni_call_return_type(&self, ty: Option<&TypeExpr>) -> String {
        match ty {
            None => "void".to_string(),
            Some(TypeExpr::Primitive(p)) => {
                let model_primitive = self.to_model_primitive(*p);
                primitives::info(model_primitive).jni_type.to_string()
            }
            Some(TypeExpr::Void) => "void".to_string(),
            _ => "jobject".to_string(),
        }
    }

    fn jni_call_method_suffix(&self, ty: Option<&TypeExpr>) -> String {
        match ty {
            None | Some(TypeExpr::Void) => "Void".to_string(),
            Some(TypeExpr::Primitive(p)) => {
                let model_primitive = self.to_model_primitive(*p);
                primitives::info(model_primitive).call_suffix.to_string()
            }
            _ => "Object".to_string(),
        }
    }

    fn c_type_for_callback(&self, ty: Option<&TypeExpr>) -> String {
        match ty {
            None => "void".to_string(),
            Some(TypeExpr::Primitive(p)) => self.primitive_c_type(*p),
            Some(TypeExpr::Void) => "void".to_string(),
            _ => "void*".to_string(),
        }
    }

    fn async_invoker_suffix(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Void => "Void".to_string(),
            ReturnDef::Value(TypeExpr::Primitive(p)) => {
                let model_primitive = self.to_model_primitive(*p);
                primitives::info(model_primitive).invoker_suffix.to_string()
            }
            ReturnDef::Result { .. } => "Wire".to_string(),
            _ => "Wire".to_string(),
        }
    }

    fn type_expr_jni_signature(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => self.primitive_signature(*p),
            TypeExpr::Void => "V".to_string(),
            _ => "Ljava/nio/ByteBuffer;".to_string(),
        }
    }

    fn lower_callback_param(
        &self,
        param_name: &ParamName,
        ty: &TypeExpr,
        is_async: bool,
    ) -> LoweredCallbackParam {
        match ty {
            TypeExpr::Primitive(p) => self.lower_callback_primitive(param_name, *p),
            TypeExpr::Option(_) => self.lower_callback_optional(param_name),
            TypeExpr::String | TypeExpr::Record(_) => {
                self.lower_callback_encoded(param_name, is_async)
            }
            _ => self.lower_callback_encoded(param_name, is_async),
        }
    }

    fn lower_callback_primitive(
        &self,
        param_name: &ParamName,
        primitive: PrimitiveType,
    ) -> LoweredCallbackParam {
        let model_primitive = self.to_model_primitive(primitive);
        let info = primitives::info(model_primitive);
        let c_type = info.c_type.to_string();
        let jni_arg = info
            .jni_cast
            .map(|cast| format!("{}{}", cast, param_name.as_str()))
            .unwrap_or_else(|| param_name.as_str().to_string());

        LoweredCallbackParam {
            c_params: vec![JniCallbackCParam {
                name: param_name.as_str().to_string(),
                c_type,
            }],
            setup_lines: Vec::new(),
            jni_arg,
        }
    }

    fn lower_callback_optional(&self, param_name: &ParamName) -> LoweredCallbackParam {
        let ptr_name = format!("{}_ptr", param_name.as_str());
        let len_name = format!("{}_len", param_name.as_str());
        let buf_name = format!("buf_{}", param_name.as_str());

        let setup_lines = vec![
            format!("jobject {buf_name} = NULL;"),
            format!("if ({ptr_name} != NULL) {{"),
            format!(
                "    {buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
            ),
            "}".to_string(),
        ];

        LoweredCallbackParam {
            c_params: vec![
                JniCallbackCParam {
                    name: ptr_name,
                    c_type: "const uint8_t*".to_string(),
                },
                JniCallbackCParam {
                    name: len_name,
                    c_type: "uintptr_t".to_string(),
                },
            ],
            setup_lines,
            jni_arg: buf_name,
        }
    }

    fn lower_callback_encoded(
        &self,
        param_name: &ParamName,
        is_async: bool,
    ) -> LoweredCallbackParam {
        let ptr_name = format!("{}_ptr", param_name.as_str());
        let len_name = format!("{}_len", param_name.as_str());
        let buf_name = format!("buf_{}", param_name.as_str());

        let setup_lines = if is_async {
            vec![
                format!(
                    "if ({ptr_name} == NULL) {{ if (attached) (*g_jvm)->DetachCurrentThread(g_jvm); return; }}"
                ),
                format!(
                    "jobject {buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
                ),
                format!(
                    "if ({buf_name} == NULL) {{ if (attached) (*g_jvm)->DetachCurrentThread(g_jvm); return; }}"
                ),
            ]
        } else {
            vec![
                format!(
                    "if ({ptr_name} == NULL) {{ status->code = 1; if (attached) (*g_jvm)->DetachCurrentThread(g_jvm); return; }}"
                ),
                format!(
                    "jobject {buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
                ),
                format!(
                    "if ({buf_name} == NULL) {{ status->code = 1; if (attached) (*g_jvm)->DetachCurrentThread(g_jvm); return; }}"
                ),
            ]
        };

        LoweredCallbackParam {
            c_params: vec![
                JniCallbackCParam {
                    name: ptr_name,
                    c_type: "const uint8_t*".to_string(),
                },
                JniCallbackCParam {
                    name: len_name,
                    c_type: "uintptr_t".to_string(),
                },
            ],
            setup_lines,
            jni_arg: buf_name,
        }
    }

    fn jni_type_signature(&self, abi_type: &AbiType) -> String {
        match abi_type {
            AbiType::Bool => "Z".to_string(),
            AbiType::I8 | AbiType::U8 => "B".to_string(),
            AbiType::I16 | AbiType::U16 => "S".to_string(),
            AbiType::I32 | AbiType::U32 => "I".to_string(),
            AbiType::I64 | AbiType::U64 => "J".to_string(),
            AbiType::F32 => "F".to_string(),
            AbiType::F64 => "D".to_string(),
            _ => "Ljava/nio/ByteBuffer;".to_string(),
        }
    }

    fn callback_param(&self, param: &AbiParam, is_async: bool) -> LoweredCallbackParam {
        let param_name = param.name.as_str();
        match SyncInputAbi::from_abi_param(param) {
            Some(SyncInputAbi::Scalar) => {
                self.lower_callback_direct_param(param_name, &param.ffi_type)
            }
            Some(SyncInputAbi::WirePacket { .. }) => {
                self.lower_callback_encoded_param(param_name, is_async)
            }
            _ => panic!(
                "unsupported JNI callback param input shape: {:?}",
                param.input_shape
            ),
        }
    }

    fn lower_callback_direct_param(
        &self,
        param_name: &str,
        abi_type: &AbiType,
    ) -> LoweredCallbackParam {
        let c_type = self.c_return_type_for_abi(abi_type);
        let jni_arg = match abi_type {
            AbiType::Bool => format!("{} != 0", param_name),
            _ => param_name.to_string(),
        };

        LoweredCallbackParam {
            c_params: vec![JniCallbackCParam {
                name: param_name.to_string(),
                c_type,
            }],
            setup_lines: Vec::new(),
            jni_arg,
        }
    }

    fn lower_callback_encoded_param(
        &self,
        param_name: &str,
        is_async: bool,
    ) -> LoweredCallbackParam {
        let ptr_name = format!("{}_ptr", param_name);
        let len_name = format!("{}_len", param_name);
        let buf_name = format!("buf_{}", param_name);

        let setup_lines = if is_async {
            vec![
                format!(
                    "if ({ptr_name} == NULL) {{ if (attached) (*g_jvm)->DetachCurrentThread(g_jvm); return; }}"
                ),
                format!(
                    "jobject {buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
                ),
                format!(
                    "if ({buf_name} == NULL) {{ if (attached) (*g_jvm)->DetachCurrentThread(g_jvm); return; }}"
                ),
            ]
        } else {
            vec![
                format!(
                    "if ({ptr_name} == NULL) {{ status->code = 1; if (attached) (*g_jvm)->DetachCurrentThread(g_jvm); return; }}"
                ),
                format!(
                    "jobject {buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
                ),
                format!(
                    "if ({buf_name} == NULL) {{ status->code = 1; if (attached) (*g_jvm)->DetachCurrentThread(g_jvm); return; }}"
                ),
            ]
        };

        LoweredCallbackParam {
            c_params: vec![
                JniCallbackCParam {
                    name: ptr_name,
                    c_type: "const uint8_t*".to_string(),
                },
                JniCallbackCParam {
                    name: len_name,
                    c_type: "uintptr_t".to_string(),
                },
            ],
            setup_lines,
            jni_arg: buf_name,
        }
    }

    fn c_return_type_for_abi(&self, abi_type: &AbiType) -> String {
        match abi_type {
            AbiType::Bool => "bool".to_string(),
            AbiType::I8 => "int8_t".to_string(),
            AbiType::U8 => "uint8_t".to_string(),
            AbiType::I16 => "int16_t".to_string(),
            AbiType::U16 => "uint16_t".to_string(),
            AbiType::I32 => "int32_t".to_string(),
            AbiType::U32 => "uint32_t".to_string(),
            AbiType::I64 | AbiType::ISize => "int64_t".to_string(),
            AbiType::U64 | AbiType::USize => "uint64_t".to_string(),
            AbiType::F32 => "float".to_string(),
            AbiType::F64 => "double".to_string(),
            AbiType::Void | AbiType::Pointer => "void".to_string(),
        }
    }

    fn jni_return_type_for_abi(&self, abi_type: &AbiType) -> String {
        match abi_type {
            AbiType::Bool => "jboolean".to_string(),
            AbiType::I8 | AbiType::U8 => "jbyte".to_string(),
            AbiType::I16 | AbiType::U16 => "jshort".to_string(),
            AbiType::I32 | AbiType::U32 => "jint".to_string(),
            AbiType::I64 | AbiType::U64 => "jlong".to_string(),
            AbiType::F32 => "jfloat".to_string(),
            AbiType::F64 => "jdouble".to_string(),
            _ => "jobject".to_string(),
        }
    }

    fn jni_call_type_for_abi(&self, abi_type: &AbiType) -> String {
        match abi_type {
            AbiType::Bool => "Boolean".to_string(),
            AbiType::I8 | AbiType::U8 => "Byte".to_string(),
            AbiType::I16 | AbiType::U16 => "Short".to_string(),
            AbiType::I32 | AbiType::U32 => "Int".to_string(),
            AbiType::I64 | AbiType::U64 => "Long".to_string(),
            AbiType::F32 => "Float".to_string(),
            AbiType::F64 => "Double".to_string(),
            _ => "Object".to_string(),
        }
    }

    fn invoker_result_type(suffix: &str) -> Option<JniInvokerResult> {
        match suffix {
            "Void" => None,
            "Wire" => Some(JniInvokerResult {
                c_type: "wire".to_string(),
                jni_type: "jbyteArray".to_string(),
            }),
            "Bool" => Some(JniInvokerResult {
                c_type: "bool".to_string(),
                jni_type: "jboolean".to_string(),
            }),
            "I8" => Some(JniInvokerResult {
                c_type: "int8_t".to_string(),
                jni_type: "jbyte".to_string(),
            }),
            "I16" => Some(JniInvokerResult {
                c_type: "int16_t".to_string(),
                jni_type: "jshort".to_string(),
            }),
            "I32" => Some(JniInvokerResult {
                c_type: "int32_t".to_string(),
                jni_type: "jint".to_string(),
            }),
            "I64" => Some(JniInvokerResult {
                c_type: "int64_t".to_string(),
                jni_type: "jlong".to_string(),
            }),
            "F32" => Some(JniInvokerResult {
                c_type: "float".to_string(),
                jni_type: "jfloat".to_string(),
            }),
            "F64" => Some(JniInvokerResult {
                c_type: "double".to_string(),
                jni_type: "jdouble".to_string(),
            }),
            _ => Some(JniInvokerResult {
                c_type: "void*".to_string(),
                jni_type: "jobject".to_string(),
            }),
        }
    }

    fn invoker_suffix(&self, abi_type: &AbiType) -> String {
        match abi_type {
            AbiType::Bool => "Bool".to_string(),
            AbiType::I8 => "I8".to_string(),
            AbiType::I16 => "I16".to_string(),
            AbiType::I32 => "I32".to_string(),
            AbiType::I64 => "I64".to_string(),
            AbiType::F32 => "F32".to_string(),
            AbiType::F64 => "F64".to_string(),
            _ => "Object".to_string(),
        }
    }

    fn collect_async_invokers(
        &self,
        callback_traits: &[JniCallbackTrait],
        jni_prefix: &str,
    ) -> Vec<JniAsyncCallbackInvoker> {
        let mut seen = HashSet::new();
        callback_traits
            .iter()
            .flat_map(|trait_view| &trait_view.async_methods)
            .filter_map(|method| {
                let suffix = method
                    .invoker_native_name
                    .strip_prefix("invokeAsyncCallback")
                    .map(|value| value.to_string())?;
                if seen.insert(suffix.clone()) {
                    Some(self.build_async_invoker(&suffix, jni_prefix))
                } else {
                    None
                }
            })
            .collect()
    }

    fn build_async_invoker(&self, suffix: &str, jni_prefix: &str) -> JniAsyncCallbackInvoker {
        JniAsyncCallbackInvoker {
            suffix: suffix.to_string(),
            jni_fn_name: format!("Java_{}_Native_invokeAsyncCallback{}", jni_prefix, suffix),
            result_type: Self::invoker_result_type(suffix),
        }
    }

    fn collect_closure_trampolines(
        &self,
        package_path: &str,
        used_callbacks: &HashSet<CallbackId>,
    ) -> Vec<JniClosureTrampoline> {
        self.contract
            .catalog
            .all_callbacks()
            .filter(|callback| matches!(callback.kind, CallbackKind::Closure))
            .filter(|callback| used_callbacks.contains(&callback.id))
            .map(|callback| self.lower_closure_trampoline(callback, package_path))
            .collect()
    }

    fn lower_closure_trampoline(
        &self,
        callback: &CallbackTraitDef,
        package_path: &str,
    ) -> JniClosureTrampoline {
        let signature_id = callback
            .id
            .as_str()
            .strip_prefix("__Closure_")
            .unwrap_or(callback.id.as_str())
            .to_string();

        let callbacks_class = format!("Closure{}Callbacks", signature_id);
        let callbacks_class_jni_path =
            format!("{}/{}", package_path.replace('.', "/"), callbacks_class);

        let method = callback
            .methods
            .iter()
            .find(|method| method.id.as_str() == "call");
        let params = method
            .map(|method| method.params.as_slice())
            .unwrap_or_default();

        let record_params = params
            .iter()
            .enumerate()
            .filter_map(|(index, param)| self.closure_record_param(index, &param.type_expr))
            .collect::<Vec<_>>();

        let c_params = self.closure_c_params(params);
        let jni_params_signature = self.closure_jni_params_signature(params);
        let jni_call_args = self.closure_jni_call_args(params);

        JniClosureTrampoline {
            trampoline_name: format!("trampoline_{}", signature_id),
            signature_id,
            callbacks_class_jni_path,
            c_params,
            jni_params_signature,
            jni_call_args,
            record_params,
        }
    }

    fn closure_record_param(&self, index: usize, ty: &TypeExpr) -> Option<JniClosureRecordParam> {
        match ty {
            TypeExpr::Record(id) => {
                let c_type = NamingConvention::class_name(id.as_str());
                let size = self.record_struct_size(id).to_string();
                Some(JniClosureRecordParam {
                    index,
                    c_type,
                    size,
                })
            }
            _ => None,
        }
    }

    fn closure_c_params(&self, params: &[ParamDef]) -> String {
        let items = params
            .iter()
            .enumerate()
            .map(|(index, param)| match param.type_expr {
                TypeExpr::Primitive(p) => format!("{} p{}", self.primitive_c_type(p), index),
                TypeExpr::Record(_) | TypeExpr::String => {
                    format!("const uint8_t* p{}_ptr, uintptr_t p{}_len", index, index)
                }
                _ => format!("void* p{}", index),
            })
            .collect::<Vec<_>>();

        if items.is_empty() {
            String::new()
        } else {
            format!(", {}", items.join(", "))
        }
    }

    fn closure_jni_params_signature(&self, params: &[ParamDef]) -> String {
        params
            .iter()
            .map(|param| match &param.type_expr {
                TypeExpr::Primitive(p) => self.primitive_signature(*p),
                TypeExpr::String => "Ljava/lang/String;".to_string(),
                TypeExpr::Record(_) => "Ljava/nio/ByteBuffer;".to_string(),
                _ => "Ljava/lang/Object;".to_string(),
            })
            .collect::<Vec<_>>()
            .join("")
    }

    fn closure_jni_call_args(&self, params: &[ParamDef]) -> String {
        params
            .iter()
            .enumerate()
            .map(|(index, param)| match &param.type_expr {
                TypeExpr::Primitive(p) => format!("({})p{}", self.primitive_jni_cast(*p), index),
                TypeExpr::Record(_) => format!("buf_p{}", index),
                _ => format!("(jlong)p{}", index),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn primitive_jni_cast(&self, primitive: PrimitiveType) -> &'static str {
        match primitive {
            PrimitiveType::Bool => "jboolean",
            PrimitiveType::I8 | PrimitiveType::U8 => "jbyte",
            PrimitiveType::I16 | PrimitiveType::U16 => "jshort",
            PrimitiveType::I32 | PrimitiveType::U32 => "jint",
            PrimitiveType::I64 | PrimitiveType::U64 => "jlong",
            PrimitiveType::F32 => "jfloat",
            PrimitiveType::F64 => "jdouble",
            PrimitiveType::ISize | PrimitiveType::USize => "jlong",
        }
    }
}

#[derive(Clone)]
struct RecordParamInfo {
    id: RecordId,
    struct_size: usize,
}

#[derive(Clone)]
struct DataEnumParamInfo {
    id: EnumId,
}

#[derive(Clone)]
struct LoweredCallbackParam {
    c_params: Vec<JniCallbackCParam>,
    setup_lines: Vec<String>,
    jni_arg: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::abi::AbiContract;
    use crate::ir::contract::{FfiContract, PackageInfo, TypeCatalog};
    use crate::ir::types::PrimitiveType;

    fn test_lowerer() -> JniLowerer<'static> {
        static CONTRACT: std::sync::LazyLock<FfiContract> =
            std::sync::LazyLock::new(|| FfiContract {
                package: PackageInfo {
                    name: "test".to_string(),
                    version: None,
                },
                catalog: TypeCatalog::default(),
                functions: vec![],
            });
        static ABI: std::sync::LazyLock<AbiContract> =
            std::sync::LazyLock::new(|| crate::ir::Lowerer::new(&CONTRACT).to_abi_contract());

        JniLowerer::new(
            &CONTRACT,
            &ABI,
            "com.test".to_string(),
            "Native".to_string(),
        )
    }

    #[test]
    fn primitive_c_type_bool_is_bool_not_uint8() {
        let lowerer = test_lowerer();
        assert_eq!(lowerer.primitive_c_type(PrimitiveType::Bool), "bool");
    }

    #[test]
    fn primitive_c_type_matches_cbindgen_for_all_types() {
        let lowerer = test_lowerer();
        let cases = [
            (PrimitiveType::Bool, "bool"),
            (PrimitiveType::I8, "int8_t"),
            (PrimitiveType::U8, "uint8_t"),
            (PrimitiveType::I16, "int16_t"),
            (PrimitiveType::U16, "uint16_t"),
            (PrimitiveType::I32, "int32_t"),
            (PrimitiveType::U32, "uint32_t"),
            (PrimitiveType::I64, "int64_t"),
            (PrimitiveType::U64, "uint64_t"),
            (PrimitiveType::ISize, "int64_t"),
            (PrimitiveType::USize, "uint64_t"),
            (PrimitiveType::F32, "float"),
            (PrimitiveType::F64, "double"),
        ];
        cases
            .iter()
            .for_each(|(prim, expected)| assert_eq!(lowerer.primitive_c_type(*prim), *expected));
    }

    #[test]
    fn c_return_type_for_abi_bool_is_bool() {
        let lowerer = test_lowerer();
        assert_eq!(lowerer.c_return_type_for_abi(&AbiType::Bool), "bool");
    }

    #[test]
    fn c_return_type_for_abi_matches_primitive_c_type() {
        let lowerer = test_lowerer();
        let abi_types = [
            (AbiType::Bool, PrimitiveType::Bool),
            (AbiType::I8, PrimitiveType::I8),
            (AbiType::U8, PrimitiveType::U8),
            (AbiType::I16, PrimitiveType::I16),
            (AbiType::U16, PrimitiveType::U16),
            (AbiType::I32, PrimitiveType::I32),
            (AbiType::U32, PrimitiveType::U32),
            (AbiType::I64, PrimitiveType::I64),
            (AbiType::U64, PrimitiveType::U64),
            (AbiType::F32, PrimitiveType::F32),
            (AbiType::F64, PrimitiveType::F64),
        ];
        abi_types.iter().for_each(|(abi, prim)| {
            assert_eq!(
                lowerer.c_return_type_for_abi(abi),
                lowerer.primitive_c_type(*prim),
                "mismatch for {:?}",
                abi
            );
        });
    }
}
