use std::collections::HashSet;

use crate::ir::abi::{AbiCall, AbiContract, AbiParam, AbiRecord, CallId};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{FieldDef, FunctionDef, RecordDef, ReturnDef};
use crate::ir::ids::{FieldName, RecordId};
use crate::ir::ops::{ReadOp, ReadSeq, WriteOp, WriteSeq};
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::ir::{InputBinding, ParamBinding};

use super::JavaOptions;
use super::mappings;
use super::names::NamingConvention;
use super::plan::{
    JavaFunction, JavaModule, JavaParam, JavaParamKind, JavaRecord, JavaRecordField,
    JavaRecordShape, JavaReturnStrategy, JavaWireWriter,
};

pub struct JavaLowerer<'a> {
    ffi: &'a FfiContract,
    abi: &'a AbiContract,
    package_name: String,
    module_name: String,
    options: JavaOptions,
    supported_records: HashSet<String>,
}

impl<'a> JavaLowerer<'a> {
    pub fn new(
        ffi: &'a FfiContract,
        abi: &'a AbiContract,
        package_name: String,
        module_name: String,
        options: JavaOptions,
    ) -> Self {
        let supported_records = Self::compute_supported_records(ffi);
        Self {
            ffi,
            abi,
            package_name,
            module_name,
            options,
            supported_records,
        }
    }

    fn compute_supported_records(ffi: &FfiContract) -> HashSet<String> {
        let mut supported = HashSet::new();
        let mut changed = true;
        while changed {
            changed = false;
            for record in ffi.catalog.all_records() {
                let id = record.id.as_str().to_string();
                if supported.contains(&id) {
                    continue;
                }
                let all_fields_ok = record.fields.iter().all(|f| match &f.type_expr {
                    TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Void => true,
                    TypeExpr::Record(ref_id) => supported.contains(ref_id.as_str()),
                    _ => false,
                });
                if all_fields_ok {
                    supported.insert(id);
                    changed = true;
                }
            }
        }
        supported
    }

    pub fn module(&self) -> JavaModule {
        let lib_name = self
            .options
            .library_name
            .clone()
            .unwrap_or_else(|| self.module_name.clone())
            .replace('-', "_");

        let prefix = boltffi_ffi_rules::naming::ffi_prefix().to_string();

        let records: Vec<JavaRecord> = self
            .ffi
            .catalog
            .all_records()
            .filter(|r| self.supported_records.contains(r.id.as_str()))
            .map(|r| self.lower_record(r))
            .collect();

        let functions: Vec<JavaFunction> = self
            .ffi
            .functions
            .iter()
            .filter(|f| !f.is_async && self.is_supported_function(f))
            .map(|f| self.lower_function(f))
            .collect();

        JavaModule {
            package_name: self.package_name.clone(),
            class_name: NamingConvention::class_name(&self.module_name),
            lib_name,
            java_version: self.options.min_java_version,
            prefix,
            records,
            functions,
        }
    }

    fn is_supported_function(&self, func: &FunctionDef) -> bool {
        let params_ok = func
            .params
            .iter()
            .all(|p| self.is_supported_type(&p.type_expr));
        let return_ok = match &func.returns {
            ReturnDef::Void => true,
            ReturnDef::Value(ty) => self.is_supported_type(ty),
            ReturnDef::Result { .. } => false,
        };
        params_ok && return_ok
    }

    fn is_supported_type(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Void => true,
            TypeExpr::Record(id) => self.supported_records.contains(id.as_str()),
            _ => false,
        }
    }

    fn lower_record(&self, record: &RecordDef) -> JavaRecord {
        let class_name = NamingConvention::class_name(record.id.as_str());
        let fields = record
            .fields
            .iter()
            .map(|field| self.lower_record_field(&record.id, field))
            .collect();
        let shape = if self.options.min_java_version.supports_records() {
            JavaRecordShape::NativeRecord
        } else {
            JavaRecordShape::ClassicClass
        };
        JavaRecord {
            shape,
            class_name,
            fields,
        }
    }

    fn lower_record_field(&self, record_id: &RecordId, field: &FieldDef) -> JavaRecordField {
        let decode_seq = self
            .record_field_read_seq(record_id, &field.name)
            .expect("record field decode ops");
        let encode_seq = self
            .record_field_write_seq(record_id, &field.name)
            .expect("record field encode ops");
        JavaRecordField {
            name: NamingConvention::field_name(field.name.as_str()),
            java_type: self.java_type(&field.type_expr),
            wire_decode_expr: super::emit::emit_reader_read(&decode_seq),
            wire_size_expr: super::emit::emit_size_expr_for_write_seq(&encode_seq),
            wire_encode_expr: super::emit::emit_write_expr(&encode_seq, "wire"),
            equals_expr: self.record_field_equals_expr(&field.type_expr, field.name.as_str()),
            hash_expr: self.record_field_hash_expr(&field.type_expr, field.name.as_str()),
        }
    }

    fn record_field_equals_expr(&self, ty: &TypeExpr, field_name: &str) -> String {
        let field = NamingConvention::field_name(field_name);
        match ty {
            TypeExpr::Primitive(PrimitiveType::F32) => {
                format!("Float.compare(this.{field}, other.{field}) == 0")
            }
            TypeExpr::Primitive(PrimitiveType::F64) => {
                format!("Double.compare(this.{field}, other.{field}) == 0")
            }
            TypeExpr::Primitive(_) => format!("this.{field} == other.{field}"),
            TypeExpr::String | TypeExpr::Record(_) => {
                format!("Objects.equals(this.{field}, other.{field})")
            }
            _ => panic!("unsupported Java record field equality type: {:?}", ty),
        }
    }

    fn record_field_hash_expr(&self, ty: &TypeExpr, field_name: &str) -> String {
        let field = NamingConvention::field_name(field_name);
        match ty {
            TypeExpr::Primitive(PrimitiveType::Bool) => format!("Boolean.hashCode({field})"),
            TypeExpr::Primitive(PrimitiveType::I8) | TypeExpr::Primitive(PrimitiveType::U8) => {
                format!("Byte.hashCode({field})")
            }
            TypeExpr::Primitive(PrimitiveType::I16) | TypeExpr::Primitive(PrimitiveType::U16) => {
                format!("Short.hashCode({field})")
            }
            TypeExpr::Primitive(PrimitiveType::I32) | TypeExpr::Primitive(PrimitiveType::U32) => {
                format!("Integer.hashCode({field})")
            }
            TypeExpr::Primitive(PrimitiveType::I64)
            | TypeExpr::Primitive(PrimitiveType::U64)
            | TypeExpr::Primitive(PrimitiveType::ISize)
            | TypeExpr::Primitive(PrimitiveType::USize) => format!("Long.hashCode({field})"),
            TypeExpr::Primitive(PrimitiveType::F32) => format!("Float.hashCode({field})"),
            TypeExpr::Primitive(PrimitiveType::F64) => format!("Double.hashCode({field})"),
            TypeExpr::String | TypeExpr::Record(_) => format!("Objects.hashCode({field})"),
            _ => panic!("unsupported Java record field hash type: {:?}", ty),
        }
    }

    fn record_field_read_seq(
        &self,
        record_id: &RecordId,
        field_name: &FieldName,
    ) -> Option<ReadSeq> {
        self.abi_record_for(record_id)
            .and_then(|record| match record.decode_ops.ops.first() {
                Some(ReadOp::Record { fields, .. }) => fields
                    .iter()
                    .find(|field| field.name == *field_name)
                    .map(|field| field.seq.clone()),
                _ => None,
            })
    }

    fn record_field_write_seq(
        &self,
        record_id: &RecordId,
        field_name: &FieldName,
    ) -> Option<WriteSeq> {
        self.abi_record_for(record_id)
            .and_then(|record| match record.encode_ops.ops.first() {
                Some(WriteOp::Record { fields, .. }) => fields
                    .iter()
                    .find(|field| field.name == *field_name)
                    .map(|field| field.seq.clone()),
                _ => None,
            })
    }

    fn abi_record_for(&self, record_id: &RecordId) -> Option<&AbiRecord> {
        self.abi
            .records
            .iter()
            .find(|record| record.id == *record_id)
    }

    fn lower_function(&self, func: &FunctionDef) -> JavaFunction {
        let call = self.abi_call_for_function(func);

        let wire_writers = self.wire_writers_for_params(call);

        let params: Vec<JavaParam> = func
            .params
            .iter()
            .map(|p| self.lower_param(p.name.as_str(), &p.type_expr, &wire_writers))
            .collect();

        let strategy = self.return_strategy(&func.returns);

        JavaFunction {
            name: NamingConvention::method_name(func.id.as_str()),
            ffi_name: call.symbol.as_str().to_string(),
            params,
            return_type: self.return_java_type(&func.returns),
            strategy,
            wire_writers,
        }
    }

    fn lower_param(&self, name: &str, ty: &TypeExpr, wire_writers: &[JavaWireWriter]) -> JavaParam {
        let field_name = NamingConvention::field_name(name);
        let java_type = self.java_type(ty);
        let (native_type, kind) = self.native_param_mapping(name, ty, wire_writers);
        JavaParam {
            name: field_name,
            java_type,
            native_type,
            kind,
        }
    }

    fn native_param_mapping(
        &self,
        name: &str,
        ty: &TypeExpr,
        wire_writers: &[JavaWireWriter],
    ) -> (String, JavaParamKind) {
        match ty {
            TypeExpr::String => ("byte[]".to_string(), JavaParamKind::Utf8Bytes),
            TypeExpr::Record(_) => {
                let binding_name = wire_writers
                    .iter()
                    .find(|w| w.param_name == name)
                    .map(|w| w.binding_name.clone())
                    .unwrap_or_default();
                (
                    "ByteBuffer".to_string(),
                    JavaParamKind::WireEncoded { binding_name },
                )
            }
            other => (self.java_type(other), JavaParamKind::Direct),
        }
    }

    fn wire_writers_for_params(&self, call: &AbiCall) -> Vec<JavaWireWriter> {
        call.params
            .iter()
            .filter_map(|param| {
                self.input_write_ops(param).map(|encode_ops| {
                    let param_name = param.name.as_str().to_string();
                    let binding_name = format!("_wire_{}", param.name.as_str());
                    let encode_expr = super::emit::emit_write_expr(&encode_ops, &binding_name);
                    JavaWireWriter {
                        binding_name,
                        param_name,
                        size_expr: super::emit::emit_size_expr_for_write_seq(&encode_ops),
                        encode_expr,
                    }
                })
            })
            .collect()
    }

    fn input_write_ops(&self, param: &AbiParam) -> Option<WriteSeq> {
        match param.param_binding() {
            ParamBinding::Input(InputBinding::WirePacket { encode_ops, .. }) => {
                Some(encode_ops.clone())
            }
            _ => None,
        }
    }

    fn return_java_type(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Void => "void".to_string(),
            ReturnDef::Value(TypeExpr::Void) => "void".to_string(),
            ReturnDef::Value(ty) => self.java_type(ty),
            ReturnDef::Result { .. } => "void".to_string(),
        }
    }

    fn return_strategy(&self, returns: &ReturnDef) -> JavaReturnStrategy {
        match returns {
            ReturnDef::Void | ReturnDef::Result { .. } => JavaReturnStrategy::Void,
            ReturnDef::Value(ty) => match ty {
                TypeExpr::Void => JavaReturnStrategy::Void,
                TypeExpr::Primitive(_) => JavaReturnStrategy::Direct,
                TypeExpr::String => JavaReturnStrategy::WireDecode {
                    decode_expr: "reader.readString()".to_string(),
                },
                TypeExpr::Record(id) => JavaReturnStrategy::WireDecode {
                    decode_expr: format!(
                        "{}.decode(reader)",
                        NamingConvention::class_name(id.as_str())
                    ),
                },
                _ => JavaReturnStrategy::Void,
            },
        }
    }

    fn abi_call_for_function(&self, func: &FunctionDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|c| matches!(&c.id, CallId::Function(id) if id == &func.id))
            .expect("abi call not found for function")
    }

    fn java_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => mappings::java_type(*p).to_string(),
            TypeExpr::String => "String".to_string(),
            TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
            _ => "Object".to_string(),
        }
    }
}
