use std::collections::HashMap;

use boltffi_ffi_rules::naming::{self, snake_to_camel as camel_case};

use crate::ir::abi::{
    AbiCall, AbiContract, AbiEnum, AbiEnumPayload, AbiParam, AbiRecord, CallId, CallMode,
    ErrorTransport, ParamRole, ReturnTransport,
};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{EnumDef, FunctionDef, RecordDef};
use crate::ir::ids::{EnumId, FieldName, RecordId};
use crate::ir::ops::{ReadOp, ReadSeq, SizeExpr, WireShape, WriteOp, WriteSeq};
use crate::ir::plan::AbiType;
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::render::typescript::emit;
use crate::render::typescript::plan::*;

struct AbiIndex {
    calls: HashMap<CallId, usize>,
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
            records,
            enums,
        }
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

        let functions = self
            .contract
            .functions
            .iter()
            .filter_map(|def| self.lower_function(def, &index))
            .collect();

        let wasm_imports = self.collect_wasm_imports(&index);

        TsModule {
            module_name: self.module_name.clone(),
            abi_version: 1,
            records,
            enums,
            functions,
            wasm_imports,
        }
    }

    fn lower_record(&self, def: &RecordDef, index: &AbiIndex) -> TsRecord {
        let abi_record = index.record(self.abi, &def.id);
        let name = naming::to_upper_camel_case(def.id.as_str());

        let decode_fields = record_decode_fields(abi_record);
        let encode_fields = record_encode_fields(abi_record);

        let fields = def
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

        TsRecord {
            name,
            fields,
            is_blittable: abi_record.is_blittable,
            wire_size: abi_record.size,
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
                            encode: field.encode.clone(),
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

    fn lower_function(&self, def: &FunctionDef, index: &AbiIndex) -> Option<TsFunction> {
        let call_id = CallId::Function(def.id.clone());
        let abi_call = index.call(self.abi, &call_id);

        if matches!(abi_call.mode, CallMode::Async(_)) {
            return None;
        }

        let func_name = camel_case(def.id.as_str());
        let ffi_name = abi_call.symbol.as_str().to_string();

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
            .map(|param| self.lower_param(param))
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

    fn lower_param(&self, param: &AbiParam) -> TsParam {
        let name = camel_case(param.name.as_str());
        match &param.role {
            ParamRole::InDirect => TsParam {
                name: emit::escape_ts_keyword(&name),
                ts_type: ts_abi_type(&param.ffi_type),
                conversion: TsParamConversion::Direct,
            },
            ParamRole::InString { .. } => TsParam {
                name: emit::escape_ts_keyword(&name),
                ts_type: "string".to_string(),
                conversion: TsParamConversion::String,
            },
            ParamRole::InEncoded {
                encode_ops,
                decode_ops: _,
                ..
            } => TsParam {
                name: emit::escape_ts_keyword(&name),
                ts_type: "unknown".to_string(),
                conversion: TsParamConversion::WireEncoded {
                    encode: encode_ops.clone(),
                },
            },
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
                let decode = emit::emit_reader_read(decode_ops);
                let ts_type_str = infer_ts_type_from_read_ops(decode_ops);
                (Some(ts_type_str), TsReturnAbi::WireEncoded, decode)
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
                    name: camel_case(p.name.as_str()),
                    wasm_type: abi_type_to_wasm(&p.ffi_type),
                })
                .collect();

            let return_wasm_type = match &call.return_ {
                ReturnTransport::Void => None,
                ReturnTransport::Direct(abi_type) => Some(abi_type_to_wasm(abi_type)),
                ReturnTransport::Encoded { .. } => Some("number".to_string()),
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
