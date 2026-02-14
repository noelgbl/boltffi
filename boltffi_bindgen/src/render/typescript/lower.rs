use std::collections::{HashMap, HashSet};

use boltffi_ffi_rules::callback as cb_naming;
use boltffi_ffi_rules::naming::{self, snake_to_camel as camel_case};

use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiContract, AbiEnum, AbiEnumPayload, AbiParam, AbiRecord,
    AsyncResultTransport, CallId, CallMode, ErrorTransport, ParamRole, ReturnTransport,
};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{
    CallbackKind, CallbackTraitDef, ClassDef, ConstructorDef, EnumDef, FunctionDef, MethodDef,
    ParamDef, Receiver, RecordDef, ReturnDef,
};
use crate::ir::ids::{CallbackId, EnumId, FieldName, RecordId};
use crate::ir::ops::{
    FieldWriteOp, ReadOp, ReadSeq, SizeExpr, ValueExpr, WireShape, WriteOp, WriteSeq,
};
use crate::ir::plan::AbiType;
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::render::typescript::emit;
use crate::render::typescript::plan::*;
use boltffi_ffi_rules::naming::ffi_prefix;

struct AbiIndex {
    calls: HashMap<CallId, usize>,
    callbacks: HashMap<CallbackId, usize>,
    records: HashMap<RecordId, usize>,
    enums: HashMap<EnumId, usize>,
}

impl AbiIndex {
    fn new(contract: &AbiContract) -> Self {
        let calls = contract
            .calls
            .iter()
            .enumerate()
            .map(|(index, call)| (call.id.clone(), index))
            .collect();
        let callbacks = contract
            .callbacks
            .iter()
            .enumerate()
            .map(|(index, cb)| (cb.callback_id.clone(), index))
            .collect();
        let records = contract
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| (record.id.clone(), index))
            .collect();
        let enums = contract
            .enums
            .iter()
            .enumerate()
            .map(|(index, enumeration)| (enumeration.id.clone(), index))
            .collect();

        Self {
            calls,
            callbacks,
            records,
            enums,
        }
    }

    fn callback<'a>(
        &self,
        contract: &'a AbiContract,
        id: &CallbackId,
    ) -> &'a AbiCallbackInvocation {
        &contract.callbacks[self.callbacks[id]]
    }

    fn call<'a>(&self, contract: &'a AbiContract, id: &CallId) -> &'a AbiCall {
        &contract.calls[self.calls[id]]
    }

    fn record<'a>(&self, contract: &'a AbiContract, id: &RecordId) -> &'a AbiRecord {
        &contract.records[self.records[id]]
    }

    fn enumeration<'a>(&self, contract: &'a AbiContract, id: &EnumId) -> &'a AbiEnum {
        &contract.enums[self.enums[id]]
    }
}

pub struct TypeScriptLowerer<'a> {
    contract: &'a FfiContract,
    abi: &'a AbiContract,
    module_name: String,
}

impl<'a> TypeScriptLowerer<'a> {
    pub fn new(contract: &'a FfiContract, abi: &'a AbiContract, module_name: String) -> Self {
        Self {
            contract,
            abi,
            module_name,
        }
    }

    pub fn lower(&self) -> TsModule {
        let index = AbiIndex::new(self.abi);

        let records = self
            .contract
            .catalog
            .all_records()
            .map(|def| self.lower_record(def, &index))
            .collect();

        let enums = self
            .contract
            .catalog
            .all_enums()
            .map(|def| self.lower_enum(def, &index))
            .collect();

        let functions: Vec<TsFunction> = self
            .contract
            .functions
            .iter()
            .filter_map(|def| self.lower_function(def, &index))
            .collect();

        let async_functions: Vec<TsAsyncFunction> = self
            .contract
            .functions
            .iter()
            .filter_map(|def| self.lower_async_function(def, &index))
            .collect();

        let classes = self
            .contract
            .catalog
            .all_classes()
            .map(|def| self.lower_class(def, &index))
            .collect();

        let wasm_imports = self.collect_wasm_imports(&index);

        let callbacks = self
            .contract
            .catalog
            .all_callbacks()
            .map(|def| self.lower_callback(def, &index))
            .collect();

        let error_exceptions = self.collect_error_exceptions(&functions, &async_functions, &index);

        TsModule {
            module_name: self.module_name.clone(),
            abi_version: 1,
            records,
            enums,
            error_exceptions,
            functions,
            async_functions,
            classes,
            callbacks,
            wasm_imports,
        }
    }

    fn collect_error_exceptions(
        &self,
        functions: &[TsFunction],
        async_functions: &[TsAsyncFunction],
        index: &AbiIndex,
    ) -> Vec<TsErrorException> {
        let mut error_types: HashSet<String> = HashSet::new();

        for func in functions {
            if func.throws && !func.err_type.is_empty() && !is_excluded_error_type(&func.err_type) {
                error_types.insert(func.err_type.clone());
            }
        }

        for func in async_functions {
            if func.throws && !func.err_type.is_empty() && !is_excluded_error_type(&func.err_type) {
                error_types.insert(func.err_type.clone());
            }
        }

        error_types
            .into_iter()
            .map(|type_name| {
                let is_c_style_enum = self
                    .contract
                    .catalog
                    .all_enums()
                    .find(|e| naming::to_upper_camel_case(e.id.as_str()) == type_name)
                    .map(|e| {
                        let abi_enum = index.enumeration(self.abi, &e.id);
                        abi_enum.is_c_style
                    })
                    .unwrap_or(false);

                TsErrorException {
                    class_name: format!("{}Exception", type_name),
                    type_name,
                    is_c_style_enum,
                }
            })
            .collect()
    }

    fn lower_record(&self, def: &RecordDef, index: &AbiIndex) -> TsRecord {
        let abi_record = index.record(self.abi, &def.id);
        let name = naming::to_upper_camel_case(def.id.as_str());

        let decode_fields = record_decode_fields(abi_record);
        let encode_fields = record_encode_fields(abi_record);

        let fields: Vec<TsField> = def
            .fields
            .iter()
            .map(|field| {
                let ts_type_str = emit::ts_type(&field.type_expr);
                let field_name = camel_case(field.name.as_str());

                let decode = decode_fields
                    .get(&field.name)
                    .cloned()
                    .unwrap_or_else(|| ReadSeq {
                        size: SizeExpr::Fixed(0),
                        ops: vec![],
                        shape: WireShape::Value,
                    });
                let encode = encode_fields
                    .get(&field.name)
                    .cloned()
                    .unwrap_or_else(|| WriteSeq {
                        size: SizeExpr::Fixed(0),
                        ops: vec![],
                        shape: WireShape::Value,
                    });

                TsField {
                    name: emit::escape_ts_keyword(&field_name),
                    ts_type: ts_type_str,
                    decode,
                    encode,
                    doc: field.doc.clone(),
                }
            })
            .collect();

        let tail_padding = if abi_record.is_blittable {
            let packed_size: usize = fields
                .iter()
                .map(|f| match f.encode.size {
                    SizeExpr::Fixed(n) => n,
                    _ => 0,
                })
                .sum();
            abi_record.size.unwrap_or(0).saturating_sub(packed_size)
        } else {
            0
        };

        TsRecord {
            name,
            fields,
            is_blittable: abi_record.is_blittable,
            wire_size: abi_record.size,
            tail_padding,
            doc: def.doc.clone(),
        }
    }

    fn lower_enum(&self, def: &EnumDef, index: &AbiIndex) -> TsEnum {
        let abi_enum = index.enumeration(self.abi, &def.id);
        let name = naming::to_upper_camel_case(def.id.as_str());

        let kind = if abi_enum.is_c_style {
            TsEnumKind::CStyle
        } else {
            TsEnumKind::Data
        };

        let variant_docs = def.variant_docs();
        let variants = abi_enum
            .variants
            .iter()
            .enumerate()
            .map(|(idx, abi_variant)| {
                let fields = match &abi_variant.payload {
                    AbiEnumPayload::Unit => vec![],
                    AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => fields
                        .iter()
                        .map(|field| TsVariantField {
                            name: camel_case(field.name.as_str()),
                            ts_type: emit::ts_type(&field.type_expr),
                            decode: field.decode.clone(),
                            encode: remap_named_to_field(&field.encode),
                        })
                        .collect(),
                };

                TsVariant {
                    name: naming::to_upper_camel_case(abi_variant.name.as_str()),
                    discriminant: abi_variant.discriminant,
                    fields,
                    doc: variant_docs.get(idx).cloned().flatten(),
                }
            })
            .collect();

        TsEnum {
            name,
            variants,
            kind,
            doc: def.doc.clone(),
        }
    }

    fn lower_class(&self, def: &ClassDef, index: &AbiIndex) -> TsClass {
        let class_name = naming::to_upper_camel_case(def.id.as_str());
        let ffi_free = naming::class_ffi_free(def.id.as_str()).as_str().to_string();

        let constructors = def
            .constructors
            .iter()
            .enumerate()
            .map(|(constructor_index, constructor)| {
                self.lower_class_constructor(def, constructor, constructor_index, index)
            })
            .collect();

        let methods = def
            .methods
            .iter()
            .map(|method| self.lower_class_method(def, method, index))
            .collect();

        TsClass {
            class_name,
            ffi_free,
            constructors,
            methods,
            doc: def.doc.clone(),
        }
    }

    fn lower_class_constructor(
        &self,
        class_def: &ClassDef,
        constructor: &ConstructorDef,
        constructor_index: usize,
        index: &AbiIndex,
    ) -> TsClassConstructor {
        let call_id = CallId::Constructor {
            class_id: class_def.id.clone(),
            index: constructor_index,
        };
        let abi_call = index.call(self.abi, &call_id);

        let ts_name = constructor
            .name()
            .map(|method_id| emit::escape_ts_keyword(&camel_case(method_id.as_str())))
            .unwrap_or_else(|| "new".to_string());

        let param_defs: HashMap<&str, &ParamDef> = constructor
            .params()
            .into_iter()
            .map(|param| (param.name.as_str(), param))
            .collect();

        let params = abi_call
            .params
            .iter()
            .filter(|parameter| {
                !matches!(
                    parameter.role,
                    ParamRole::SyntheticLen { .. }
                        | ParamRole::OutLen { .. }
                        | ParamRole::StatusOut
                )
            })
            .map(|abi_param| {
                let param_def = param_defs.get(abi_param.name.as_str()).copied();
                self.lower_param(param_def, abi_param)
            })
            .collect();

        TsClassConstructor {
            ts_name,
            ffi_name: abi_call.symbol.as_str().to_string(),
            is_default: constructor.name().is_none(),
            params,
            returns_nullable_handle: matches!(
                abi_call.return_,
                ReturnTransport::Handle { nullable: true, .. }
            ),
            doc: constructor.doc().map(String::from),
        }
    }

    fn lower_class_method(
        &self,
        class_def: &ClassDef,
        method_def: &MethodDef,
        index: &AbiIndex,
    ) -> TsClassMethod {
        let call_id = CallId::Method {
            class_id: class_def.id.clone(),
            method_id: method_def.id.clone(),
        };
        let abi_call = index.call(self.abi, &call_id);
        let is_static = method_def.receiver == Receiver::Static;

        let param_defs: HashMap<&str, &ParamDef> = method_def
            .params
            .iter()
            .map(|param| (param.name.as_str(), param))
            .collect();

        let params = abi_call
            .params
            .iter()
            .enumerate()
            .filter(|(param_index, parameter)| {
                if matches!(
                    parameter.role,
                    ParamRole::SyntheticLen { .. }
                        | ParamRole::OutLen { .. }
                        | ParamRole::StatusOut
                ) {
                    return false;
                }
                if !is_static
                    && *param_index == 0
                    && matches!(parameter.role, ParamRole::InHandle { .. })
                {
                    return false;
                }
                true
            })
            .map(|(_, abi_param)| {
                let param_def = param_defs.get(abi_param.name.as_str()).copied();
                self.lower_param(param_def, abi_param)
            })
            .collect();

        let (return_type, return_handle, mode) = match &abi_call.mode {
            CallMode::Sync => {
                let (return_type, return_abi, decode_expr) = self.lower_return(&abi_call.return_);
                let return_handle = match &abi_call.return_ {
                    ReturnTransport::Handle { class_id, nullable } => Some(TsHandleReturn {
                        class_name: naming::to_upper_camel_case(class_id.as_str()),
                        nullable: *nullable,
                    }),
                    _ => None,
                };
                (
                    return_type,
                    return_handle,
                    TsClassMethodMode::Sync(TsClassSyncMethod {
                        return_abi,
                        decode_expr,
                    }),
                )
            }
            CallMode::Async(async_call) => {
                let entry_ffi_name = abi_call.symbol.as_str().to_string();
                let (return_type, decode_expr) = self.lower_async_result(&async_call.result);
                let return_handle = match &async_call.result {
                    AsyncResultTransport::Handle { class_id, nullable } => Some(TsHandleReturn {
                        class_name: naming::to_upper_camel_case(class_id.as_str()),
                        nullable: *nullable,
                    }),
                    _ => None,
                };
                (
                    return_type,
                    return_handle,
                    TsClassMethodMode::Async(TsClassAsyncMethod {
                        poll_sync_ffi_name: format!("{entry_ffi_name}_poll_sync"),
                        complete_ffi_name: format!("{entry_ffi_name}_complete"),
                        panic_message_ffi_name: format!("{entry_ffi_name}_panic_message"),
                        cancel_ffi_name: format!("{entry_ffi_name}_cancel"),
                        free_ffi_name: format!("{entry_ffi_name}_free"),
                        decode_expr,
                    }),
                )
            }
        };

        TsClassMethod {
            ts_name: emit::escape_ts_keyword(&camel_case(method_def.id.as_str())),
            ffi_name: abi_call.symbol.as_str().to_string(),
            is_static,
            params,
            return_type,
            return_handle,
            mode,
            doc: method_def.doc.clone(),
        }
    }

    fn lower_callback(&self, def: &CallbackTraitDef, index: &AbiIndex) -> TsCallback {
        let abi_callback = index.callback(self.abi, &def.id);
        let interface_name = naming::to_upper_camel_case(def.id.as_str());
        let trait_name_snake = naming::to_snake_case(def.id.as_str());
        let create_handle_fn = cb_naming::callback_create_handle_global().to_string();

        let methods = def
            .methods
            .iter()
            .filter(|m| !m.is_async)
            .filter_map(|method_def| {
                let abi_method = abi_callback
                    .methods
                    .iter()
                    .find(|am| am.id == method_def.id)?;
                let ts_name = camel_case(method_def.id.as_str());
                let import_name = format!(
                    "__boltffi_callback_{}_{}",
                    trait_name_snake,
                    naming::to_snake_case(method_def.id.as_str())
                );

                let params = method_def
                    .params
                    .iter()
                    .map(|p| {
                        let ts_type = emit::ts_type(&p.type_expr);
                        let param_name = p.name.as_str();
                        let callback_param_name = camel_case(param_name);
                        let abi_param = abi_method
                            .params
                            .iter()
                            .find(|ap| ap.name.as_str() == param_name);

                        let kind = match abi_param {
                            Some(abi_param)
                                if matches!(abi_param.role, ParamRole::InEncoded { .. }) =>
                            {
                                let ParamRole::InEncoded { decode_ops, .. } = &abi_param.role
                                else {
                                    unreachable!();
                                };
                                let decode_expr = emit::emit_reader_read(decode_ops);
                                TsCallbackParamKind::WireEncoded { decode_expr }
                            }
                            Some(abi_param) => callback_primitive_param_kind(
                                callback_param_name.as_str(),
                                Some(abi_param.ffi_type),
                            ),
                            None => {
                                callback_primitive_param_kind(callback_param_name.as_str(), None)
                            }
                        };

                        TsCallbackParam {
                            name: callback_param_name,
                            ts_type,
                            kind,
                        }
                    })
                    .collect();

                let return_kind = match &abi_method.return_ {
                    ReturnTransport::Void => TsCallbackReturnKind::Void,
                    ReturnTransport::Direct(_) => {
                        let ts_type = match &method_def.returns {
                            ReturnDef::Value(ty) => emit::ts_type(ty),
                            _ => "number".to_string(),
                        };
                        TsCallbackReturnKind::Primitive { ts_type }
                    }
                    ReturnTransport::Encoded { encode_ops, .. } => {
                        let ts_type = match &method_def.returns {
                            ReturnDef::Value(ty) => emit::ts_type(ty),
                            ReturnDef::Result { ok, .. } => emit::ts_type(ok),
                            _ => "unknown".to_string(),
                        };
                        let encode_expr = emit::emit_writer_write(encode_ops, "writer", "result");
                        let size_expr = emit::emit_size_expr(&encode_ops.size, "result");
                        TsCallbackReturnKind::WireEncoded {
                            ts_type,
                            encode_expr,
                            size_expr,
                        }
                    }
                    ReturnTransport::Handle { .. } | ReturnTransport::Callback { .. } => {
                        TsCallbackReturnKind::Primitive {
                            ts_type: "number".to_string(),
                        }
                    }
                };

                Some(TsCallbackMethod {
                    ts_name,
                    import_name,
                    params,
                    return_kind,
                    doc: method_def.doc.clone(),
                })
            })
            .collect();

        let async_methods = def
            .methods
            .iter()
            .filter(|m| m.is_async)
            .filter_map(|method_def| {
                let abi_method = abi_callback
                    .methods
                    .iter()
                    .find(|am| am.id == method_def.id)?;
                let ts_name = camel_case(method_def.id.as_str());
                let method_name_snake = naming::to_snake_case(method_def.id.as_str());
                let start_import_name = format!(
                    "__boltffi_callback_{}_{}_start",
                    trait_name_snake, method_name_snake
                );
                let complete_export_name = format!(
                    "boltffi_callback_{}_{}_complete",
                    trait_name_snake, method_name_snake
                );

                let params = method_def
                    .params
                    .iter()
                    .map(|p| {
                        let ts_type = emit::ts_type(&p.type_expr);
                        let param_name = p.name.as_str();
                        let callback_param_name = camel_case(param_name);
                        let abi_param = abi_method
                            .params
                            .iter()
                            .find(|ap| ap.name.as_str() == param_name);

                        let kind = match abi_param {
                            Some(abi_param)
                                if matches!(abi_param.role, ParamRole::InEncoded { .. }) =>
                            {
                                let ParamRole::InEncoded { decode_ops, .. } = &abi_param.role
                                else {
                                    unreachable!();
                                };
                                let decode_expr = emit::emit_reader_read(decode_ops);
                                TsCallbackParamKind::WireEncoded { decode_expr }
                            }
                            Some(abi_param) => callback_primitive_param_kind(
                                callback_param_name.as_str(),
                                Some(abi_param.ffi_type),
                            ),
                            None => {
                                callback_primitive_param_kind(callback_param_name.as_str(), None)
                            }
                        };

                        TsCallbackParam {
                            name: callback_param_name,
                            ts_type,
                            kind,
                        }
                    })
                    .collect();

                let (
                    return_type,
                    encode_expr,
                    size_expr,
                    direct_write_method,
                    direct_write_value_expr,
                    direct_size,
                ) = match &abi_method.return_ {
                    ReturnTransport::Void => (None, None, None, None, None, None),
                    ReturnTransport::Direct(abi) => {
                        let ts_type = match &method_def.returns {
                            ReturnDef::Value(ty) => emit::ts_type(ty),
                            _ => "number".to_string(),
                        };
                        let direct_write = direct_write_info(abi);
                        (
                            Some(ts_type),
                            None,
                            None,
                            Some(direct_write.method_name.to_string()),
                            Some(direct_write_argument_expr(abi, "result")),
                            Some(direct_write.byte_width),
                        )
                    }
                    ReturnTransport::Encoded { encode_ops, .. } => {
                        let ts_type = match &method_def.returns {
                            ReturnDef::Value(ty) => emit::ts_type(ty),
                            ReturnDef::Result { ok, .. } => emit::ts_type(ok),
                            _ => "unknown".to_string(),
                        };
                        let encode_expr = emit::emit_writer_write(encode_ops, "writer", "result");
                        let size_expr = emit::emit_size_expr(&encode_ops.size, "result");
                        (
                            Some(ts_type),
                            Some(encode_expr),
                            Some(size_expr),
                            None,
                            None,
                            None,
                        )
                    }
                    ReturnTransport::Handle { .. } | ReturnTransport::Callback { .. } => (
                        Some("number".to_string()),
                        None,
                        None,
                        Some("writeU32".to_string()),
                        Some("result".to_string()),
                        Some(4),
                    ),
                };

                Some(TsAsyncCallbackMethod {
                    ts_name,
                    start_import_name,
                    complete_export_name,
                    params,
                    return_type,
                    encode_expr,
                    size_expr,
                    direct_write_method,
                    direct_write_value_expr,
                    direct_size,
                    doc: method_def.doc.clone(),
                })
            })
            .collect();

        let closure_fn_type = matches!(def.kind, CallbackKind::Closure)
            .then(|| {
                def.methods.first().map(|method| {
                    let params = method
                        .params
                        .iter()
                        .enumerate()
                        .map(|(i, p)| format!("p{}: {}", i, emit::ts_type(&p.type_expr)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let return_type = match &method.returns {
                        ReturnDef::Void => "void".to_string(),
                        ReturnDef::Value(ty) => emit::ts_type(ty),
                        ReturnDef::Result { ok, .. } => emit::ts_type(ok),
                    };
                    format!("({}) => {}", params, return_type)
                })
            })
            .flatten();

        TsCallback {
            interface_name,
            trait_name_snake,
            create_handle_fn,
            methods,
            async_methods,
            closure_fn_type,
            doc: def.doc.clone(),
        }
    }

    fn lower_function(&self, def: &FunctionDef, index: &AbiIndex) -> Option<TsFunction> {
        let call_id = CallId::Function(def.id.clone());
        let abi_call = index.call(self.abi, &call_id);

        if matches!(abi_call.mode, CallMode::Async(_)) {
            return None;
        }

        let func_name = camel_case(def.id.as_str());
        let ffi_name = abi_call.symbol.as_str().to_string();

        let param_defs: HashMap<&str, &ParamDef> =
            def.params.iter().map(|p| (p.name.as_str(), p)).collect();

        let params = abi_call
            .params
            .iter()
            .filter(|p| {
                !matches!(
                    p.role,
                    ParamRole::SyntheticLen { .. }
                        | ParamRole::OutLen { .. }
                        | ParamRole::StatusOut
                )
            })
            .map(|abi_param| {
                let param_def = param_defs.get(abi_param.name.as_str()).copied();
                self.lower_param(param_def, abi_param)
            })
            .collect();

        let (return_type, return_abi, decode_expr) = self.lower_return(&abi_call.return_);
        let (throws, err_type) = self.lower_error(&abi_call.error);

        Some(TsFunction {
            name: emit::escape_ts_keyword(&func_name),
            ffi_name,
            params,
            return_type,
            return_abi,
            decode_expr,
            throws,
            err_type,
            doc: def.doc.clone(),
        })
    }

    fn lower_async_function(&self, def: &FunctionDef, index: &AbiIndex) -> Option<TsAsyncFunction> {
        let call_id = CallId::Function(def.id.clone());
        let abi_call = index.call(self.abi, &call_id);

        let async_call = match &abi_call.mode {
            CallMode::Async(async_call) => async_call,
            _ => return None,
        };

        let func_name = camel_case(def.id.as_str());
        let fn_name_snake = naming::to_snake_case(def.id.as_str());
        let base_ffi_name = format!("{}_{}", ffi_prefix(), fn_name_snake);

        let param_defs: HashMap<&str, &ParamDef> =
            def.params.iter().map(|p| (p.name.as_str(), p)).collect();

        let params = abi_call
            .params
            .iter()
            .filter(|p| {
                !matches!(
                    p.role,
                    ParamRole::SyntheticLen { .. }
                        | ParamRole::OutLen { .. }
                        | ParamRole::StatusOut
                )
            })
            .map(|abi_param| {
                let param_def = param_defs.get(abi_param.name.as_str()).copied();
                self.lower_param(param_def, abi_param)
            })
            .collect();

        let (return_type, decode_expr) = self.lower_async_result(&async_call.result);
        let (throws, err_type) = self.lower_error(&async_call.error);

        Some(TsAsyncFunction {
            name: emit::escape_ts_keyword(&func_name),
            entry_ffi_name: base_ffi_name.clone(),
            poll_sync_ffi_name: format!("{}_poll_sync", base_ffi_name),
            complete_ffi_name: format!("{}_complete", base_ffi_name),
            panic_message_ffi_name: format!("{}_panic_message", base_ffi_name),
            cancel_ffi_name: format!("{}_cancel", base_ffi_name),
            free_ffi_name: format!("{}_free", base_ffi_name),
            params,
            return_type,
            decode_expr,
            throws,
            err_type,
            doc: def.doc.clone(),
        })
    }

    fn lower_param(&self, param_def: Option<&ParamDef>, abi_param: &AbiParam) -> TsParam {
        let name = camel_case(abi_param.name.as_str());
        match &abi_param.role {
            ParamRole::InDirect => TsParam {
                name: emit::escape_ts_keyword(&name),
                ts_type: ts_abi_type(&abi_param.ffi_type),
                conversion: TsParamConversion::Direct,
            },
            ParamRole::InString { .. } => TsParam {
                name: emit::escape_ts_keyword(&name),
                ts_type: "string".to_string(),
                conversion: TsParamConversion::String,
            },
            ParamRole::InBuffer { element_abi, .. } => {
                let (ts_type, conversion) = match element_abi {
                    AbiType::U8 => ("Uint8Array".to_string(), TsParamConversion::Bytes),
                    _ => (
                        param_def
                            .map(|p| emit::ts_type(&p.type_expr))
                            .unwrap_or_else(|| primitive_buffer_ts_type(*element_abi)),
                        TsParamConversion::PrimitiveBuffer {
                            element_abi: *element_abi,
                        },
                    ),
                };
                TsParam {
                    name: emit::escape_ts_keyword(&name),
                    ts_type,
                    conversion,
                }
            }
            ParamRole::InEncoded { encode_ops, .. } => {
                let ts_type = param_def
                    .map(|p| emit::ts_type(&p.type_expr))
                    .unwrap_or_else(|| "unknown".to_string());
                let has_codec = param_def
                    .map(|p| matches!(&p.type_expr, TypeExpr::Record(_) | TypeExpr::Enum(_)))
                    .unwrap_or(false);
                let conversion = if has_codec {
                    TsParamConversion::CodecEncoded {
                        codec_name: ts_type.clone(),
                    }
                } else {
                    TsParamConversion::OtherEncoded {
                        encode: encode_ops.clone(),
                    }
                };
                TsParam {
                    name: emit::escape_ts_keyword(&name),
                    ts_type,
                    conversion,
                }
            }
            ParamRole::InCallback { callback_id, .. } => {
                let interface_name = naming::to_upper_camel_case(callback_id.as_str());
                TsParam {
                    name: emit::escape_ts_keyword(&name),
                    ts_type: interface_name.clone(),
                    conversion: TsParamConversion::Callback { interface_name },
                }
            }
            _ => TsParam {
                name: emit::escape_ts_keyword(&name),
                ts_type: "unknown".to_string(),
                conversion: TsParamConversion::Direct,
            },
        }
    }

    fn lower_return(&self, transport: &ReturnTransport) -> (Option<String>, TsReturnAbi, String) {
        match transport {
            ReturnTransport::Void => (None, TsReturnAbi::Void, String::new()),
            ReturnTransport::Direct(abi_type) => {
                let ts_type_str = ts_abi_type(abi_type);
                let cast = ts_direct_cast(abi_type);
                (
                    Some(ts_type_str),
                    TsReturnAbi::Direct { ts_cast: cast },
                    String::new(),
                )
            }
            ReturnTransport::Encoded {
                decode_ops,
                encode_ops: _,
            } => {
                // Fast paths that bypass WireReader for common return types:
                // - Vec<primitive>: uses takePackedXxxArray (Rust uses from_raw_vec, no count prefix)
                // - String: uses takePackedUtf8StringPrefixed (decodes directly from WASM memory)
                let ts_type_str = infer_ts_type_from_read_ops(decode_ops);
                match decode_ops.ops.first() {
                    Some(ReadOp::Vec { element_type: TypeExpr::Primitive(prim), .. }) => {
                        let decode = emit::emit_raw_primitive_array_read(*prim);
                        (Some(ts_type_str), TsReturnAbi::RawPacked { decode_expr: decode }, String::new())
                    }
                    Some(ReadOp::String { .. }) => {
                        let decode = "_module.takePackedUtf8String(packed)".to_string();
                        (Some(ts_type_str), TsReturnAbi::RawPacked { decode_expr: decode }, String::new())
                    }
                    _ => {
                        let decode = emit::emit_reader_read(decode_ops);
                        (Some(ts_type_str), TsReturnAbi::Packed, decode)
                    }
                }
            }
            ReturnTransport::Handle { class_id, nullable } => {
                let class_name = naming::to_upper_camel_case(class_id.as_str());
                let ts_type_str = if *nullable {
                    format!("{} | null", class_name)
                } else {
                    class_name
                };
                (
                    Some(ts_type_str),
                    TsReturnAbi::Direct {
                        ts_cast: String::new(),
                    },
                    String::new(),
                )
            }
            ReturnTransport::Callback { .. } => (
                Some("unknown".to_string()),
                TsReturnAbi::Void,
                String::new(),
            ),
        }
    }

    fn lower_error(&self, transport: &ErrorTransport) -> (bool, String) {
        match transport {
            ErrorTransport::None => (false, String::new()),
            ErrorTransport::StatusCode => (true, "FfiError".to_string()),
            ErrorTransport::Encoded { decode_ops, .. } => {
                let err_type = infer_ts_type_from_read_ops(decode_ops);
                (true, err_type)
            }
        }
    }

    fn lower_async_result(&self, result: &AsyncResultTransport) -> (Option<String>, String) {
        match result {
            AsyncResultTransport::Void => (None, String::new()),
            AsyncResultTransport::Direct(abi_type) => {
                let ts_type = ts_abi_type(abi_type);
                let read_method = match abi_type {
                    AbiType::Bool => "reader.readBool()",
                    AbiType::I8 => "reader.readI8()",
                    AbiType::U8 => "reader.readU8()",
                    AbiType::I16 => "reader.readI16()",
                    AbiType::U16 => "reader.readU16()",
                    AbiType::I32 => "reader.readI32()",
                    AbiType::U32 => "reader.readU32()",
                    AbiType::I64 => "reader.readI64()",
                    AbiType::U64 => "reader.readU64()",
                    AbiType::ISize => "reader.readISize()",
                    AbiType::USize => "reader.readUSize()",
                    AbiType::F32 => "reader.readF32()",
                    AbiType::F64 => "reader.readF64()",
                    AbiType::Void | AbiType::Pointer => "reader.readI32()",
                };
                (Some(ts_type), read_method.to_string())
            }
            AsyncResultTransport::Encoded { decode_ops, .. } => {
                let ts_type = infer_ts_type_from_read_ops(decode_ops);
                let decode_expr = emit::emit_reader_read(decode_ops);
                (Some(ts_type), decode_expr)
            }
            AsyncResultTransport::Handle { class_id, nullable } => {
                let class_name = naming::to_upper_camel_case(class_id.as_str());
                let ts_type = if *nullable {
                    format!("{} | null", class_name)
                } else {
                    class_name
                };
                (Some(ts_type), "reader.readU32()".to_string())
            }
            AsyncResultTransport::Callback { .. } => (None, String::new()),
        }
    }

    fn collect_wasm_imports(&self, _index: &AbiIndex) -> Vec<TsWasmImport> {
        let mut imports = Vec::new();

        for call in &self.abi.calls {
            if matches!(call.mode, CallMode::Async(_)) {
                continue;
            }

            let wasm_params: Vec<TsWasmParam> = call
                .params
                .iter()
                .map(|p| TsWasmParam {
                    name: emit::escape_ts_keyword(&camel_case(p.name.as_str())),
                    wasm_type: abi_type_to_wasm(&p.ffi_type),
                })
                .collect();

            let return_wasm_type = match &call.return_ {
                ReturnTransport::Void => None,
                ReturnTransport::Direct(abi_type) => Some(abi_type_to_wasm(abi_type)),
                ReturnTransport::Encoded { .. } => Some("bigint".to_string()),
                ReturnTransport::Handle { .. } => Some("number".to_string()),
                ReturnTransport::Callback { .. } => None,
            };

            imports.push(TsWasmImport {
                ffi_name: call.symbol.as_str().to_string(),
                params: wasm_params,
                return_wasm_type,
            });
        }

        imports
    }
}

fn is_excluded_error_type(err_type: &str) -> bool {
    matches!(
        err_type,
        "String" | "string" | "FfiError" | "Error" | "unknown"
    )
}

fn ts_abi_type(abi_type: &AbiType) -> String {
    match abi_type {
        AbiType::Void => "void".to_string(),
        AbiType::Bool => "boolean".to_string(),
        AbiType::I8 | AbiType::U8 | AbiType::I16 | AbiType::U16 => "number".to_string(),
        AbiType::I32 | AbiType::U32 => "number".to_string(),
        AbiType::I64 | AbiType::U64 => "bigint".to_string(),
        AbiType::ISize | AbiType::USize => "number".to_string(),
        AbiType::F32 | AbiType::F64 => "number".to_string(),
        AbiType::Pointer => "number".to_string(),
    }
}

fn ts_direct_cast(abi_type: &AbiType) -> String {
    match abi_type {
        AbiType::Bool => " !== 0".to_string(),
        _ => String::new(),
    }
}

fn abi_type_to_wasm(abi_type: &AbiType) -> String {
    match abi_type {
        AbiType::Void => "void".to_string(),
        AbiType::Bool | AbiType::I8 | AbiType::U8 | AbiType::I16 | AbiType::U16 => {
            "number".to_string()
        }
        AbiType::I32 | AbiType::U32 | AbiType::ISize | AbiType::USize => "number".to_string(),
        AbiType::I64 | AbiType::U64 => "bigint".to_string(),
        AbiType::F32 | AbiType::F64 => "number".to_string(),
        AbiType::Pointer => "number".to_string(),
    }
}

fn callback_primitive_param_kind(
    param_name: &str,
    abi_type: Option<AbiType>,
) -> TsCallbackParamKind {
    let import_ts_type = abi_type
        .as_ref()
        .map(abi_type_to_wasm)
        .unwrap_or_else(|| "number".to_string());
    let call_expr = match abi_type {
        Some(AbiType::Bool) => format!("{param_name} !== 0"),
        _ => param_name.to_string(),
    };

    TsCallbackParamKind::Primitive {
        import_ts_type,
        call_expr,
    }
}

struct DirectWriteInfo {
    method_name: &'static str,
    byte_width: usize,
}

fn direct_write_info(abi_type: &AbiType) -> DirectWriteInfo {
    match abi_type {
        AbiType::Void => DirectWriteInfo {
            method_name: "",
            byte_width: 0,
        },
        AbiType::Bool => DirectWriteInfo {
            method_name: "writeBool",
            byte_width: 1,
        },
        AbiType::I8 => DirectWriteInfo {
            method_name: "writeI8",
            byte_width: 1,
        },
        AbiType::U8 => DirectWriteInfo {
            method_name: "writeU8",
            byte_width: 1,
        },
        AbiType::I16 => DirectWriteInfo {
            method_name: "writeI16",
            byte_width: 2,
        },
        AbiType::U16 => DirectWriteInfo {
            method_name: "writeU16",
            byte_width: 2,
        },
        AbiType::I32 => DirectWriteInfo {
            method_name: "writeI32",
            byte_width: 4,
        },
        AbiType::U32 => DirectWriteInfo {
            method_name: "writeU32",
            byte_width: 4,
        },
        AbiType::I64 => DirectWriteInfo {
            method_name: "writeI64",
            byte_width: 8,
        },
        AbiType::U64 => DirectWriteInfo {
            method_name: "writeU64",
            byte_width: 8,
        },
        AbiType::ISize => DirectWriteInfo {
            method_name: "writeI64",
            byte_width: 8,
        },
        AbiType::USize => DirectWriteInfo {
            method_name: "writeU64",
            byte_width: 8,
        },
        AbiType::F32 => DirectWriteInfo {
            method_name: "writeF32",
            byte_width: 4,
        },
        AbiType::F64 => DirectWriteInfo {
            method_name: "writeF64",
            byte_width: 8,
        },
        AbiType::Pointer => DirectWriteInfo {
            method_name: "writeU32",
            byte_width: 4,
        },
    }
}

fn direct_write_argument_expr(abi_type: &AbiType, value_expr: &str) -> String {
    match abi_type {
        AbiType::ISize | AbiType::USize => format!("BigInt({value_expr})"),
        _ => value_expr.to_string(),
    }
}

fn primitive_buffer_ts_type(abi_type: AbiType) -> String {
    match abi_type {
        AbiType::Bool => "boolean[]".to_string(),
        AbiType::I64 | AbiType::U64 => "bigint[]".to_string(),
        AbiType::I8
        | AbiType::U8
        | AbiType::I16
        | AbiType::U16
        | AbiType::I32
        | AbiType::U32
        | AbiType::ISize
        | AbiType::USize
        | AbiType::F32
        | AbiType::F64 => "number[]".to_string(),
        AbiType::Void | AbiType::Pointer => "unknown[]".to_string(),
    }
}

fn infer_ts_type_from_read_ops(seq: &ReadSeq) -> String {
    seq.ops
        .first()
        .map(|op| match op {
            ReadOp::Primitive { primitive, .. } => emit::ts_primitive(*primitive),
            ReadOp::String { .. } => "string".to_string(),
            ReadOp::Bytes { .. } => "Uint8Array".to_string(),
            ReadOp::Option { some, .. } => {
                format!("{} | null", infer_ts_type_from_read_ops(some))
            }
            ReadOp::Vec { element_type, .. } => {
                if matches!(element_type, TypeExpr::Primitive(PrimitiveType::U8)) {
                    "Uint8Array".to_string()
                } else {
                    format!("{}[]", emit::ts_type(element_type))
                }
            }
            ReadOp::Record { id, .. } => naming::to_upper_camel_case(id.as_str()),
            ReadOp::Enum { id, .. } => naming::to_upper_camel_case(id.as_str()),
            ReadOp::Result { ok, .. } => infer_ts_type_from_read_ops(ok),
            ReadOp::Builtin { id, .. } => emit::ts_builtin(id),
            ReadOp::Custom { underlying, .. } => infer_ts_type_from_read_ops(underlying),
        })
        .unwrap_or_else(|| "void".to_string())
}

fn record_decode_fields(record: &AbiRecord) -> HashMap<FieldName, ReadSeq> {
    record
        .decode_ops
        .ops
        .iter()
        .find_map(|op| match op {
            ReadOp::Record { fields, .. } => Some(fields),
            _ => None,
        })
        .into_iter()
        .flat_map(|fields| {
            fields
                .iter()
                .map(|field| (field.name.clone(), field.seq.clone()))
        })
        .collect()
}

fn record_encode_fields(record: &AbiRecord) -> HashMap<FieldName, WriteSeq> {
    record
        .encode_ops
        .ops
        .iter()
        .find_map(|op| match op {
            WriteOp::Record { fields, .. } => Some(fields),
            _ => None,
        })
        .into_iter()
        .flat_map(|fields| {
            fields
                .iter()
                .map(|field| (field.name.clone(), field.seq.clone()))
        })
        .collect()
}

fn remap_named_to_field(seq: &WriteSeq) -> WriteSeq {
    WriteSeq {
        size: remap_named_in_size(&seq.size),
        ops: seq.ops.iter().map(remap_named_in_write_op).collect(),
        shape: seq.shape,
    }
}

fn remap_named_in_value(expr: &ValueExpr) -> ValueExpr {
    match expr {
        ValueExpr::Named(name) => ValueExpr::Instance.field(FieldName::new(name)),
        ValueExpr::Field(parent, field) => {
            ValueExpr::Field(Box::new(remap_named_in_value(parent)), field.clone())
        }
        other => other.clone(),
    }
}

fn remap_named_in_size(size: &SizeExpr) -> SizeExpr {
    match size {
        SizeExpr::StringLen(v) => SizeExpr::StringLen(remap_named_in_value(v)),
        SizeExpr::BytesLen(v) => SizeExpr::BytesLen(remap_named_in_value(v)),
        SizeExpr::ValueSize(v) => SizeExpr::ValueSize(remap_named_in_value(v)),
        SizeExpr::WireSize { value, record_id } => SizeExpr::WireSize {
            value: remap_named_in_value(value),
            record_id: record_id.clone(),
        },
        SizeExpr::BuiltinSize { id, value } => SizeExpr::BuiltinSize {
            id: id.clone(),
            value: remap_named_in_value(value),
        },
        SizeExpr::Sum(parts) => SizeExpr::Sum(parts.iter().map(remap_named_in_size).collect()),
        SizeExpr::OptionSize { value, inner } => SizeExpr::OptionSize {
            value: remap_named_in_value(value),
            inner: Box::new(remap_named_in_size(inner)),
        },
        SizeExpr::VecSize {
            value,
            inner,
            layout,
        } => SizeExpr::VecSize {
            value: remap_named_in_value(value),
            inner: Box::new(remap_named_in_size(inner)),
            layout: layout.clone(),
        },
        SizeExpr::ResultSize { value, ok, err } => SizeExpr::ResultSize {
            value: remap_named_in_value(value),
            ok: Box::new(remap_named_in_size(ok)),
            err: Box::new(remap_named_in_size(err)),
        },
        other => other.clone(),
    }
}

fn remap_named_in_write_op(op: &WriteOp) -> WriteOp {
    match op {
        WriteOp::Primitive { primitive, value } => WriteOp::Primitive {
            primitive: *primitive,
            value: remap_named_in_value(value),
        },
        WriteOp::String { value } => WriteOp::String {
            value: remap_named_in_value(value),
        },
        WriteOp::Bytes { value } => WriteOp::Bytes {
            value: remap_named_in_value(value),
        },
        WriteOp::Option { value, some } => WriteOp::Option {
            value: remap_named_in_value(value),
            some: Box::new(remap_named_to_field(some)),
        },
        WriteOp::Vec {
            value,
            element_type,
            element,
            layout,
        } => WriteOp::Vec {
            value: remap_named_in_value(value),
            element_type: element_type.clone(),
            element: Box::new(remap_named_to_field(element)),
            layout: layout.clone(),
        },
        WriteOp::Record { id, value, fields } => WriteOp::Record {
            id: id.clone(),
            value: remap_named_in_value(value),
            fields: fields
                .iter()
                .map(|f| FieldWriteOp {
                    name: f.name.clone(),
                    accessor: remap_named_in_value(&f.accessor),
                    seq: remap_named_to_field(&f.seq),
                })
                .collect(),
        },
        WriteOp::Enum { id, value, layout } => WriteOp::Enum {
            id: id.clone(),
            value: remap_named_in_value(value),
            layout: layout.clone(),
        },
        WriteOp::Result { value, ok, err } => WriteOp::Result {
            value: remap_named_in_value(value),
            ok: Box::new(remap_named_to_field(ok)),
            err: Box::new(remap_named_to_field(err)),
        },
        WriteOp::Builtin { id, value } => WriteOp::Builtin {
            id: id.clone(),
            value: remap_named_in_value(value),
        },
        WriteOp::Custom {
            id,
            value,
            underlying,
        } => WriteOp::Custom {
            id: id.clone(),
            value: remap_named_in_value(value),
            underlying: Box::new(remap_named_to_field(underlying)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::{FfiContract, PackageInfo};
    use crate::ir::definitions::{
        ClassDef, ConstructorDef, FunctionDef, MethodDef, ParamDef, ParamPassing, Receiver,
        ReturnDef,
    };
    use crate::ir::ids::{ClassId, FunctionId, MethodId, ParamName};

    fn empty_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            catalog: Default::default(),
            functions: vec![],
        }
    }

    fn primitive_param(name: &str, primitive: PrimitiveType) -> ParamDef {
        ParamDef {
            name: ParamName::new(name),
            type_expr: TypeExpr::Primitive(primitive),
            passing: ParamPassing::Value,
            doc: None,
        }
    }

    fn vec_param(name: &str, primitive: PrimitiveType) -> ParamDef {
        ParamDef {
            name: ParamName::new(name),
            type_expr: TypeExpr::Vec(Box::new(TypeExpr::Primitive(primitive))),
            passing: ParamPassing::Value,
            doc: None,
        }
    }

    fn function(
        name: &str,
        params: Vec<ParamDef>,
        returns: ReturnDef,
        is_async: bool,
    ) -> FunctionDef {
        FunctionDef {
            id: FunctionId::new(name),
            params,
            returns,
            is_async,
            doc: None,
            deprecated: None,
        }
    }

    fn lower_contract(contract: &FfiContract) -> TsModule {
        let abi = IrLowerer::new(contract).to_abi_contract();
        TypeScriptLowerer::new(contract, &abi, "Test".to_string()).lower()
    }

    fn class_with_sync_and_async_methods() -> ClassDef {
        ClassDef {
            id: ClassId::new("Counter"),
            constructors: vec![ConstructorDef::Default {
                params: vec![],
                is_fallible: false,
                doc: None,
                deprecated: None,
            }],
            methods: vec![
                MethodDef {
                    id: MethodId::new("increment"),
                    receiver: Receiver::RefSelf,
                    params: vec![primitive_param("delta", PrimitiveType::I32)],
                    returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                    is_async: false,
                    doc: None,
                    deprecated: None,
                },
                MethodDef {
                    id: MethodId::new("next_value"),
                    receiver: Receiver::RefSelf,
                    params: vec![],
                    returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                    is_async: true,
                    doc: None,
                    deprecated: None,
                },
            ],
            streams: vec![],
            doc: None,
            deprecated: None,
        }
    }

    #[test]
    fn wasm_import_string_return_uses_packed_bigint() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "echo_name",
            vec![primitive_param("count", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::String),
            false,
        ));

        let module = lower_contract(&contract);
        let import = module
            .wasm_imports
            .iter()
            .find(|import| import.ffi_name == "boltffi_echo_name")
            .expect("wasm import for string return");

        assert_eq!(import.return_wasm_type, Some("bigint".to_string()));
        assert_eq!(import.params.len(), 1);
        assert_eq!(import.params[0].name, "count");
        assert_eq!(import.params[0].wasm_type, "number");
    }

    #[test]
    fn wasm_import_direct_return_does_not_insert_out_param() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "add",
            vec![
                primitive_param("left", PrimitiveType::I32),
                primitive_param("right", PrimitiveType::I32),
            ],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            false,
        ));

        let module = lower_contract(&contract);
        let import = module
            .wasm_imports
            .iter()
            .find(|import| import.ffi_name == "boltffi_add")
            .expect("wasm import for direct return");

        assert_eq!(import.return_wasm_type.as_deref(), Some("number"));
        assert_eq!(
            import
                .params
                .iter()
                .map(|param| param.name.as_str())
                .collect::<Vec<_>>(),
            vec!["left", "right"]
        );
    }

    #[test]
    fn wasm_imports_skip_async_calls() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "sync_value",
            vec![],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            false,
        ));
        contract.functions.push(function(
            "async_value",
            vec![],
            ReturnDef::Value(TypeExpr::String),
            true,
        ));

        let module = lower_contract(&contract);

        assert_eq!(module.wasm_imports.len(), 1);
        assert_eq!(module.wasm_imports[0].ffi_name, "boltffi_sync_value");
    }

    #[test]
    fn class_instance_methods_exclude_receiver_from_public_params() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_class(class_with_sync_and_async_methods());

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|class| class.class_name == "Counter")
            .expect("class should be lowered");
        let method = class
            .methods
            .iter()
            .find(|method| method.ts_name == "increment")
            .expect("instance method should be lowered");

        assert_eq!(method.params.len(), 1);
        assert_eq!(method.params[0].name, "delta");
        assert_eq!(method.ffi_call_args(), "this._handle, delta");
    }

    #[test]
    fn class_async_methods_generate_wasm_poll_sync_symbol_names() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_class(class_with_sync_and_async_methods());

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|class| class.class_name == "Counter")
            .expect("class should be lowered");
        let method = class
            .methods
            .iter()
            .find(|method| method.ts_name == "nextValue")
            .expect("async method should be lowered");

        match &method.mode {
            TsClassMethodMode::Async(async_method) => {
                assert_eq!(method.ffi_name, "boltffi_counter_next_value");
                assert_eq!(
                    async_method.poll_sync_ffi_name,
                    "boltffi_counter_next_value_poll_sync"
                );
                assert_eq!(
                    async_method.complete_ffi_name,
                    "boltffi_counter_next_value_complete"
                );
                assert_eq!(
                    async_method.panic_message_ffi_name,
                    "boltffi_counter_next_value_panic_message"
                );
            }
            TsClassMethodMode::Sync(_) => panic!("expected async class method mode"),
        }
    }

    #[test]
    fn vec_i32_param_uses_number_array_conversion() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "process_values",
            vec![vec_param("values", PrimitiveType::I32)],
            ReturnDef::Void,
            false,
        ));

        let module = lower_contract(&contract);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "processValues")
            .expect("function should be lowered");
        let param = function
            .params
            .iter()
            .find(|param| param.name == "values")
            .expect("vec parameter should exist");

        assert_eq!(param.ts_type, "number[]");
        assert!(matches!(
            param.conversion,
            TsParamConversion::PrimitiveBuffer {
                element_abi: AbiType::I32
            }
        ));
    }

    #[test]
    fn vec_i32_param_builds_primitive_buffer_wrapper_sequence() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "process_values",
            vec![vec_param("values", PrimitiveType::I32)],
            ReturnDef::Void,
            false,
        ));

        let module = lower_contract(&contract);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "processValues")
            .expect("function should be lowered");
        let param = function
            .params
            .iter()
            .find(|param| param.name == "values")
            .expect("vec parameter should exist");

        assert_eq!(
            param.wrapper_code(),
            Some("const values_alloc = _module.allocPrimitiveBuffer(values, \"i32\");".to_string())
        );
        assert_eq!(
            param.ffi_args(),
            vec![
                "values_alloc.ptr".to_string(),
                "values_alloc.len".to_string()
            ]
        );
        assert_eq!(
            param.cleanup_code(),
            Some("_module.freePrimitiveBuffer(values_alloc);".to_string())
        );
    }

    #[test]
    fn vec_u8_param_remains_uint8_array() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "process_bytes",
            vec![vec_param("values", PrimitiveType::U8)],
            ReturnDef::Void,
            false,
        ));

        let module = lower_contract(&contract);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "processBytes")
            .expect("function should be lowered");
        let param = function
            .params
            .iter()
            .find(|param| param.name == "values")
            .expect("vec parameter should exist");

        assert_eq!(param.ts_type, "Uint8Array");
        assert!(matches!(param.conversion, TsParamConversion::Bytes));
        assert_eq!(
            param.wrapper_code(),
            Some("const values_alloc = _module.allocBytes(values);".to_string())
        );
        assert_eq!(
            param.ffi_args(),
            vec![
                "values_alloc.ptr".to_string(),
                "values_alloc.len".to_string()
            ]
        );
        assert_eq!(
            param.cleanup_code(),
            Some("_module.freeAlloc(values_alloc);".to_string())
        );
    }

    #[test]
    fn direct_write_info_uses_bool_writer_for_bool() {
        let info = direct_write_info(&AbiType::Bool);
        assert_eq!(info.method_name, "writeBool");
        assert_eq!(info.byte_width, 1);
        assert_eq!(
            direct_write_argument_expr(&AbiType::Bool, "result"),
            "result"
        );
    }

    #[test]
    fn direct_write_info_uses_eight_bytes_for_isize_and_usize() {
        let isize_info = direct_write_info(&AbiType::ISize);
        let usize_info = direct_write_info(&AbiType::USize);
        assert_eq!(isize_info.method_name, "writeI64");
        assert_eq!(isize_info.byte_width, 8);
        assert_eq!(usize_info.method_name, "writeU64");
        assert_eq!(usize_info.byte_width, 8);
    }

    #[test]
    fn direct_write_argument_expr_casts_pointer_sized_scalars_to_bigint() {
        assert_eq!(
            direct_write_argument_expr(&AbiType::ISize, "result"),
            "BigInt(result)"
        );
        assert_eq!(
            direct_write_argument_expr(&AbiType::USize, "result"),
            "BigInt(result)"
        );
    }

    #[test]
    fn callback_primitive_param_kind_uses_bigint_for_i64() {
        let kind = callback_primitive_param_kind("count", Some(AbiType::I64));
        match kind {
            TsCallbackParamKind::Primitive {
                import_ts_type,
                call_expr,
            } => {
                assert_eq!(import_ts_type, "bigint");
                assert_eq!(call_expr, "count");
            }
            TsCallbackParamKind::WireEncoded { .. } => {
                panic!("expected primitive callback param kind")
            }
        }
    }

    #[test]
    fn callback_primitive_param_kind_coerces_bool_to_boolean_expression() {
        let kind = callback_primitive_param_kind("isActive", Some(AbiType::Bool));
        match kind {
            TsCallbackParamKind::Primitive {
                import_ts_type,
                call_expr,
            } => {
                assert_eq!(import_ts_type, "number");
                assert_eq!(call_expr, "isActive !== 0");
            }
            TsCallbackParamKind::WireEncoded { .. } => {
                panic!("expected primitive callback param kind")
            }
        }
    }

    #[test]
    fn class_constructor_generates_correct_ffi_name_and_params() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Counter"),
            constructors: vec![ConstructorDef::Default {
                params: vec![primitive_param("initial", PrimitiveType::I32)],
                is_fallible: false,
                doc: None,
                deprecated: None,
            }],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Counter")
            .expect("class should be lowered");

        assert_eq!(class.constructors.len(), 1);
        let constructor = &class.constructors[0];
        assert_eq!(constructor.ffi_name, "boltffi_counter_new");
        assert_eq!(constructor.params.len(), 1);
        assert_eq!(constructor.params[0].name, "initial");
    }

    #[test]
    fn class_named_factory_constructor_uses_name_in_ffi() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Connection"),
            constructors: vec![
                ConstructorDef::Default {
                    params: vec![],
                    is_fallible: false,
                    doc: None,
                    deprecated: None,
                },
                ConstructorDef::NamedFactory {
                    name: MethodId::new("connect"),
                    is_fallible: false,
                    doc: None,
                    deprecated: None,
                },
            ],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Connection")
            .expect("class should be lowered");

        assert_eq!(class.constructors.len(), 2);
        assert_eq!(class.constructors[0].ffi_name, "boltffi_connection_new");
        assert_eq!(class.constructors[0].ts_name, "new");
        assert_eq!(class.constructors[1].ffi_name, "boltffi_connection_connect");
        assert_eq!(class.constructors[1].ts_name, "connect");
    }

    #[test]
    fn class_static_method_excludes_handle_from_ffi_args() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Factory"),
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("create_item"),
                receiver: Receiver::Static,
                params: vec![primitive_param("id", PrimitiveType::I32)],
                returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                is_async: false,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Factory")
            .expect("class should be lowered");

        let method = &class.methods[0];
        assert!(method.is_static);
        assert_eq!(method.ffi_call_args(), "id");
    }

    #[test]
    fn class_ref_mut_self_method_passes_handle_same_as_ref_self() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Buffer"),
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("push"),
                receiver: Receiver::RefMutSelf,
                params: vec![primitive_param("value", PrimitiveType::I32)],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Buffer")
            .expect("class should be lowered");

        let method = &class.methods[0];
        assert!(!method.is_static);
        assert_eq!(method.ffi_call_args(), "this._handle, value");
    }

    #[test]
    fn class_async_method_with_params_includes_handle_in_entry_ffi() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Database"),
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("query"),
                receiver: Receiver::RefSelf,
                params: vec![ParamDef {
                    name: ParamName::new("sql"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Value(TypeExpr::String),
                is_async: true,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Database")
            .expect("class should be lowered");

        let method = &class.methods[0];
        assert_eq!(method.ffi_name, "boltffi_database_query");
        assert!(!method.is_static);
        assert!(method.params.iter().any(|p| p.name == "sql"));
        assert!(!method.params.iter().any(|p| p.name == "handle"));
    }

    #[test]
    fn class_ffi_free_uses_correct_naming() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Resource"),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Resource")
            .expect("class should be lowered");

        assert_eq!(class.ffi_free, "boltffi_resource_free");
    }

    #[test]
    fn wasm_import_escapes_reserved_keyword_params() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "use_default",
            vec![primitive_param("default", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            false,
        ));

        let module = lower_contract(&contract);
        let import = module
            .wasm_imports
            .iter()
            .find(|i| i.ffi_name == "boltffi_use_default")
            .expect("wasm import should exist");

        assert_eq!(import.params[0].name, "default_");
    }

    #[test]
    fn class_async_method_with_mut_self_generates_correct_ffi_structure() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Counter"),
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("increment_async"),
                receiver: Receiver::RefMutSelf,
                params: vec![primitive_param("amount", PrimitiveType::I32)],
                returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                is_async: true,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Counter")
            .expect("class should be lowered");

        let method = &class.methods[0];
        assert_eq!(method.ts_name, "incrementAsync");
        assert!(!method.is_static);
        assert_eq!(method.params.len(), 1);
        assert_eq!(method.params[0].name, "amount");

        match &method.mode {
            TsClassMethodMode::Async(async_method) => {
                assert_eq!(
                    async_method.poll_sync_ffi_name,
                    "boltffi_counter_increment_async_poll_sync"
                );
            }
            TsClassMethodMode::Sync(_) => panic!("expected async method mode"),
        }
    }

    #[test]
    fn class_lowering_generates_pascal_case_class_name() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("http_client"),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "HttpClient")
            .expect("class should be lowered with PascalCase name");

        assert_eq!(class.ffi_free, "boltffi_http_client_free");
    }

    #[test]
    fn class_method_with_string_param_uses_string_conversion() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Logger"),
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("log"),
                receiver: Receiver::RefSelf,
                params: vec![ParamDef {
                    name: ParamName::new("message"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Logger")
            .expect("class should be lowered");

        let method = &class.methods[0];
        assert_eq!(method.params[0].name, "message");
        assert_eq!(method.params[0].ts_type, "string");
        assert!(matches!(
            method.params[0].conversion,
            TsParamConversion::String
        ));
    }

    #[test]
    fn class_constructor_with_string_param_generates_wrapper_and_cleanup() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Connection"),
            constructors: vec![ConstructorDef::Default {
                params: vec![ParamDef {
                    name: ParamName::new("url"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                is_fallible: false,
                doc: None,
                deprecated: None,
            }],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Connection")
            .expect("class should be lowered");

        let ctor = &class.constructors[0];
        assert_eq!(ctor.params[0].name, "url");
        assert!(ctor.params[0].wrapper_code().is_some());
        assert!(ctor.params[0].cleanup_code().is_some());
    }

    #[test]
    fn class_method_returns_none_for_void_return_type() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Printer"),
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("print"),
                receiver: Receiver::RefSelf,
                params: vec![],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Printer")
            .expect("class should be lowered");

        let method = &class.methods[0];
        assert!(method.return_type.is_none());
        match &method.mode {
            TsClassMethodMode::Sync(sync) => {
                assert!(sync.return_abi.is_void());
            }
            _ => panic!("expected sync method"),
        }
    }

    #[test]
    fn class_method_with_record_param_uses_codec_conversion() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(crate::ir::definitions::RecordDef {
                id: crate::ir::ids::RecordId::new("Point"),
                fields: vec![
                    crate::ir::definitions::FieldDef {
                        name: FieldName::new("x"),
                        type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                        doc: None,
                        default: None,
                    },
                    crate::ir::definitions::FieldDef {
                        name: FieldName::new("y"),
                        type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                        doc: None,
                        default: None,
                    },
                ],
                doc: None,
                deprecated: None,
            });
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Canvas"),
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("draw_point"),
                receiver: Receiver::RefSelf,
                params: vec![ParamDef {
                    name: ParamName::new("point"),
                    type_expr: TypeExpr::Record(crate::ir::ids::RecordId::new("Point")),
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Canvas")
            .expect("class should be lowered");

        let method = &class.methods[0];
        assert_eq!(method.params[0].name, "point");
        assert_eq!(method.params[0].ts_type, "Point");
        match &method.params[0].conversion {
            TsParamConversion::CodecEncoded { codec_name } => {
                assert_eq!(codec_name, "Point");
            }
            _ => panic!("expected codec encoded conversion"),
        }
    }

    #[test]
    fn class_sync_method_with_direct_return_has_correct_abi() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Counter"),
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("get"),
                receiver: Receiver::RefSelf,
                params: vec![],
                returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                is_async: false,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Counter")
            .expect("class should be lowered");

        let method = &class.methods[0];
        assert_eq!(method.return_type.as_deref(), Some("number"));
        match &method.mode {
            TsClassMethodMode::Sync(sync) => {
                assert!(sync.return_abi.is_direct());
            }
            _ => panic!("expected sync method"),
        }
    }

    #[test]
    fn class_method_ts_name_uses_camel_case() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Service"),
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("get_user_by_id"),
                receiver: Receiver::RefSelf,
                params: vec![primitive_param("user_id", PrimitiveType::I32)],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Service")
            .expect("class should be lowered");

        let method = &class.methods[0];
        assert_eq!(method.ts_name, "getUserById");
        assert_eq!(method.params[0].name, "userId");
    }

    #[test]
    fn wasm_import_bigint_params_use_bigint_type() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "process_large",
            vec![primitive_param("value", PrimitiveType::I64)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I64)),
            false,
        ));

        let module = lower_contract(&contract);
        let import = module
            .wasm_imports
            .iter()
            .find(|i| i.ffi_name == "boltffi_process_large")
            .expect("wasm import should exist");

        assert_eq!(import.params[0].wasm_type, "bigint");
        assert_eq!(import.return_wasm_type.as_deref(), Some("bigint"));
    }

    #[test]
    fn class_async_method_all_ffi_names_follow_convention() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("NetworkClient"),
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("fetch_data"),
                receiver: Receiver::RefSelf,
                params: vec![],
                returns: ReturnDef::Value(TypeExpr::String),
                is_async: true,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let class = module
            .classes
            .iter()
            .find(|c| c.class_name == "NetworkClient")
            .expect("class should be lowered");

        let method = &class.methods[0];
        assert_eq!(method.ffi_name, "boltffi_network_client_fetch_data");

        match &method.mode {
            TsClassMethodMode::Async(async_method) => {
                assert_eq!(
                    async_method.poll_sync_ffi_name,
                    "boltffi_network_client_fetch_data_poll_sync"
                );
                assert_eq!(
                    async_method.complete_ffi_name,
                    "boltffi_network_client_fetch_data_complete"
                );
                assert_eq!(
                    async_method.panic_message_ffi_name,
                    "boltffi_network_client_fetch_data_panic_message"
                );
                assert_eq!(
                    async_method.cancel_ffi_name,
                    "boltffi_network_client_fetch_data_cancel"
                );
                assert_eq!(
                    async_method.free_ffi_name,
                    "boltffi_network_client_fetch_data_free"
                );
            }
            _ => panic!("expected async method"),
        }
    }
}
