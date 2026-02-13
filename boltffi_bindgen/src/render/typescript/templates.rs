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
#[template(path = "render_typescript/preamble_node.txt", escape = "none")]
pub struct NodePreambleTemplate {
    pub abi_version: u32,
    pub module_name: String,
}

#[derive(Template)]
#[template(path = "render_typescript/footer_node.txt", escape = "none")]
pub struct NodeFooterTemplate;

#[derive(Template)]
#[template(path = "render_typescript/record.txt", escape = "none")]
pub struct RecordTemplate<'a> {
    pub name: &'a str,
    pub fields: &'a [TsField],
    pub is_blittable: bool,
    pub wire_size: Option<usize>,
    pub tail_padding: usize,
    pub size_expr: String,
    pub doc: &'a Option<String>,
}

impl<'a> RecordTemplate<'a> {
    pub fn from_record(record: &'a TsRecord) -> Self {
        let size_expr = if let Some(size) = record.wire_size {
            size.to_string()
        } else {
            record
                .fields
                .iter()
                .map(|f| f.wire_size_expr("v"))
                .collect::<Vec<_>>()
                .join(" + ")
        };
        Self {
            name: &record.name,
            fields: &record.fields,
            is_blittable: record.is_blittable,
            wire_size: record.wire_size,
            tail_padding: record.tail_padding,
            size_expr,
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
#[template(path = "render_typescript/error_exception.txt", escape = "none")]
pub struct ErrorExceptionTemplate<'a> {
    pub type_name: &'a str,
    pub class_name: &'a str,
    pub is_c_style_enum: bool,
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
    pub call_args_with_out: &'a str,
    pub wrapper_code: &'a str,
    pub cleanup_code: &'a str,
    pub decode_expr: &'a str,
    pub doc: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "render_typescript/class.txt", escape = "none")]
pub struct ClassTemplate<'a> {
    pub cls: &'a TsClass,
}

#[derive(Template)]
#[template(path = "render_typescript/callback.txt", escape = "none")]
pub struct CallbackTemplate<'a> {
    pub callback: &'a TsCallback,
}

#[derive(Template)]
#[template(path = "render_typescript/async_function.txt", escape = "none")]
pub struct AsyncFunctionTemplate<'a> {
    pub name: &'a str,
    pub params: &'a [TsParam],
    pub return_type_str: &'a str,
    pub entry_ffi_name: &'a str,
    pub poll_sync_ffi_name: &'a str,
    pub complete_ffi_name: &'a str,
    pub panic_message_ffi_name: &'a str,
    pub free_ffi_name: &'a str,
    pub call_args: &'a str,
    pub wrapper_code: &'a str,
    pub cleanup_code: &'a str,
    pub decode_expr: &'a str,
    pub has_return: bool,
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

        for error_exception in &module.error_exceptions {
            output.push_str(
                &ErrorExceptionTemplate {
                    type_name: &error_exception.type_name,
                    class_name: &error_exception.class_name,
                    is_c_style_enum: error_exception.is_c_style_enum,
                }
                .render()
                .unwrap(),
            );
            output.push_str("\n\n");
        }

        for function in &module.functions {
            let call_args = function
                .params
                .iter()
                .flat_map(|p| p.ffi_args())
                .collect::<Vec<_>>()
                .join(", ");
            let call_args_with_out = if call_args.is_empty() {
                "outPtr".to_string()
            } else {
                format!("outPtr, {call_args}")
            };

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
                    call_args_with_out: &call_args_with_out,
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

        for async_function in &module.async_functions {
            let call_args = async_function
                .params
                .iter()
                .flat_map(|p| p.ffi_args())
                .collect::<Vec<_>>()
                .join(", ");

            let wrapper_code = async_function
                .params
                .iter()
                .filter_map(|p| p.wrapper_code())
                .collect::<Vec<_>>()
                .join("\n    ");

            let cleanup_code = async_function
                .params
                .iter()
                .filter_map(|p| p.cleanup_code())
                .collect::<Vec<_>>()
                .join("\n    ");

            let return_type_str = async_function.return_type.as_deref().unwrap_or("void");
            let has_return = async_function.return_type.is_some();

            output.push_str(
                &AsyncFunctionTemplate {
                    name: &async_function.name,
                    params: &async_function.params,
                    return_type_str,
                    entry_ffi_name: &async_function.entry_ffi_name,
                    poll_sync_ffi_name: &async_function.poll_sync_ffi_name,
                    complete_ffi_name: &async_function.complete_ffi_name,
                    panic_message_ffi_name: &async_function.panic_message_ffi_name,
                    free_ffi_name: &async_function.free_ffi_name,
                    call_args: &call_args,
                    wrapper_code: &wrapper_code,
                    cleanup_code: &cleanup_code,
                    decode_expr: &async_function.decode_expr,
                    has_return,
                    doc: &async_function.doc,
                }
                .render()
                .unwrap(),
            );
            output.push_str("\n\n");
        }

        for class in &module.classes {
            output.push_str(&ClassTemplate { cls: class }.render().unwrap());
            output.push_str("\n\n");
        }

        for callback in &module.callbacks {
            output.push_str(&CallbackTemplate { callback }.render().unwrap());
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

    pub fn emit_node(module: &TsModule, module_name: &str) -> String {
        let mut output = String::new();

        output.push_str(
            &NodePreambleTemplate {
                abi_version: module.abi_version,
                module_name: module_name.to_string(),
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

        for error_exception in &module.error_exceptions {
            output.push_str(
                &ErrorExceptionTemplate {
                    type_name: &error_exception.type_name,
                    class_name: &error_exception.class_name,
                    is_c_style_enum: error_exception.is_c_style_enum,
                }
                .render()
                .unwrap(),
            );
            output.push_str("\n\n");
        }

        for callback in &module.callbacks {
            output.push_str(&CallbackTemplate { callback }.render().unwrap());
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

        output.push_str(&NodeFooterTemplate.render().unwrap());
        output.push_str("\n\n");

        for function in &module.functions {
            let call_args = function
                .params
                .iter()
                .flat_map(|p| p.ffi_args())
                .collect::<Vec<_>>()
                .join(", ");
            let call_args_with_out = if call_args.is_empty() {
                "outPtr".to_string()
            } else {
                format!("outPtr, {call_args}")
            };

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
                    call_args_with_out: &call_args_with_out,
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

        for async_function in &module.async_functions {
            let call_args = async_function
                .params
                .iter()
                .flat_map(|p| p.ffi_args())
                .collect::<Vec<_>>()
                .join(", ");

            let wrapper_code = async_function
                .params
                .iter()
                .filter_map(|p| p.wrapper_code())
                .collect::<Vec<_>>()
                .join("\n  ");

            let cleanup_code = async_function
                .params
                .iter()
                .filter_map(|p| p.cleanup_code())
                .collect::<Vec<_>>()
                .join("\n  ");

            let return_type_str = async_function.return_type.as_deref().unwrap_or("void");
            let has_return = async_function.return_type.is_some();

            output.push_str(
                &AsyncFunctionTemplate {
                    name: &async_function.name,
                    params: &async_function.params,
                    return_type_str,
                    entry_ffi_name: &async_function.entry_ffi_name,
                    poll_sync_ffi_name: &async_function.poll_sync_ffi_name,
                    complete_ffi_name: &async_function.complete_ffi_name,
                    panic_message_ffi_name: &async_function.panic_message_ffi_name,
                    free_ffi_name: &async_function.free_ffi_name,
                    call_args: &call_args,
                    wrapper_code: &wrapper_code,
                    cleanup_code: &cleanup_code,
                    decode_expr: &async_function.decode_expr,
                    has_return,
                    doc: &async_function.doc,
                }
                .render()
                .unwrap(),
            );
            output.push_str("\n\n");
        }

        for class in &module.classes {
            output.push_str(&ClassTemplate { cls: class }.render().unwrap());
            output.push_str("\n\n");
        }

        output
    }
}

#[cfg(all(test, not(miri)))]
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
            call_args_with_out: "outPtr",
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
            call_args_with_out: "outPtr, a, b",
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
            call_args_with_out: "outPtr",
            wrapper_code: "",
            cleanup_code: "",
            decode_expr: "reader.readArray(() => decodeUser(reader))",
            doc: &doc,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    fn sync_callback_fixture() -> TsCallback {
        TsCallback {
            interface_name: "ValueHandler".to_string(),
            trait_name_snake: "value_handler".to_string(),
            create_handle_fn: "boltffi_create_value_handler_handle".to_string(),
            methods: vec![TsCallbackMethod {
                ts_name: "onValue".to_string(),
                import_name: "__boltffi_callback_value_handler_on_value".to_string(),
                params: vec![TsCallbackParam {
                    name: "value".to_string(),
                    ts_type: "number".to_string(),
                    kind: TsCallbackParamKind::Primitive {
                        import_ts_type: "number".to_string(),
                        call_expr: "value".to_string(),
                    },
                }],
                return_kind: TsCallbackReturnKind::Primitive {
                    ts_type: "number".to_string(),
                },
                doc: None,
            }],
            async_methods: vec![],
            closure_fn_type: None,
            doc: None,
        }
    }

    fn async_callback_fixture() -> TsCallback {
        TsCallback {
            interface_name: "AsyncFetcher".to_string(),
            trait_name_snake: "async_fetcher".to_string(),
            create_handle_fn: "boltffi_create_async_fetcher_handle".to_string(),
            methods: vec![],
            async_methods: vec![TsAsyncCallbackMethod {
                ts_name: "fetch".to_string(),
                start_import_name: "__boltffi_callback_async_fetcher_fetch_start".to_string(),
                complete_export_name: "boltffi_callback_async_fetcher_fetch_complete".to_string(),
                params: vec![TsCallbackParam {
                    name: "key".to_string(),
                    ts_type: "number".to_string(),
                    kind: TsCallbackParamKind::Primitive {
                        import_ts_type: "number".to_string(),
                        call_expr: "key".to_string(),
                    },
                }],
                return_type: Some("number".to_string()),
                encode_expr: None,
                size_expr: None,
                direct_write_method: Some("writeI32".to_string()),
                direct_write_value_expr: Some("result".to_string()),
                direct_size: Some(4),
                doc: None,
            }],
            closure_fn_type: None,
            doc: None,
        }
    }

    #[test]
    fn callback_registry_emits_refcounted_lifecycle_contract() {
        let callback = sync_callback_fixture();
        let rendered = CallbackTemplate {
            callback: &callback,
        }
        .render()
        .unwrap();

        assert!(rendered.contains(
            "const _value_handler_ref_counts = new Map<number, number>();"
        ));
        assert!(rendered.contains("_value_handler_ref_counts.set(id, 1);"));
        assert!(rendered.contains("return _value_handler_retain(handle);"));
        assert!(rendered.contains("_value_handler_release(handle);"));
        assert!(rendered.contains("const impl = _value_handler_lookup(handle);"));
    }

    #[test]
    fn callback_registry_emits_invalid_handle_and_no_resurrection_guards() {
        let callback = sync_callback_fixture();
        let rendered = CallbackTemplate {
            callback: &callback,
        }
        .render()
        .unwrap();

        assert!(rendered.contains(
            "Cannot clone unknown callback handle ${handle} in ValueHandler registry"
        ));
        assert!(rendered.contains(
            "Cannot free unknown callback handle ${handle} in ValueHandler registry"
        ));
        assert!(rendered.contains(
            "Callback handle ${handle} not found in ValueHandler registry"
        ));
        assert!(rendered.contains("if (currentCount === 1) {"));
        assert!(rendered.contains("_value_handler_ref_counts.delete(handle);"));
        assert!(rendered.contains("_value_handler_registry.delete(handle);"));
        assert!(rendered.contains("return handle;"));
    }

    #[test]
    fn async_callback_invalid_handle_is_reported_through_completion() {
        let callback = async_callback_fixture();
        let rendered = CallbackTemplate {
            callback: &callback,
        }
        .render()
        .unwrap();

        assert!(rendered.contains("let impl: AsyncFetcher;"));
        assert!(rendered.contains("impl = _async_fetcher_lookup(handle);"));
        assert!(rendered.contains("completeError(err);"));
        assert!(rendered.contains("return;"));
    }

    #[test]
    fn snapshot_class_with_constructor_and_methods() {
        let class = TsClass {
            class_name: "Counter".to_string(),
            ffi_free: "boltffi_counter_free".to_string(),
            constructors: vec![TsClassConstructor {
                ts_name: "new".to_string(),
                ffi_name: "boltffi_counter_new".to_string(),
                is_default: true,
                params: vec![],
                returns_nullable_handle: false,
                doc: Some("Creates a counter".to_string()),
            }],
            methods: vec![
                TsClassMethod {
                    ts_name: "increment".to_string(),
                    ffi_name: "boltffi_counter_increment".to_string(),
                    is_static: false,
                    params: vec![TsParam {
                        name: "delta".to_string(),
                        ts_type: "number".to_string(),
                        conversion: TsParamConversion::Direct,
                    }],
                    return_type: Some("number".to_string()),
                    return_handle: None,
                    mode: TsClassMethodMode::Sync(TsClassSyncMethod {
                        return_abi: TsReturnAbi::Direct {
                            ts_cast: String::new(),
                        },
                        decode_expr: String::new(),
                    }),
                    doc: None,
                },
                TsClassMethod {
                    ts_name: "nextValue".to_string(),
                    ffi_name: "boltffi_counter_next_value".to_string(),
                    is_static: false,
                    params: vec![],
                    return_type: Some("number".to_string()),
                    return_handle: None,
                    mode: TsClassMethodMode::Async(TsClassAsyncMethod {
                        poll_sync_ffi_name: "boltffi_counter_next_value_poll_sync".to_string(),
                        complete_ffi_name: "boltffi_counter_next_value_complete".to_string(),
                        panic_message_ffi_name: "boltffi_counter_next_value_panic_message"
                            .to_string(),
                        cancel_ffi_name: "boltffi_counter_next_value_cancel".to_string(),
                        free_ffi_name: "boltffi_counter_next_value_free".to_string(),
                        decode_expr: "reader.readI32()".to_string(),
                    }),
                    doc: None,
                },
            ],
            doc: Some("A counter class".to_string()),
        };
        let template = ClassTemplate { cls: &class };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn class_nullable_constructor_preserves_null_contract() {
        let class = TsClass {
            class_name: "Session".to_string(),
            ffi_free: "boltffi_session_free".to_string(),
            constructors: vec![TsClassConstructor {
                ts_name: "open".to_string(),
                ffi_name: "boltffi_session_open".to_string(),
                is_default: false,
                params: vec![TsParam {
                    name: "path".to_string(),
                    ts_type: "string".to_string(),
                    conversion: TsParamConversion::String,
                }],
                returns_nullable_handle: true,
                doc: None,
            }],
            methods: vec![],
            doc: None,
        };

        let rendered = ClassTemplate { cls: &class }.render().unwrap();
        assert!(rendered.contains("static open(path: string): Session | null {"));
        assert!(rendered.contains("if (handle === 0) {\n        return null;\n      }"));
    }

    #[test]
    fn class_async_return_frees_handles_on_decode_failures() {
        let class = TsClass {
            class_name: "Counter".to_string(),
            ffi_free: "boltffi_counter_free".to_string(),
            constructors: vec![],
            methods: vec![TsClassMethod {
                ts_name: "nextValue".to_string(),
                ffi_name: "boltffi_counter_next_value".to_string(),
                is_static: false,
                params: vec![],
                return_type: Some("number".to_string()),
                return_handle: None,
                mode: TsClassMethodMode::Async(TsClassAsyncMethod {
                    poll_sync_ffi_name: "boltffi_counter_next_value_poll_sync".to_string(),
                    complete_ffi_name: "boltffi_counter_next_value_complete".to_string(),
                    panic_message_ffi_name: "boltffi_counter_next_value_panic_message"
                        .to_string(),
                    cancel_ffi_name: "boltffi_counter_next_value_cancel".to_string(),
                    free_ffi_name: "boltffi_counter_next_value_free".to_string(),
                    decode_expr: "reader.readI32()".to_string(),
                }),
                doc: None,
            }],
            doc: None,
        };

        let rendered = ClassTemplate { cls: &class }.render().unwrap();
        assert!(rendered.contains("let completeCompleted = false;"));
        assert!(rendered.contains("_module.freeBuf(outPtr);"));
        assert!(rendered.contains("_module.freeBufDescriptor(outPtr);"));
        assert!(rendered.contains(
            "(_module.exports.boltffi_counter_next_value_free as Function)(awaitedHandle);"
        ));
    }

    #[test]
    fn class_async_param_cleanup_runs_before_await() {
        let class = TsClass {
            class_name: "Database".to_string(),
            ffi_free: "boltffi_database_free".to_string(),
            constructors: vec![],
            methods: vec![TsClassMethod {
                ts_name: "query".to_string(),
                ffi_name: "boltffi_database_query".to_string(),
                is_static: false,
                params: vec![TsParam {
                    name: "sql".to_string(),
                    ts_type: "string".to_string(),
                    conversion: TsParamConversion::String,
                }],
                return_type: Some("QueryResult".to_string()),
                return_handle: None,
                mode: TsClassMethodMode::Async(TsClassAsyncMethod {
                    poll_sync_ffi_name: "boltffi_database_query_poll_sync".to_string(),
                    complete_ffi_name: "boltffi_database_query_complete".to_string(),
                    panic_message_ffi_name: "boltffi_database_query_panic_message".to_string(),
                    cancel_ffi_name: "boltffi_database_query_cancel".to_string(),
                    free_ffi_name: "boltffi_database_query_free".to_string(),
                    decode_expr: "QueryResultCodec.decode(reader)".to_string(),
                }),
                doc: None,
            }],
            doc: None,
        };

        let rendered = ClassTemplate { cls: &class }.render().unwrap();
        let cleanup_index = rendered.find("_module.freeAlloc(sql_alloc);").unwrap();
        let await_index = rendered
            .find("const awaitedHandle = await _module.asyncManager.pollAsync(")
            .unwrap();
        assert!(cleanup_index < await_index);
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

    #[test]
    fn wasm_exports_renders_encoded_return_with_out_param() {
        let params = vec![
            TsWasmParam {
                name: "out".to_string(),
                wasm_type: "number".to_string(),
            },
            TsWasmParam {
                name: "payload".to_string(),
                wasm_type: "number".to_string(),
            },
        ];
        let imports = vec![TsWasmImportView {
            ffi_name: "boltffi_echo_payload",
            params: &params,
            return_wasm_type_str: "void",
        }];
        let template = WasmExportsTemplate {
            wasm_imports: &imports,
        };
        let rendered = template.render().unwrap();
        assert!(rendered.contains("boltffi_echo_payload(out: number, payload: number): void;"));
    }

    #[test]
    fn snapshot_class_with_static_method() {
        let class = TsClass {
            class_name: "MathUtils".to_string(),
            ffi_free: "boltffi_math_utils_free".to_string(),
            constructors: vec![],
            methods: vec![TsClassMethod {
                ts_name: "add".to_string(),
                ffi_name: "boltffi_math_utils_add".to_string(),
                is_static: true,
                params: vec![
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
                ],
                return_type: Some("number".to_string()),
                return_handle: None,
                mode: TsClassMethodMode::Sync(TsClassSyncMethod {
                    return_abi: TsReturnAbi::Direct {
                        ts_cast: String::new(),
                    },
                    decode_expr: String::new(),
                }),
                doc: None,
            }],
            doc: None,
        };
        let template = ClassTemplate { cls: &class };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_class_with_void_method() {
        let class = TsClass {
            class_name: "Logger".to_string(),
            ffi_free: "boltffi_logger_free".to_string(),
            constructors: vec![TsClassConstructor {
                ts_name: "new".to_string(),
                ffi_name: "boltffi_logger_new".to_string(),
                is_default: true,
                params: vec![],
                returns_nullable_handle: false,
                doc: None,
            }],
            methods: vec![TsClassMethod {
                ts_name: "log".to_string(),
                ffi_name: "boltffi_logger_log".to_string(),
                is_static: false,
                params: vec![TsParam {
                    name: "message".to_string(),
                    ts_type: "string".to_string(),
                    conversion: TsParamConversion::String,
                }],
                return_type: None,
                return_handle: None,
                mode: TsClassMethodMode::Sync(TsClassSyncMethod {
                    return_abi: TsReturnAbi::Void,
                    decode_expr: String::new(),
                }),
                doc: None,
            }],
            doc: None,
        };
        let template = ClassTemplate { cls: &class };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_class_with_handle_return() {
        let class = TsClass {
            class_name: "Factory".to_string(),
            ffi_free: "boltffi_factory_free".to_string(),
            constructors: vec![],
            methods: vec![TsClassMethod {
                ts_name: "createChild".to_string(),
                ffi_name: "boltffi_factory_create_child".to_string(),
                is_static: false,
                params: vec![],
                return_type: Some("Child".to_string()),
                return_handle: Some(TsHandleReturn {
                    class_name: "Child".to_string(),
                    nullable: false,
                }),
                mode: TsClassMethodMode::Sync(TsClassSyncMethod {
                    return_abi: TsReturnAbi::Direct {
                        ts_cast: String::new(),
                    },
                    decode_expr: String::new(),
                }),
                doc: None,
            }],
            doc: None,
        };
        let template = ClassTemplate { cls: &class };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_class_with_nullable_handle_return() {
        let class = TsClass {
            class_name: "Cache".to_string(),
            ffi_free: "boltffi_cache_free".to_string(),
            constructors: vec![],
            methods: vec![TsClassMethod {
                ts_name: "get".to_string(),
                ffi_name: "boltffi_cache_get".to_string(),
                is_static: false,
                params: vec![TsParam {
                    name: "key".to_string(),
                    ts_type: "string".to_string(),
                    conversion: TsParamConversion::String,
                }],
                return_type: Some("Entry | null".to_string()),
                return_handle: Some(TsHandleReturn {
                    class_name: "Entry".to_string(),
                    nullable: true,
                }),
                mode: TsClassMethodMode::Sync(TsClassSyncMethod {
                    return_abi: TsReturnAbi::Direct {
                        ts_cast: String::new(),
                    },
                    decode_expr: String::new(),
                }),
                doc: None,
            }],
            doc: None,
        };
        let template = ClassTemplate { cls: &class };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_class_with_encoded_param() {
        let class = TsClass {
            class_name: "Renderer".to_string(),
            ffi_free: "boltffi_renderer_free".to_string(),
            constructors: vec![],
            methods: vec![TsClassMethod {
                ts_name: "draw".to_string(),
                ffi_name: "boltffi_renderer_draw".to_string(),
                is_static: false,
                params: vec![TsParam {
                    name: "point".to_string(),
                    ts_type: "Point".to_string(),
                    conversion: TsParamConversion::CodecEncoded {
                        codec_name: "Point".to_string(),
                    },
                }],
                return_type: None,
                return_handle: None,
                mode: TsClassMethodMode::Sync(TsClassSyncMethod {
                    return_abi: TsReturnAbi::Void,
                    decode_expr: String::new(),
                }),
                doc: None,
            }],
            doc: None,
        };
        let template = ClassTemplate { cls: &class };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_class_async_with_encoded_return() {
        let class = TsClass {
            class_name: "Database".to_string(),
            ffi_free: "boltffi_database_free".to_string(),
            constructors: vec![],
            methods: vec![TsClassMethod {
                ts_name: "query".to_string(),
                ffi_name: "boltffi_database_query".to_string(),
                is_static: false,
                params: vec![TsParam {
                    name: "sql".to_string(),
                    ts_type: "string".to_string(),
                    conversion: TsParamConversion::String,
                }],
                return_type: Some("QueryResult".to_string()),
                return_handle: None,
                mode: TsClassMethodMode::Async(TsClassAsyncMethod {
                    poll_sync_ffi_name: "boltffi_database_query_poll_sync".to_string(),
                    complete_ffi_name: "boltffi_database_query_complete".to_string(),
                    panic_message_ffi_name: "boltffi_database_query_panic_message".to_string(),
                    cancel_ffi_name: "boltffi_database_query_cancel".to_string(),
                    free_ffi_name: "boltffi_database_query_free".to_string(),
                    decode_expr: "QueryResultCodec.decode(reader)".to_string(),
                }),
                doc: None,
            }],
            doc: None,
        };
        let template = ClassTemplate { cls: &class };
        insta::assert_snapshot!(template.render().unwrap());
    }
}
