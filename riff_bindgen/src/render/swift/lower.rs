use heck::ToLowerCamelCase;
use riff_ffi_rules::naming::{
    self, snake_to_camel as camel_case, to_upper_camel_case as pascal_case,
};

use std::collections::HashMap;

use super::emit;
use super::plan::{
    SwiftAsyncConversion, SwiftAsyncResult, SwiftCallMode, SwiftCallback, SwiftCallbackMethod,
    SwiftCallbackParam, SwiftClass, SwiftClosureTrampoline, SwiftClosureTrampolineParam,
    SwiftConstructor, SwiftConversion, SwiftEnum, SwiftEnumStyle, SwiftField, SwiftFunction,
    SwiftMethod, SwiftModule, SwiftParam, SwiftRecord, SwiftReturn, SwiftStream, SwiftStreamMode,
    SwiftVariant, SwiftVariantPayload,
};
use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiContract, AbiEnum, AbiEnumField, AbiEnumPayload,
    AbiEnumVariant, AbiParam, AbiRecord, AbiStream, AsyncResultTransport, CallId, CallMode,
    ErrorTransport, ParamRole, ReturnTransport, StreamItemTransport,
};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{
    CallbackKind, ConstructorDef, ParamDef, Receiver, ReturnDef, StreamDef, StreamMode,
};
use crate::ir::ids::{CallbackId, ClassId, EnumId, FieldName, ParamName, RecordId};
use crate::ir::ops::{
    FieldReadOp, OffsetExpr, ReadOp, ReadSeq, SizeExpr, ValueExpr, WireShape, WriteOp, WriteSeq,
    remap_root_in_seq,
};
use crate::ir::plan::AbiType;
use crate::ir::plan::{CallbackStyle, Mutability};
use crate::ir::types::TypeExpr;

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
            .map(|(index, callback)| (callback.callback_id.clone(), index))
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

    fn call<'a>(&self, contract: &'a AbiContract, id: &CallId) -> &'a AbiCall {
        let index = self.calls.get(id).expect("abi call should exist");
        &contract.calls[*index]
    }

    fn callback<'a>(
        &self,
        contract: &'a AbiContract,
        id: &CallbackId,
    ) -> &'a AbiCallbackInvocation {
        let index = self.callbacks.get(id).expect("abi callback should exist");
        &contract.callbacks[*index]
    }

    fn record<'a>(&self, contract: &'a AbiContract, id: &RecordId) -> &'a AbiRecord {
        let index = self.records.get(id).expect("record should exist");
        &contract.records[*index]
    }

    fn enumeration<'a>(&self, contract: &'a AbiContract, id: &EnumId) -> &'a AbiEnum {
        let index = self.enums.get(id).expect("enum should exist");
        &contract.enums[*index]
    }
}

pub struct SwiftLowerer<'a> {
    contract: &'a FfiContract,
    abi: &'a AbiContract,
    abi_index: AbiIndex,
}

// ─────────────────────────────────────────────────────────────────────────────
// Construction
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    pub fn new(contract: &'a FfiContract, abi: &'a AbiContract) -> Self {
        Self {
            contract,
            abi,
            abi_index: AbiIndex::new(abi),
        }
    }

    pub fn lower(self) -> SwiftModule {
        let records = self.lower_records();
        let enums = self.lower_enums();
        let classes = self.lower_classes();
        let callbacks = self.lower_callbacks();
        let functions = self.lower_functions();

        SwiftModule {
            imports: vec!["Foundation".to_string()],
            records,
            enums,
            classes,
            callbacks,
            functions,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Records
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_records(&self) -> Vec<SwiftRecord> {
        self.contract
            .catalog
            .all_records()
            .map(|def| {
                let abi_record = self.abi_index.record(self.abi, &def.id);
                let decode_fields = self.record_decode_fields(abi_record);
                let encode_fields = self.record_encode_fields(abi_record);
                let fields =
                    def.fields
                        .iter()
                        .map(|field| {
                            let swift_name = camel_case(field.name.as_str());
                            let decode =
                                decode_fields.get(&field.name).cloned().unwrap_or_else(|| {
                                    ReadSeq {
                                        size: SizeExpr::Fixed(0),
                                        ops: vec![],
                                        shape: WireShape::Value,
                                    }
                                });
                            let encode =
                                encode_fields.get(&field.name).cloned().unwrap_or_else(|| {
                                    WriteSeq {
                                        size: SizeExpr::Fixed(0),
                                        ops: vec![],
                                        shape: WireShape::Value,
                                    }
                                });
                            let c_offset = if abi_record.is_blittable {
                                decode_fields
                                    .get(&field.name)
                                    .and_then(|seq| self.record_field_offset(seq))
                            } else {
                                None
                            };
                            SwiftField {
                                swift_name,
                                swift_type: emit::swift_type(&field.type_expr),
                                default_expr: None,
                                decode,
                                encode,
                                doc: field.doc.clone(),
                                c_offset,
                            }
                        })
                        .collect();

                SwiftRecord {
                    class_name: self.swift_name_for_record(&def.id),
                    fields,
                    is_blittable: abi_record.is_blittable,
                    blittable_size: abi_record.size,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Enums
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_enums(&self) -> Vec<SwiftEnum> {
        self.contract
            .catalog
            .all_enums()
            .map(|def| {
                let abi_enum = self.abi_index.enumeration(self.abi, &def.id);
                let variant_docs = def.variant_docs();
                let variants = abi_enum
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(i, variant)| SwiftVariant {
                        swift_name: variant.name.as_str().to_lower_camel_case(),
                        discriminant: variant.discriminant,
                        payload: self.lower_variant_payload(variant),
                        doc: variant_docs.get(i).cloned().flatten(),
                    })
                    .collect();

                let style = if abi_enum.is_c_style {
                    SwiftEnumStyle::CStyle
                } else {
                    SwiftEnumStyle::Data
                };
                SwiftEnum {
                    name: self.swift_name_for_enum(&def.id),
                    variants,
                    style,
                    is_error: def.is_error,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }

    fn lower_variant_payload(&self, variant: &AbiEnumVariant) -> SwiftVariantPayload {
        match &variant.payload {
            AbiEnumPayload::Unit => SwiftVariantPayload::Unit,
            // we match tuple variants as case .foo(let value) so we get one
            // binding for everything and need to remap all fields to encode
            // from value instead of individual names
            AbiEnumPayload::Tuple(fields) => SwiftVariantPayload::Tuple(
                fields
                    .iter()
                    .map(|field| {
                        let lowered = self.lower_enum_field(field);
                        let encode =
                            remap_root_in_seq(&lowered.encode, ValueExpr::Var("value".into()));
                        SwiftField { encode, ..lowered }
                    })
                    .collect(),
            ),
            // if the field name is just 0 or 1 we dont have a real name to
            // bind in the switch so we treat it as value same as tuples
            AbiEnumPayload::Struct(fields) => SwiftVariantPayload::Struct(
                fields
                    .iter()
                    .map(|field| {
                        let lowered = self.lower_enum_field(field);
                        if lowered.swift_name.chars().all(|c| c.is_ascii_digit()) {
                            let encode =
                                remap_root_in_seq(&lowered.encode, ValueExpr::Var("value".into()));
                            SwiftField { encode, ..lowered }
                        } else {
                            lowered
                        }
                    })
                    .collect(),
            ),
        }
    }

    fn lower_enum_field(&self, field: &AbiEnumField) -> SwiftField {
        let swift_name = camel_case(field.name.as_str());
        let encode = field.encode.clone();
        SwiftField {
            swift_name,
            swift_type: emit::swift_type(&field.type_expr),
            default_expr: None,
            decode: field.decode.clone(),
            encode,
            doc: None,
            c_offset: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Classes
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_classes(&self) -> Vec<SwiftClass> {
        self.contract
            .catalog
            .all_classes()
            .map(|def| {
                let class_name = self.swift_name_for_class(&def.id);
                let ffi_free = naming::class_ffi_free(def.id.as_str()).to_string();

                let constructors = def
                    .constructors
                    .iter()
                    .enumerate()
                    .map(|(idx, ctor)| {
                        let call = self.abi_call(&CallId::Constructor {
                            class_id: def.id.clone(),
                            index: idx,
                        });

                        match ctor {
                            ConstructorDef::Default {
                                is_fallible, doc, ..
                            } => SwiftConstructor::Designated {
                                ffi_symbol: call.symbol.as_str().to_string(),
                                params: ctor
                                    .params()
                                    .into_iter()
                                    .map(|p| self.lower_param(p, call))
                                    .collect(),
                                is_fallible: *is_fallible,
                                doc: doc.clone(),
                            },
                            ConstructorDef::NamedFactory {
                                name,
                                is_fallible,
                                doc,
                                ..
                            } => SwiftConstructor::Factory {
                                name: camel_case(name.as_str()),
                                ffi_symbol: call.symbol.as_str().to_string(),
                                is_fallible: *is_fallible,
                                doc: doc.clone(),
                            },
                            ConstructorDef::NamedInit {
                                name,
                                first_param,
                                rest_params,
                                is_fallible,
                                doc,
                                ..
                            } => {
                                let label = camel_case(name.as_str());
                                let mut first = self.lower_param(first_param, call);
                                first.label = Some(label.clone());
                                let rest = rest_params.iter().map(|p| self.lower_param(p, call));
                                SwiftConstructor::Convenience {
                                    name: label,
                                    ffi_symbol: call.symbol.as_str().to_string(),
                                    params: std::iter::once(first).chain(rest).collect(),
                                    is_fallible: *is_fallible,
                                    doc: doc.clone(),
                                }
                            }
                        }
                    })
                    .collect();

                let methods =
                    def.methods
                        .iter()
                        .map(|method| {
                            let call = self.abi_call(&CallId::Method {
                                class_id: def.id.clone(),
                                method_id: method.id.clone(),
                            });
                            let mode = self.lower_call_mode(call, &method.returns);
                            // we get the actual value back through poll/complete for async
                            // so all we set up here is the error wrapping
                            let returns = match &call.mode {
                                CallMode::Async(async_call) => self
                                    .lower_return_def_for_async(&async_call.error, &method.returns),
                                CallMode::Sync => self.swift_return_from_abi(
                                    &call.return_,
                                    &call.error,
                                    &method.returns,
                                ),
                            };

                            SwiftMethod {
                                name: camel_case(method.id.as_str()),
                                mode,
                                params: method
                                    .params
                                    .iter()
                                    .map(|p| self.lower_param(p, call))
                                    .collect(),
                                returns,
                                is_static: method.receiver == Receiver::Static,
                                doc: method.doc.clone(),
                            }
                        })
                        .collect();

                let streams = def
                    .streams
                    .iter()
                    .map(|stream_def| {
                        let abi_stream = self
                            .abi
                            .streams
                            .iter()
                            .find(|stream| {
                                stream.class_id == def.id && stream.stream_id == stream_def.id
                            })
                            .expect("abi stream");
                        self.lower_stream(stream_def, abi_stream, &class_name)
                    })
                    .collect();

                SwiftClass {
                    name: class_name,
                    ffi_free,
                    constructors,
                    methods,
                    streams,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }

    fn lower_stream(
        &self,
        stream_def: &StreamDef,
        stream: &AbiStream,
        class_name: &str,
    ) -> SwiftStream {
        let StreamItemTransport::WireEncoded { decode_ops } = &stream.item;
        let method_name_pascal = pascal_case(stream.stream_id.as_str());

        let mode = match stream.mode {
            StreamMode::Async => SwiftStreamMode::Async,
            StreamMode::Batch => SwiftStreamMode::Batch {
                class_name: class_name.to_string(),
                method_name_pascal: method_name_pascal.clone(),
            },
            StreamMode::Callback => SwiftStreamMode::Callback {
                class_name: class_name.to_string(),
                method_name_pascal: method_name_pascal.clone(),
            },
        };

        SwiftStream {
            name: camel_case(stream.stream_id.as_str()),
            mode,
            item_type: emit::swift_type(&stream_def.item_type),
            // remap_root_in_seq only works on writes (ValueExpr), reads
            // use OffsetExpr so we do our own variable rename here
            item_decode: self.rebase_read_seq(decode_ops, "pos", "0"),
            subscribe: stream.subscribe.to_string(),
            poll: stream.poll.to_string(),
            pop_batch: stream.pop_batch.to_string(),
            wait: stream.wait.to_string(),
            unsubscribe: stream.unsubscribe.to_string(),
            free: stream.free.to_string(),
            free_buf: self.abi.free_buf.to_string(),
            atomic_cas: self.abi.atomic_cas.to_string(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Callbacks
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_callbacks(&self) -> Vec<SwiftCallback> {
        self.contract
            .catalog
            .all_callbacks()
            .filter(|def| def.kind == CallbackKind::Trait)
            .map(|def| {
                let protocol_name = pascal_case(def.id.as_str());
                let vtable_var = format!("{}VTableInstance", lower_first_char(&protocol_name));
                let plan = self.abi_index.callback(self.abi, &def.id);

                let methods = def
                    .methods
                    .iter()
                    .map(|method_def| {
                        let abi_method = plan
                            .methods
                            .iter()
                            .find(|m| m.id == method_def.id)
                            .expect("callback method");
                        // IR generates encode ops with self as root but we capture the
                        // callback return as result in the template so we need to rewrite
                        // the root before handing it off
                        let returns = self.rebase_return_encode(
                            self.swift_return_from_abi(
                                &abi_method.return_,
                                &abi_method.error,
                                &method_def.returns,
                            ),
                            "result",
                        );
                        let has_out_param = !abi_method.is_async && !returns.is_void();
                        let param_map = method_def
                            .params
                            .iter()
                            .map(|param| (param.name.clone(), param))
                            .collect::<HashMap<_, _>>();
                        // abi has extra params like context pointers for vtable machinery
                        // we only care about the ones the user declared in their trait
                        let params = abi_method
                            .params
                            .iter()
                            .filter(|param| {
                                matches!(
                                    param.role,
                                    ParamRole::InDirect | ParamRole::InEncoded { .. }
                                )
                            })
                            .map(|param| {
                                let def = param_map.get(&param.name).unwrap_or_else(|| {
                                    panic!(
                                        "param def not found: callback={}, method={}, param={}, role={:?}",
                                        plan.callback_id.as_str(),
                                        abi_method.id.as_str(),
                                        param.name.as_str(),
                                        param.role,
                                    )
                                });
                                self.lower_callback_param(def, param)
                            })
                            .collect();

                        SwiftCallbackMethod {
                            swift_name: camel_case(abi_method.id.as_str()),
                            ffi_name: abi_method.vtable_field.as_str().to_string(),
                            params,
                            returns,
                            is_async: abi_method.is_async,
                            has_out_param,
                            doc: method_def.doc.clone(),
                        }
                    })
                    .collect();

                SwiftCallback {
                    protocol_name: protocol_name.clone(),
                    wrapper_class: format!("{}Wrapper", protocol_name),
                    vtable_var,
                    vtable_type: plan.vtable_type.as_str().to_string(),
                    bridge_name: format!("{}Bridge", protocol_name),
                    register_fn: plan.register_fn.as_str().to_string(),
                    create_fn: plan.create_fn.as_str().to_string(),
                    methods,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }

    fn lower_callback_param(&self, def: &ParamDef, param: &AbiParam) -> SwiftCallbackParam {
        let label = camel_case(param.name.as_str());
        let (swift_type, ffi_args, decode_prelude) = match &param.role {
            ParamRole::InDirect => (emit::swift_type(&def.type_expr), vec![label.clone()], None),
            ParamRole::InEncoded { decode_ops, .. } => {
                let len_name = format!("{}Len", label);
                let decode = emit::emit_read_inline(decode_ops, "pos");
                (
                    emit::swift_type(&def.type_expr),
                    vec![label.clone(), len_name.clone()],
                    Some(format!(
                        "let {} = {{ let wire = WireBuffer(ptr: {}!, len: Int({})); var pos = 0; return {} }}()",
                        label, label, len_name, decode
                    )),
                )
            }
            ParamRole::SyntheticLen { .. }
            | ParamRole::InString { .. }
            | ParamRole::InBuffer { .. }
            | ParamRole::InHandle { .. }
            | ParamRole::InCallback { .. }
            | ParamRole::OutBuffer { .. }
            | ParamRole::OutDirect
            | ParamRole::OutLen { .. }
            | ParamRole::StatusOut => {
                panic!("unsupported ABI param role for Swift callback: {:?}", param.role)
            }
        };

        SwiftCallbackParam {
            label: label.clone(),
            swift_type,
            call_arg: label,
            ffi_args,
            decode_prelude,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Functions
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_functions(&self) -> Vec<SwiftFunction> {
        self.contract
            .functions
            .iter()
            .map(|def| {
                let call = self.abi_call(&CallId::Function(def.id.clone()));
                let mode = self.lower_call_mode(call, &def.returns);
                let returns = match &call.mode {
                    CallMode::Async(async_call) => {
                        self.lower_return_def_for_async(&async_call.error, &def.returns)
                    }
                    CallMode::Sync => {
                        self.swift_return_from_abi(&call.return_, &call.error, &def.returns)
                    }
                };

                SwiftFunction {
                    name: camel_case(def.id.as_str()),
                    mode,
                    params: def
                        .params
                        .iter()
                        .map(|p| self.lower_param(p, call))
                        .collect(),
                    returns,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Params
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_param(&self, param: &ParamDef, call: &AbiCall) -> SwiftParam {
        let abi_param = self.abi_param_for_semantic(call, &param.name);
        let swift_name = camel_case(param.name.as_str());

        let (swift_type, conversion) = match &abi_param.role {
            ParamRole::InDirect => (self.swift_type(&param.type_expr), SwiftConversion::Direct),
            ParamRole::InBuffer {
                element_abi,
                mutability,
                ..
            } => {
                let element_type = self.abi_to_swift(*element_abi);
                // raw byte buffers show up as Data in Swift because that is
                // what Swift developers expect for opaque byte blobs, [UInt8]
                // would work but Data is the idiomatic Swift type for this
                if *element_abi == AbiType::U8 && *mutability == Mutability::Shared {
                    ("Data".to_string(), SwiftConversion::ToData)
                } else {
                    let conversion = match mutability {
                        Mutability::Mutable => SwiftConversion::MutableBuffer {
                            element_type: element_type.clone(),
                        },
                        Mutability::Shared => SwiftConversion::PrimitiveBuffer {
                            element_type: element_type.clone(),
                        },
                    };
                    (format!("[{}]", element_type), conversion)
                }
            }
            ParamRole::InString { .. } => ("String".to_string(), SwiftConversion::ToString),
            ParamRole::InEncoded { encode_ops, .. } => (
                emit::swift_type(&param.type_expr),
                SwiftConversion::ToWireBuffer {
                    encode: encode_ops.clone(),
                },
            ),
            ParamRole::InHandle { class_id, nullable } => {
                let class_name = self.swift_name_for_class(class_id);
                let swift_type = if *nullable {
                    format!("{}?", class_name)
                } else {
                    class_name.clone()
                };
                (
                    swift_type,
                    SwiftConversion::PassHandle {
                        class_name,
                        nullable: *nullable,
                    },
                )
            }
            ParamRole::InCallback {
                callback_id,
                nullable,
                style,
            } => match style {
                CallbackStyle::BoxedDyn => {
                    let protocol = pascal_case(callback_id.as_str());
                    let swift_type = if *nullable {
                        format!("(any {})?", protocol)
                    } else {
                        format!("any {}", protocol)
                    };
                    (swift_type, SwiftConversion::WrapCallback { protocol })
                }
                CallbackStyle::ImplTrait => {
                    let closure_plan = self.build_closure_trampoline(callback_id, &swift_name);
                    let swift_type = format!("@escaping {}", closure_plan.swift_type);
                    (
                        swift_type,
                        SwiftConversion::InlineClosure {
                            closure: closure_plan,
                        },
                    )
                }
            },
            ParamRole::OutBuffer { .. } => {
                let element_type = self.buffer_element_swift_type(&param.type_expr);
                (
                    format!("[{}]", element_type),
                    SwiftConversion::MutableBuffer {
                        element_type: element_type.clone(),
                    },
                )
            }
            ParamRole::SyntheticLen { .. }
            | ParamRole::OutDirect
            | ParamRole::OutLen { .. }
            | ParamRole::StatusOut => panic!(
                "unsupported ABI param role for Swift param: {:?}",
                abi_param.role
            ),
        };

        SwiftParam {
            label: None,
            name: swift_name,
            swift_type,
            conversion,
        }
    }

    fn abi_param_for_semantic<'b>(&self, call: &'b AbiCall, name: &ParamName) -> &'b AbiParam {
        call.params
            .iter()
            .find(|param| {
                param.name.as_str() == name.as_str()
                    && matches!(
                        param.role,
                        ParamRole::InDirect
                            | ParamRole::InBuffer { .. }
                            | ParamRole::InString { .. }
                            | ParamRole::InEncoded { .. }
                            | ParamRole::OutBuffer { .. }
                            | ParamRole::InHandle { .. }
                            | ParamRole::InCallback { .. }
                    )
            })
            .expect("ABI param should exist")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Returns
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn swift_return_from_abi(
        &self,
        return_: &ReturnTransport,
        error: &ErrorTransport,
        returns: &ReturnDef,
    ) -> SwiftReturn {
        let base = match return_ {
            ReturnTransport::Void => SwiftReturn::Void,
            ReturnTransport::Direct(abi) => SwiftReturn::Direct {
                swift_type: self.abi_to_swift(*abi),
            },
            ReturnTransport::Encoded {
                decode_ops,
                encode_ops,
            } => SwiftReturn::FromWireBuffer {
                swift_type: self.swift_return_value_type(returns),
                decode: decode_ops.clone(),
                encode: encode_ops.clone(),
            },
            ReturnTransport::Handle { class_id, nullable } => {
                let class_name = self.swift_name_for_class(class_id);
                SwiftReturn::Handle {
                    class_name,
                    nullable: *nullable,
                }
            }
            ReturnTransport::Callback {
                callback_id,
                nullable,
            } => {
                let protocol = pascal_case(callback_id.as_str());
                let swift_type = if *nullable {
                    format!("(any {})?", protocol)
                } else {
                    format!("any {}", protocol)
                };
                SwiftReturn::Direct { swift_type }
            }
        };

        match error {
            ErrorTransport::None => base,
            ErrorTransport::Encoded { decode_ops } => SwiftReturn::Throws {
                ok: Box::new(base),
                err_type: self.swift_error_type(returns),
                err_decode: decode_ops.clone(),
                err_is_string: self.error_is_string(returns),
            },
            ErrorTransport::StatusCode => SwiftReturn::Throws {
                ok: Box::new(base),
                err_type: "FfiError".to_string(),
                err_decode: ReadSeq {
                    size: SizeExpr::Fixed(0),
                    ops: vec![],
                    shape: WireShape::Value,
                },
                err_is_string: false,
            },
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Type Helpers
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn abi_call(&self, id: &CallId) -> &AbiCall {
        self.abi_index.call(self.abi, id)
    }

    fn swift_type(&self, ty: &TypeExpr) -> String {
        // handles and callbacks need the lowerer to resolve class/protocol
        // names, emit::swift_type does not have that context so we handle
        // them here. Option wrapping them also needs special casing because
        // "any Protocol" requires parens to become "(any Protocol)?"
        match ty {
            TypeExpr::Handle(id) => self.swift_name_for_class(id),
            TypeExpr::Callback(id) => format!("any {}", pascal_case(id.as_str())),
            TypeExpr::Option(inner) => match inner.as_ref() {
                TypeExpr::Handle(id) => format!("{}?", self.swift_name_for_class(id)),
                TypeExpr::Callback(id) => format!("(any {})?", pascal_case(id.as_str())),
                _ => emit::swift_type(ty),
            },
            _ => emit::swift_type(ty),
        }
    }

    fn swift_return_value_type(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Void => "Void".to_string(),
            ReturnDef::Value(ty) => self.swift_type(ty),
            ReturnDef::Result { ok, .. } => self.swift_type(ok),
        }
    }

    fn swift_error_type(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Result { err, .. } => self.swift_type(err),
            _ => "FfiError".to_string(),
        }
    }

    fn error_is_string(&self, returns: &ReturnDef) -> bool {
        matches!(returns, ReturnDef::Result { err, .. } if matches!(err, TypeExpr::String))
    }

    fn record_decode_fields(&self, record: &AbiRecord) -> HashMap<FieldName, ReadSeq> {
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

    fn record_encode_fields(&self, record: &AbiRecord) -> HashMap<FieldName, WriteSeq> {
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

    fn record_field_offset(&self, seq: &ReadSeq) -> Option<usize> {
        seq.ops.first().and_then(|op| match op {
            ReadOp::Primitive { offset, .. } => match offset {
                OffsetExpr::Fixed(value) => Some(*value),
                OffsetExpr::Base => Some(0),
                OffsetExpr::BasePlus(add) => Some(*add),
                OffsetExpr::Var(_) | OffsetExpr::VarPlus(_, _) => None,
            },
            _ => None,
        })
    }

    fn rebase_read_seq(&self, seq: &ReadSeq, old_base: &str, new_base: &str) -> ReadSeq {
        ReadSeq {
            size: seq.size.clone(),
            ops: seq
                .ops
                .iter()
                .map(|op| self.rebase_read_op(op, old_base, new_base))
                .collect(),
            shape: seq.shape,
        }
    }

    fn rebase_read_op(&self, op: &ReadOp, old_base: &str, new_base: &str) -> ReadOp {
        match op {
            ReadOp::Primitive { primitive, offset } => ReadOp::Primitive {
                primitive: *primitive,
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::String { offset } => ReadOp::String {
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::Bytes { offset } => ReadOp::Bytes {
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::Option { tag_offset, some } => ReadOp::Option {
                tag_offset: self.rebase_offset_expr(tag_offset, old_base, new_base),
                some: Box::new(self.rebase_read_seq(some, old_base, new_base)),
            },
            ReadOp::Vec {
                len_offset,
                element_type,
                element,
                layout,
            } => ReadOp::Vec {
                len_offset: self.rebase_offset_expr(len_offset, old_base, new_base),
                element_type: element_type.clone(),
                element: Box::new(self.rebase_read_seq(element, old_base, new_base)),
                layout: layout.clone(),
            },
            ReadOp::Record { id, offset, fields } => ReadOp::Record {
                id: id.clone(),
                offset: self.rebase_offset_expr(offset, old_base, new_base),
                fields: fields
                    .iter()
                    .map(|field| {
                        let seq = self.rebase_read_seq(&field.seq, old_base, new_base);
                        FieldReadOp {
                            name: field.name.clone(),
                            seq,
                        }
                    })
                    .collect(),
            },
            ReadOp::Enum { id, offset, layout } => ReadOp::Enum {
                id: id.clone(),
                offset: self.rebase_offset_expr(offset, old_base, new_base),
                layout: layout.clone(),
            },
            ReadOp::Result {
                tag_offset,
                ok,
                err,
            } => ReadOp::Result {
                tag_offset: self.rebase_offset_expr(tag_offset, old_base, new_base),
                ok: Box::new(self.rebase_read_seq(ok, old_base, new_base)),
                err: Box::new(self.rebase_read_seq(err, old_base, new_base)),
            },
            ReadOp::Builtin { id, offset } => ReadOp::Builtin {
                id: id.clone(),
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::Custom { id, underlying } => ReadOp::Custom {
                id: id.clone(),
                underlying: Box::new(self.rebase_read_seq(underlying, old_base, new_base)),
            },
        }
    }

    fn rebase_offset_expr(
        &self,
        offset: &OffsetExpr,
        old_base: &str,
        new_base: &str,
    ) -> OffsetExpr {
        match offset {
            OffsetExpr::Fixed(value) => OffsetExpr::Fixed(*value),
            OffsetExpr::Base => OffsetExpr::Base,
            OffsetExpr::BasePlus(add) => OffsetExpr::BasePlus(*add),
            OffsetExpr::Var(name) => {
                if name == old_base {
                    OffsetExpr::Var(new_base.to_string())
                } else {
                    OffsetExpr::Var(name.clone())
                }
            }
            OffsetExpr::VarPlus(name, add) => {
                if name == old_base {
                    OffsetExpr::VarPlus(new_base.to_string(), *add)
                } else {
                    OffsetExpr::VarPlus(name.clone(), *add)
                }
            }
        }
    }

    fn rebase_return_encode(&self, returns: SwiftReturn, new_base: &str) -> SwiftReturn {
        match returns {
            SwiftReturn::FromWireBuffer {
                swift_type,
                decode,
                encode,
            } => SwiftReturn::FromWireBuffer {
                swift_type,
                decode,
                encode: remap_root_in_seq(&encode, ValueExpr::Var(new_base.to_string())),
            },
            SwiftReturn::Throws {
                ok,
                err_type,
                err_decode,
                err_is_string,
            } => SwiftReturn::Throws {
                ok: Box::new(self.rebase_return_encode(*ok, new_base)),
                err_type,
                err_decode,
                err_is_string,
            },
            other => other,
        }
    }

    fn buffer_element_swift_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Vec(inner) => emit::swift_type(inner),
            TypeExpr::Bytes => "UInt8".to_string(),
            _ => "UInt8".to_string(),
        }
    }

    fn build_closure_trampoline(
        &self,
        callback_id: &CallbackId,
        param_name: &str,
    ) -> SwiftClosureTrampoline {
        let callback_def = self
            .contract
            .catalog
            .resolve_callback(callback_id)
            .expect("closure callback should exist");
        let method = &callback_def.methods[0];
        let abi_callback = self.abi_index.callback(self.abi, callback_id);
        let abi_method = abi_callback
            .methods
            .iter()
            .find(|m| m.id == method.id)
            .expect("closure callback method");

        let param_types: Vec<String> = method
            .params
            .iter()
            .map(|p| self.swift_type(&p.type_expr))
            .collect();
        let return_type = match &method.returns {
            ReturnDef::Void => "Void".to_string(),
            ReturnDef::Value(ty) => self.swift_type(ty),
            ReturnDef::Result { ok, .. } => format!("Result<{}, Error>", self.swift_type(ok)),
        };

        let swift_type = if param_types.is_empty() {
            format!("() -> {}", return_type)
        } else {
            format!("({}) -> {}", param_types.join(", "), return_type)
        };

        let upper_name = pascal_case(param_name);
        let type_alias = format!("{}CallbackFn", upper_name);
        let box_class = format!("{}CallbackBox", upper_name);

        let abi_params: Vec<&AbiParam> = abi_method
            .params
            .iter()
            .filter(|param| {
                matches!(
                    param.role,
                    ParamRole::InDirect | ParamRole::InEncoded { .. }
                )
            })
            .collect();
        let trampoline_params: Vec<SwiftClosureTrampolineParam> = method
            .params
            .iter()
            .zip(abi_params.iter())
            .enumerate()
            .map(|(idx, (param_def, abi_param))| {
                self.build_closure_trampoline_param(idx, param_def, abi_param)
            })
            .collect();

        SwiftClosureTrampoline {
            type_alias,
            swift_type,
            box_class,
            box_var: format!("{}Box", param_name),
            ptr_var: format!("{}Ptr", param_name),
            trampoline_var: format!("{}Trampoline", param_name),
            param_name: param_name.to_string(),
            trampoline_params,
        }
    }

    fn build_closure_trampoline_param(
        &self,
        idx: usize,
        param_def: &ParamDef,
        abi_param: &AbiParam,
    ) -> SwiftClosureTrampolineParam {
        match &abi_param.role {
            ParamRole::InEncoded { decode_ops, .. } => {
                let ptr_name = format!("ptr{}", idx);
                let len_name = format!("len{}", idx);
                let wire_buffer = format!("WireBuffer(ptr: {}!, len: Int({}))", ptr_name, len_name);
                let decode = emit::emit_read_value_at(decode_ops, "0");
                let decode_expr = format!("{{ let wire = {}; return {} }}()", wire_buffer, decode);
                SwiftClosureTrampolineParam {
                    name: format!("{}, {}", ptr_name, len_name),
                    c_type: "UnsafePointer<UInt8>?, UInt".to_string(),
                    decode_expr,
                }
            }
            ParamRole::InDirect => {
                let arg_name = format!("arg{}", idx);
                SwiftClosureTrampolineParam {
                    name: arg_name.clone(),
                    c_type: self.abi_to_swift(abi_param.ffi_type),
                    decode_expr: arg_name,
                }
            }
            ParamRole::SyntheticLen { .. }
            | ParamRole::InString { .. }
            | ParamRole::InBuffer { .. }
            | ParamRole::InHandle { .. }
            | ParamRole::InCallback { .. }
            | ParamRole::OutBuffer { .. }
            | ParamRole::OutDirect
            | ParamRole::OutLen { .. }
            | ParamRole::StatusOut => panic!(
                "unsupported closure param role for {}",
                param_def.name.as_str()
            ),
        }
    }

    fn abi_to_swift(&self, abi: AbiType) -> String {
        match abi {
            AbiType::Void => "Void",
            AbiType::Bool => "Bool",
            AbiType::I8 => "Int8",
            AbiType::U8 => "UInt8",
            AbiType::I16 => "Int16",
            AbiType::U16 => "UInt16",
            AbiType::I32 => "Int32",
            AbiType::U32 => "UInt32",
            AbiType::I64 => "Int64",
            AbiType::U64 => "UInt64",
            AbiType::ISize => "Int",
            AbiType::USize => "UInt",
            AbiType::F32 => "Float",
            AbiType::F64 => "Double",
            AbiType::Pointer => "OpaquePointer",
        }
        .to_string()
    }

    fn swift_name_for_record(&self, id: &RecordId) -> String {
        pascal_case(id.as_str())
    }

    fn swift_name_for_enum(&self, id: &EnumId) -> String {
        pascal_case(id.as_str())
    }

    fn swift_name_for_class(&self, id: &ClassId) -> String {
        pascal_case(id.as_str())
    }

    fn lower_call_mode(&self, call: &AbiCall, returns: &ReturnDef) -> SwiftCallMode {
        match &call.mode {
            CallMode::Sync => SwiftCallMode::Sync {
                symbol: call.symbol.as_str().to_string(),
            },
            CallMode::Async(async_call) => SwiftCallMode::Async {
                start: call.symbol.as_str().to_string(),
                poll: async_call.poll.as_str().to_string(),
                complete: async_call.complete.as_str().to_string(),
                cancel: async_call.cancel.as_str().to_string(),
                free: async_call.free.as_str().to_string(),
                result: Box::new(self.lower_async_result(
                    &async_call.result,
                    &async_call.error,
                    returns,
                )),
            },
        }
    }

    fn lower_async_result(
        &self,
        result: &AsyncResultTransport,
        error: &ErrorTransport,
        returns: &ReturnDef,
    ) -> SwiftAsyncResult {
        let returns_is_result = matches!(returns, ReturnDef::Result { .. });
        let throws = returns_is_result || matches!(error, ErrorTransport::Encoded { .. });

        match result {
            AsyncResultTransport::Void => SwiftAsyncResult::Void,
            AsyncResultTransport::Direct(abi) => SwiftAsyncResult::Direct {
                swift_type: self.abi_to_swift(*abi),
                conversion: SwiftAsyncConversion::None,
            },
            AsyncResultTransport::Encoded { decode_ops, .. } => {
                let ok_type = if throws {
                    match returns {
                        ReturnDef::Result { ok, .. } => Some(self.swift_type(ok)),
                        _ => None,
                    }
                } else {
                    None
                };
                let (swift_type, err_is_string) = match returns {
                    ReturnDef::Result { ok, err } => {
                        (self.swift_type(ok), matches!(err, TypeExpr::String))
                    }
                    ReturnDef::Value(ty) => (self.swift_type(ty), false),
                    ReturnDef::Void => ("Void".to_string(), false),
                };
                // when the error transport is not Encoded, the error decode
                // ops are bundled inside the result decode ops themselves,
                // so we dig into the first ReadOp::Result to pull out
                // the err branch
                let err_decode = match error {
                    ErrorTransport::Encoded { decode_ops } => decode_ops.clone(),
                    _ => match decode_ops.ops.first() {
                        Some(ReadOp::Result { err, .. }) => err.as_ref().clone(),
                        _ => ReadSeq {
                            size: SizeExpr::Fixed(0),
                            ops: Vec::new(),
                            shape: WireShape::Value,
                        },
                    },
                };
                SwiftAsyncResult::Encoded {
                    swift_type,
                    ok_type,
                    decode: decode_ops.clone(),
                    throws,
                    err_decode,
                    err_is_string,
                }
            }
            AsyncResultTransport::Handle { class_id, nullable } => SwiftAsyncResult::Direct {
                swift_type: if *nullable {
                    format!("{}?", self.swift_name_for_class(class_id))
                } else {
                    self.swift_name_for_class(class_id)
                },
                conversion: SwiftAsyncConversion::Handle {
                    class_name: self.swift_name_for_class(class_id),
                    nullable: *nullable,
                },
            },
            AsyncResultTransport::Callback {
                callback_id,
                nullable,
            } => SwiftAsyncResult::Direct {
                swift_type: if *nullable {
                    format!("(any {})?", pascal_case(callback_id.as_str()))
                } else {
                    format!("any {}", pascal_case(callback_id.as_str()))
                },
                conversion: SwiftAsyncConversion::Callback {
                    protocol: pascal_case(callback_id.as_str()),
                    nullable: *nullable,
                },
            },
        }
    }

    fn lower_return_def_for_async(
        &self,
        error: &ErrorTransport,
        returns: &ReturnDef,
    ) -> SwiftReturn {
        match error {
            ErrorTransport::None => SwiftReturn::Void,
            ErrorTransport::StatusCode => SwiftReturn::Throws {
                ok: Box::new(SwiftReturn::Void),
                err_type: "FfiError".to_string(),
                err_decode: ReadSeq {
                    size: SizeExpr::Fixed(0),
                    ops: vec![],
                    shape: WireShape::Value,
                },
                err_is_string: false,
            },
            ErrorTransport::Encoded { decode_ops } => SwiftReturn::Throws {
                ok: Box::new(SwiftReturn::Void),
                err_type: self.swift_error_type(returns),
                err_decode: decode_ops.clone(),
                err_is_string: self.error_is_string(returns),
            },
        }
    }
}

fn lower_first_char(name: &str) -> String {
    name.chars()
        .enumerate()
        .map(|(index, ch)| {
            if index == 0 {
                ch.to_ascii_lowercase()
            } else {
                ch
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::{FfiContract, PackageInfo};
    use crate::ir::definitions::{
        CStyleVariant, CallbackKind, CallbackMethodDef, CallbackTraitDef, DataVariant, EnumDef,
        EnumRepr, FieldDef, ParamDef, ParamPassing, RecordDef, ReturnDef, VariantPayload,
    };
    use crate::ir::ids::{CallbackId, FieldName, MethodId, ParamName, VariantName};
    use crate::ir::types::{PrimitiveType, TypeExpr};

    fn empty_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        }
    }

    fn lower_contract(contract: &FfiContract) -> SwiftModule {
        let abi = IrLowerer::new(contract).to_abi_contract();
        SwiftLowerer::new(contract, &abi).lower()
    }

    #[test]
    fn blittable_record_is_detected() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            id: RecordId::new("Point"),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        assert_eq!(module.records.len(), 1);
        let record = &module.records[0];
        assert!(
            record.is_blittable,
            "Point should be blittable (primitives only)"
        );
        assert_eq!(record.blittable_size, Some(16));
    }

    #[test]
    fn non_blittable_record_with_string() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            id: RecordId::new("User"),
            fields: vec![
                FieldDef {
                    name: FieldName::new("id"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I64),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("name"),
                    type_expr: TypeExpr::String,
                    doc: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        assert_eq!(module.records.len(), 1);
        let record = &module.records[0];
        assert!(
            !record.is_blittable,
            "User should NOT be blittable (has String)"
        );
        assert_eq!(record.blittable_size, None);
    }

    #[test]
    fn non_blittable_record_with_vec() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            id: RecordId::new("Scores"),
            fields: vec![FieldDef {
                name: FieldName::new("values"),
                type_expr: TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
                doc: None,
            }],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        assert_eq!(module.records.len(), 1);
        let record = &module.records[0];
        assert!(
            !record.is_blittable,
            "Scores should NOT be blittable (has Vec)"
        );
    }

    #[test]
    fn field_names_are_camel_case() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            id: RecordId::new("Config"),
            fields: vec![
                FieldDef {
                    name: FieldName::new("max_connections"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("timeout_ms"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U64),
                    doc: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        let record = &module.records[0];
        assert_eq!(record.fields[0].swift_name, "maxConnections");
        assert_eq!(record.fields[1].swift_name, "timeoutMs");
    }

    #[test]
    fn primitive_types_map_correctly() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            id: RecordId::new("AllPrimitives"),
            fields: vec![
                FieldDef {
                    name: FieldName::new("a"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::Bool),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("b"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I8),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("c"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U8),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("d"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I16),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("e"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U16),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("f"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("g"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("h"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I64),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("i"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U64),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("j"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F32),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("k"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let record = &module.records[0];

        assert_eq!(record.fields[0].swift_type, "Bool");
        assert_eq!(record.fields[1].swift_type, "Int8");
        assert_eq!(record.fields[2].swift_type, "UInt8");
        assert_eq!(record.fields[3].swift_type, "Int16");
        assert_eq!(record.fields[4].swift_type, "UInt16");
        assert_eq!(record.fields[5].swift_type, "Int32");
        assert_eq!(record.fields[6].swift_type, "UInt32");
        assert_eq!(record.fields[7].swift_type, "Int64");
        assert_eq!(record.fields[8].swift_type, "UInt64");
        assert_eq!(record.fields[9].swift_type, "Float");
        assert_eq!(record.fields[10].swift_type, "Double");
    }

    #[test]
    fn c_style_enum_is_lowered() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Status"),
            repr: EnumRepr::CStyle {
                tag_type: PrimitiveType::I32,
                variants: vec![
                    CStyleVariant {
                        name: VariantName::new("Active"),
                        discriminant: 0,
                        doc: None,
                    },
                    CStyleVariant {
                        name: VariantName::new("Inactive"),
                        discriminant: 1,
                        doc: None,
                    },
                    CStyleVariant {
                        name: VariantName::new("Pending"),
                        discriminant: 2,
                        doc: None,
                    },
                ],
            },
            is_error: false,
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        assert_eq!(module.enums.len(), 1);
        let e = &module.enums[0];
        assert_eq!(e.name, "Status");
        assert!(e.is_c_style());
        assert_eq!(e.variants.len(), 3);
        assert_eq!(e.variants[0].swift_name, "active");
        assert_eq!(e.variants[1].swift_name, "inactive");
        assert_eq!(e.variants[2].swift_name, "pending");
    }

    #[test]
    fn data_enum_is_lowered() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Value"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![
                    DataVariant {
                        name: VariantName::new("Int"),
                        discriminant: 0,
                        payload: VariantPayload::Tuple(vec![TypeExpr::Primitive(
                            PrimitiveType::I64,
                        )]),
                        doc: None,
                    },
                    DataVariant {
                        name: VariantName::new("Text"),
                        discriminant: 1,
                        payload: VariantPayload::Tuple(vec![TypeExpr::String]),
                        doc: None,
                    },
                ],
            },
            is_error: false,
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        assert_eq!(module.enums.len(), 1);
        let e = &module.enums[0];
        assert_eq!(e.name, "Value");
        assert!(!e.is_c_style());
        assert_eq!(e.variants.len(), 2);
    }

    #[test]
    fn blittable_struct_has_c_offsets() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            id: RecordId::new("Aligned"),
            fields: vec![
                FieldDef {
                    name: FieldName::new("a"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U8),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("b"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("c"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U8),
                    doc: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let record = &module.records[0];

        assert!(record.is_blittable);
        assert_eq!(record.fields[0].c_offset, Some(0));
        assert_eq!(record.fields[1].c_offset, Some(4));
        assert_eq!(record.fields[2].c_offset, Some(8));
    }

    #[test]
    fn option_type_maps_to_swift_optional() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            id: RecordId::new("MaybeValue"),
            fields: vec![FieldDef {
                name: FieldName::new("value"),
                type_expr: TypeExpr::Option(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
                doc: None,
            }],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let record = &module.records[0];

        assert_eq!(record.fields[0].swift_type, "Int32?");
    }

    #[test]
    fn vec_type_maps_to_swift_array() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            id: RecordId::new("Numbers"),
            fields: vec![FieldDef {
                name: FieldName::new("items"),
                type_expr: TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
                doc: None,
            }],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let record = &module.records[0];

        assert_eq!(record.fields[0].swift_type, "[Int32]");
    }

    #[test]
    fn nested_record_type_maps_correctly() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            id: RecordId::new("Inner"),
            fields: vec![FieldDef {
                name: FieldName::new("value"),
                type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                doc: None,
            }],
            doc: None,
            deprecated: None,
        });
        contract.catalog.insert_record(RecordDef {
            id: RecordId::new("Outer"),
            fields: vec![FieldDef {
                name: FieldName::new("inner"),
                type_expr: TypeExpr::Record(RecordId::new("Inner")),
                doc: None,
            }],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        let outer = module
            .records
            .iter()
            .find(|r| r.class_name == "Outer")
            .unwrap();
        assert_eq!(outer.fields[0].swift_type, "Inner");
    }

    #[test]
    fn callback_with_encoded_param_does_not_panic() {
        let mut contract = empty_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("Logger"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("log"),
                params: vec![ParamDef {
                    name: ParamName::new("message"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        });

        let module = lower_contract(&contract);

        assert_eq!(module.callbacks.len(), 1);
        let cb = &module.callbacks[0];
        assert_eq!(cb.protocol_name, "Logger");
        assert_eq!(cb.methods.len(), 1);
        assert_eq!(cb.methods[0].params.len(), 1);
    }
}
