use std::collections::HashMap;

use boltffi_ffi_rules::naming::{self, snake_to_camel as camel_case};

use crate::ir::abi::{
    AbiCall, AbiContract, AbiEnum, AbiEnumPayload, AbiParam, AbiRecord, CallId, CallMode,
    ErrorTransport, ParamRole, ReturnTransport,
};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{EnumDef, FunctionDef, ParamDef, RecordDef};
use crate::ir::ids::{EnumId, FieldName, RecordId};
use crate::ir::ops::{
    FieldWriteOp, ReadOp, ReadSeq, SizeExpr, ValueExpr, WireShape, WriteOp, WriteSeq,
};
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

            let mut wasm_params: Vec<TsWasmParam> = call
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
                ReturnTransport::Encoded { .. } => {
                    wasm_params.insert(
                        0,
                        TsWasmParam {
                            name: "out".to_string(),
                            wasm_type: "number".to_string(),
                        },
                    );
                    None
                }
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
    use crate::ir::definitions::{FunctionDef, ParamDef, ParamPassing, ReturnDef};
    use crate::ir::ids::{FunctionId, ParamName};

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

    #[test]
    fn wasm_import_encoded_return_uses_sret_out_param() {
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
            .expect("wasm import for encoded return");

        assert_eq!(import.return_wasm_type, None);
        assert_eq!(import.params.len(), 2);
        assert_eq!(import.params[0].name, "out");
        assert_eq!(import.params[0].wasm_type, "number");
        assert_eq!(import.params[1].name, "count");
        assert_eq!(import.params[1].wasm_type, "number");
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
}
