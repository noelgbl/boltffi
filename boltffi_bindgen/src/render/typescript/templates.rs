use askama::Template;

use super::plan::*;

pub fn ts_doc_block(doc: &Option<String>, indent: &str) -> String {
    match doc {
        Some(text) => {
            let mut result = format!("{indent}/**\n");
            text.lines().for_each(|line| {
                if line.is_empty() {
                    result.push_str(&format!("{indent} *\n"));
                } else {
                    result.push_str(&format!("{indent} * {line}\n"));
                }
            });
            result.push_str(&format!("{indent} */\n"));
            result
        }
        None => String::new(),
    }
}

#[derive(Template)]
#[template(path = "render_typescript/preamble.txt", escape = "none")]
pub struct PreambleTemplate {
    pub abi_version: u32,
}

#[derive(Template)]
#[template(path = "render_typescript/record.txt", escape = "none")]
pub struct RecordTemplate<'a> {
    pub name: &'a str,
    pub fields: &'a [TsField],
    pub is_blittable: bool,
    pub wire_size: Option<usize>,
    pub doc: &'a Option<String>,
}

impl<'a> RecordTemplate<'a> {
    pub fn from_record(record: &'a TsRecord) -> Self {
        Self {
            name: &record.name,
            fields: &record.fields,
            is_blittable: record.is_blittable,
            wire_size: record.wire_size,
            doc: &record.doc,
        }
    }
}

#[derive(Template)]
#[template(path = "render_typescript/enum_c_style.txt", escape = "none")]
pub struct EnumCStyleTemplate<'a> {
    pub name: &'a str,
    pub variants: &'a [TsVariant],
    pub doc: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "render_typescript/enum_data.txt", escape = "none")]
pub struct EnumDataTemplate<'a> {
    pub name: &'a str,
    pub variants: &'a [TsVariant],
    pub doc: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "render_typescript/function.txt", escape = "none")]
pub struct FunctionTemplate<'a> {
    pub name: &'a str,
    pub params: &'a [TsParam],
    pub return_type_str: &'a str,
    pub return_abi: &'a TsReturnAbi,
    pub ffi_name: &'a str,
    pub call_args: &'a str,
    pub wrapper_code: &'a str,
    pub cleanup_code: &'a str,
    pub decode_expr: &'a str,
    pub doc: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "render_typescript/wasm_exports.txt", escape = "none")]
pub struct WasmExportsTemplate<'a> {
    pub wasm_imports: &'a [TsWasmImportView<'a>],
}

pub struct TsWasmImportView<'a> {
    pub ffi_name: &'a str,
    pub params: &'a [TsWasmParam],
    pub return_wasm_type_str: &'a str,
}

pub struct TypeScriptEmitter;

impl TypeScriptEmitter {
    pub fn emit(module: &TsModule) -> String {
        let mut output = String::new();

        output.push_str(
            &PreambleTemplate {
                abi_version: module.abi_version,
            }
            .render()
            .unwrap(),
        );
        output.push('\n');

        for record in &module.records {
            output.push_str(&RecordTemplate::from_record(record).render().unwrap());
            output.push_str("\n\n");
        }

        for enumeration in &module.enums {
            if enumeration.is_c_style() {
                output.push_str(
                    &EnumCStyleTemplate {
                        name: &enumeration.name,
                        variants: &enumeration.variants,
                        doc: &enumeration.doc,
                    }
                    .render()
                    .unwrap(),
                );
            } else {
                output.push_str(
                    &EnumDataTemplate {
                        name: &enumeration.name,
                        variants: &enumeration.variants,
                        doc: &enumeration.doc,
                    }
                    .render()
                    .unwrap(),
                );
            }
            output.push_str("\n\n");
        }

        for function in &module.functions {
            let call_args = function
                .params
                .iter()
                .flat_map(|p| p.ffi_args())
                .collect::<Vec<_>>()
                .join(", ");

            let wrapper_code = function
                .params
                .iter()
                .filter_map(|p| p.wrapper_code())
                .collect::<Vec<_>>()
                .join("\n  ");

            let cleanup_code = function
                .params
                .iter()
                .filter_map(|p| p.cleanup_code())
                .collect::<Vec<_>>()
                .join("\n  ");

            let return_type_str = function.return_type.as_deref().unwrap_or("void");

            output.push_str(
                &FunctionTemplate {
                    name: &function.name,
                    params: &function.params,
                    return_type_str,
                    return_abi: &function.return_abi,
                    ffi_name: &function.ffi_name,
                    call_args: &call_args,
                    wrapper_code: &wrapper_code,
                    cleanup_code: &cleanup_code,
                    decode_expr: &function.decode_expr,
                    doc: &function.doc,
                }
                .render()
                .unwrap(),
            );
            output.push_str("\n\n");
        }

        let wasm_import_views: Vec<TsWasmImportView> = module
            .wasm_imports
            .iter()
            .map(|import| TsWasmImportView {
                ffi_name: &import.ffi_name,
                params: &import.params,
                return_wasm_type_str: import.return_wasm_type.as_deref().unwrap_or("void"),
            })
            .collect();

        output.push_str(
            &WasmExportsTemplate {
                wasm_imports: &wasm_import_views,
            }
            .render()
            .unwrap(),
        );
        output.push('\n');

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::ids::FieldName;
    use crate::ir::ops::{
        OffsetExpr, ReadOp, ReadSeq, SizeExpr, ValueExpr, WireShape, WriteOp, WriteSeq,
    };
    use crate::ir::types::PrimitiveType;

    fn primitive_size(p: PrimitiveType) -> usize {
        match p {
            PrimitiveType::Bool | PrimitiveType::I8 | PrimitiveType::U8 => 1,
            PrimitiveType::I16 | PrimitiveType::U16 => 2,
            PrimitiveType::I32 | PrimitiveType::U32 | PrimitiveType::F32 => 4,
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::F64
            | PrimitiveType::ISize
            | PrimitiveType::USize => 8,
        }
    }

    fn primitive_read(primitive: PrimitiveType) -> ReadSeq {
        ReadSeq {
            size: SizeExpr::Fixed(primitive_size(primitive)),
            ops: vec![ReadOp::Primitive {
                primitive,
                offset: OffsetExpr::Base,
            }],
            shape: WireShape::Value,
        }
    }

    fn primitive_write(primitive: PrimitiveType, field: &str) -> WriteSeq {
        WriteSeq {
            size: SizeExpr::Fixed(primitive_size(primitive)),
            ops: vec![WriteOp::Primitive {
                primitive,
                value: ValueExpr::Field(
                    Box::new(ValueExpr::Var("value".to_string())),
                    FieldName::new(field),
                ),
            }],
            shape: WireShape::Value,
        }
    }

    fn string_read() -> ReadSeq {
        ReadSeq {
            size: SizeExpr::Runtime,
            ops: vec![ReadOp::String {
                offset: OffsetExpr::Base,
            }],
            shape: WireShape::Value,
        }
    }

    fn string_write(field: &str) -> WriteSeq {
        WriteSeq {
            size: SizeExpr::StringLen(ValueExpr::Field(
                Box::new(ValueExpr::Var("value".to_string())),
                FieldName::new(field),
            )),
            ops: vec![WriteOp::String {
                value: ValueExpr::Field(
                    Box::new(ValueExpr::Var("value".to_string())),
                    FieldName::new(field),
                ),
            }],
            shape: WireShape::Value,
        }
    }

    #[test]
    fn snapshot_preamble() {
        let output = PreambleTemplate { abi_version: 1 }.render().unwrap();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn snapshot_record_with_primitive_fields() {
        let record = TsRecord {
            name: "Point".to_string(),
            fields: vec![
                TsField {
                    name: "x".to_string(),
                    ts_type: "number".to_string(),
                    decode: primitive_read(PrimitiveType::F64),
                    encode: primitive_write(PrimitiveType::F64, "x"),
                    doc: None,
                },
                TsField {
                    name: "y".to_string(),
                    ts_type: "number".to_string(),
                    decode: primitive_read(PrimitiveType::F64),
                    encode: primitive_write(PrimitiveType::F64, "y"),
                    doc: None,
                },
            ],
            is_blittable: true,
            wire_size: Some(16),
            doc: None,
        };

        let template = RecordTemplate::from_record(&record);
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_record_with_string_field() {
        let record = TsRecord {
            name: "User".to_string(),
            fields: vec![
                TsField {
                    name: "id".to_string(),
                    ts_type: "number".to_string(),
                    decode: primitive_read(PrimitiveType::I32),
                    encode: primitive_write(PrimitiveType::I32, "id"),
                    doc: None,
                },
                TsField {
                    name: "name".to_string(),
                    ts_type: "string".to_string(),
                    decode: string_read(),
                    encode: string_write("name"),
                    doc: Some("The user's display name".to_string()),
                },
            ],
            is_blittable: false,
            wire_size: None,
            doc: Some("A user record".to_string()),
        };

        let template = RecordTemplate::from_record(&record);
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_enum_c_style() {
        let doc = Some("A color enum".to_string());
        let variants = vec![
            TsVariant {
                name: "Red".to_string(),
                discriminant: 0,
                fields: vec![],
                doc: None,
            },
            TsVariant {
                name: "Green".to_string(),
                discriminant: 1,
                fields: vec![],
                doc: None,
            },
            TsVariant {
                name: "Blue".to_string(),
                discriminant: 2,
                fields: vec![],
                doc: Some("The blue channel".to_string()),
            },
        ];
        let template = EnumCStyleTemplate {
            name: "Color",
            variants: &variants,
            doc: &doc,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_enum_data() {
        let doc: Option<String> = None;
        let variants = vec![
            TsVariant {
                name: "Circle".to_string(),
                discriminant: 0,
                fields: vec![TsVariantField {
                    name: "radius".to_string(),
                    ts_type: "number".to_string(),
                    decode: primitive_read(PrimitiveType::F64),
                    encode: primitive_write(PrimitiveType::F64, "radius"),
                }],
                doc: None,
            },
            TsVariant {
                name: "Rectangle".to_string(),
                discriminant: 1,
                fields: vec![
                    TsVariantField {
                        name: "width".to_string(),
                        ts_type: "number".to_string(),
                        decode: primitive_read(PrimitiveType::F64),
                        encode: primitive_write(PrimitiveType::F64, "width"),
                    },
                    TsVariantField {
                        name: "height".to_string(),
                        ts_type: "number".to_string(),
                        decode: primitive_read(PrimitiveType::F64),
                        encode: primitive_write(PrimitiveType::F64, "height"),
                    },
                ],
                doc: None,
            },
            TsVariant {
                name: "Nothing".to_string(),
                discriminant: 2,
                fields: vec![],
                doc: Some("An empty shape".to_string()),
            },
        ];
        let template = EnumDataTemplate {
            name: "Shape",
            variants: &variants,
            doc: &doc,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_function_void() {
        let doc: Option<String> = None;
        let template = FunctionTemplate {
            name: "reset",
            params: &[],
            return_type_str: "void",
            return_abi: &TsReturnAbi::Void,
            ffi_name: "boltffi_reset",
            call_args: "",
            wrapper_code: "",
            cleanup_code: "",
            decode_expr: "",
            doc: &doc,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_function_direct_return() {
        let doc = Some("Adds two numbers".to_string());
        let params = vec![
            TsParam {
                name: "a".to_string(),
                ts_type: "number".to_string(),
                conversion: TsParamConversion::Direct,
            },
            TsParam {
                name: "b".to_string(),
                ts_type: "number".to_string(),
                conversion: TsParamConversion::Direct,
            },
        ];
        let template = FunctionTemplate {
            name: "add",
            params: &params,
            return_type_str: "number",
            return_abi: &TsReturnAbi::Direct {
                ts_cast: String::new(),
            },
            ffi_name: "boltffi_add",
            call_args: "a, b",
            wrapper_code: "",
            cleanup_code: "",
            decode_expr: "",
            doc: &doc,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_function_wire_encoded_return() {
        let doc: Option<String> = None;
        let template = FunctionTemplate {
            name: "getUsers",
            params: &[],
            return_type_str: "User[]",
            return_abi: &TsReturnAbi::WireEncoded,
            ffi_name: "boltffi_get_users",
            call_args: "",
            wrapper_code: "",
            cleanup_code: "",
            decode_expr: "reader.readArray(() => decodeUser(reader))",
            doc: &doc,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_wasm_exports() {
        let params = vec![
            TsWasmParam {
                name: "a".to_string(),
                wasm_type: "number".to_string(),
            },
            TsWasmParam {
                name: "b".to_string(),
                wasm_type: "number".to_string(),
            },
        ];
        let imports = vec![TsWasmImportView {
            ffi_name: "boltffi_add",
            params: &params,
            return_wasm_type_str: "number",
        }];
        let template = WasmExportsTemplate {
            wasm_imports: &imports,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }
}
