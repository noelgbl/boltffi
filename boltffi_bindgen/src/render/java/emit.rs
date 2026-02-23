use crate::ir::abi::AbiContract;
use crate::ir::contract::FfiContract;
use crate::ir::ops::{ReadOp, ReadSeq, SizeExpr, ValueExpr, WriteOp, WriteSeq};
use crate::ir::types::PrimitiveType;

use super::JavaOptions;
use super::lower::JavaLowerer;
use super::names::NamingConvention;
use super::templates::{FunctionsTemplate, NativeTemplate, PreambleTemplate, RecordTemplate};
use askama::Template;

pub struct JavaFile {
    pub file_name: String,
    pub source: String,
}

pub struct JavaOutput {
    pub files: Vec<JavaFile>,
    pub class_name: String,
    pub package_path: String,
}

pub struct JavaEmitter;

impl JavaEmitter {
    pub fn emit(
        ffi: &FfiContract,
        abi: &AbiContract,
        package_name: String,
        module_name: String,
        options: JavaOptions,
    ) -> JavaOutput {
        let lowerer = JavaLowerer::new(ffi, abi, package_name, module_name, options);
        let module = lowerer.module();
        let package_path = module.package_path();
        let class_name = module.class_name.clone();

        let mut files = Vec::new();

        for record in &module.records {
            let record_template = RecordTemplate {
                record,
                package_name: &module.package_name,
            };
            files.push(JavaFile {
                file_name: format!("{}.java", record.class_name),
                source: record_template.render().expect("record template failed"),
            });
        }

        let mut main_source = String::new();

        let preamble = PreambleTemplate { module: &module };
        main_source.push_str(&preamble.render().expect("preamble template failed"));

        let native = NativeTemplate { module: &module };
        main_source.push_str(&native.render().expect("native template failed"));

        main_source.push('\n');

        let functions = FunctionsTemplate { module: &module };
        main_source.push_str(&functions.render().expect("functions template failed"));

        files.push(JavaFile {
            file_name: format!("{}.java", class_name),
            source: main_source,
        });

        JavaOutput {
            files,
            class_name,
            package_path,
        }
    }
}

fn render_value(expr: &ValueExpr) -> String {
    match expr {
        ValueExpr::Instance => String::new(),
        ValueExpr::Var(name) => name.clone(),
        ValueExpr::Named(name) => NamingConvention::field_name(name),
        ValueExpr::Field(parent, field) => {
            let parent_str = render_value(parent);
            let field_str = NamingConvention::field_name(field.as_str());
            if parent_str.is_empty() {
                field_str
            } else {
                format!("{}.{}", parent_str, field_str)
            }
        }
    }
}

fn primitive_read_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "readBool",
        PrimitiveType::I8 | PrimitiveType::U8 => "readI8",
        PrimitiveType::I16 | PrimitiveType::U16 => "readI16",
        PrimitiveType::I32 | PrimitiveType::U32 => "readI32",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "readI64"
        }
        PrimitiveType::F32 => "readF32",
        PrimitiveType::F64 => "readF64",
    }
}

fn primitive_write_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "writeBool",
        PrimitiveType::I8 | PrimitiveType::U8 => "writeI8",
        PrimitiveType::I16 | PrimitiveType::U16 => "writeI16",
        PrimitiveType::I32 | PrimitiveType::U32 => "writeI32",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "writeI64"
        }
        PrimitiveType::F32 => "writeF32",
        PrimitiveType::F64 => "writeF64",
    }
}

pub fn emit_reader_read(seq: &ReadSeq) -> String {
    let op = seq.ops.first().expect("read ops");
    match op {
        ReadOp::Primitive { primitive, .. } => {
            format!("reader.{}()", primitive_read_method(*primitive))
        }
        ReadOp::String { .. } => "reader.readString()".to_string(),
        ReadOp::Bytes { .. } => "reader.readBytes()".to_string(),
        ReadOp::Record { id, .. } => {
            format!(
                "{}.decode(reader)",
                NamingConvention::class_name(id.as_str())
            )
        }
        other => panic!("unsupported Java read op: {:?}", other),
    }
}

pub fn emit_write_expr(seq: &WriteSeq, writer_name: &str) -> String {
    let op = seq.ops.first().expect("write ops");
    match op {
        WriteOp::Primitive { primitive, value } => {
            format!(
                "{}.{}({})",
                writer_name,
                primitive_write_method(*primitive),
                render_value(value)
            )
        }
        WriteOp::String { value } => {
            format!("{}.writeString({})", writer_name, render_value(value))
        }
        WriteOp::Bytes { value } => {
            format!("{}.writeBytes({})", writer_name, render_value(value))
        }
        WriteOp::Record { value, .. } => {
            format!("{}.wireEncodeTo({})", render_value(value), writer_name)
        }
        other => panic!("unsupported Java write op: {:?}", other),
    }
}

fn emit_size_expr(size: &SizeExpr) -> String {
    match size {
        SizeExpr::Fixed(value) => value.to_string(),
        SizeExpr::StringLen(value) => {
            format!("WireWriter.stringWireSize({})", render_value(value))
        }
        SizeExpr::BytesLen(value) => {
            format!("(4 + {}.length)", render_value(value))
        }
        SizeExpr::WireSize { value, .. } => {
            format!("{}.wireEncodedSize()", render_value(value))
        }
        SizeExpr::Sum(parts) => {
            let rendered = parts
                .iter()
                .map(emit_size_expr)
                .collect::<Vec<_>>()
                .join(" + ");
            format!("({})", rendered)
        }
        other => panic!("unsupported Java size expr: {:?}", other),
    }
}

pub fn emit_size_expr_for_write_seq(seq: &WriteSeq) -> String {
    emit_size_expr(&seq.size)
}
