use crate::ir::ops::{ReadSeq, WriteSeq};
use crate::render::typescript::emit;

#[derive(Debug, Clone)]
pub struct TsModule {
    pub module_name: String,
    pub abi_version: u32,
    pub records: Vec<TsRecord>,
    pub enums: Vec<TsEnum>,
    pub functions: Vec<TsFunction>,
    pub wasm_imports: Vec<TsWasmImport>,
}

#[derive(Debug, Clone)]
pub struct TsRecord {
    pub name: String,
    pub fields: Vec<TsField>,
    pub is_blittable: bool,
    pub wire_size: Option<usize>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TsField {
    pub name: String,
    pub ts_type: String,
    pub decode: ReadSeq,
    pub encode: WriteSeq,
    pub doc: Option<String>,
}

impl TsField {
    pub fn wire_decode_expr(&self) -> String {
        emit::emit_reader_read(&self.decode)
    }

    pub fn wire_encode_expr(&self, writer: &str, value: &str) -> String {
        emit::emit_writer_write(&self.encode, writer, value)
    }

    pub fn wire_size_expr(&self, value: &str) -> String {
        emit::emit_size_expr(&self.encode.size, value)
    }
}

#[derive(Debug, Clone)]
pub struct TsEnum {
    pub name: String,
    pub variants: Vec<TsVariant>,
    pub kind: TsEnumKind,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum TsEnumKind {
    CStyle,
    Data,
}

impl TsEnum {
    pub fn is_c_style(&self) -> bool {
        matches!(self.kind, TsEnumKind::CStyle)
    }
}

#[derive(Debug, Clone)]
pub struct TsVariant {
    pub name: String,
    pub discriminant: i64,
    pub fields: Vec<TsVariantField>,
    pub doc: Option<String>,
}

impl TsVariant {
    pub fn is_unit(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn size_expr(&self) -> String {
        if self.fields.is_empty() {
            "4".to_string()
        } else {
            let field_sizes: Vec<String> =
                self.fields.iter().map(|f| f.wire_size_expr("v")).collect();
            format!("4 + {}", field_sizes.join(" + "))
        }
    }
}

#[derive(Debug, Clone)]
pub struct TsVariantField {
    pub name: String,
    pub ts_type: String,
    pub decode: ReadSeq,
    pub encode: WriteSeq,
}

impl TsVariantField {
    pub fn wire_decode_expr(&self) -> String {
        emit::emit_reader_read(&self.decode)
    }

    pub fn wire_encode_expr(&self, writer: &str, value: &str) -> String {
        emit::emit_writer_write(&self.encode, writer, value)
    }

    pub fn wire_size_expr(&self, value: &str) -> String {
        emit::emit_size_expr(&self.encode.size, value)
    }
}

#[derive(Debug, Clone)]
pub struct TsFunction {
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<TsParam>,
    pub return_type: Option<String>,
    pub return_abi: TsReturnAbi,
    pub decode_expr: String,
    pub throws: bool,
    pub err_type: String,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TsParam {
    pub name: String,
    pub ts_type: String,
    pub conversion: TsParamConversion,
}

impl TsParam {
    pub fn wrapper_code(&self) -> Option<String> {
        match &self.conversion {
            TsParamConversion::Direct => None,
            TsParamConversion::String => Some(format!(
                "const {}_alloc = _module.allocString({});",
                self.name, self.name
            )),
            TsParamConversion::CodecEncoded { codec_name } => {
                let writer_name = format!("{}_writer", self.name);
                Some(format!(
                    "const {writer_name} = _module.allocWriter({codec_name}Codec.size({}));\n  {codec_name}Codec.encode({writer_name}, {});",
                    self.name, self.name
                ))
            }
            TsParamConversion::OtherEncoded { encode } => {
                let writer_name = format!("{}_writer", self.name);
                let size_expr = emit::emit_size_expr(&encode.size, &self.name);
                let encode_expr = emit::emit_writer_write(encode, &writer_name, &self.name);
                Some(format!(
                    "const {writer_name} = _module.allocWriter({size_expr});\n  {encode_expr};",
                ))
            }
        }
    }

    pub fn ffi_args(&self) -> Vec<String> {
        match &self.conversion {
            TsParamConversion::Direct => vec![self.name.clone()],
            TsParamConversion::String => {
                vec![
                    format!("{}_alloc.ptr", self.name),
                    format!("{}_alloc.len", self.name),
                ]
            }
            TsParamConversion::CodecEncoded { .. } | TsParamConversion::OtherEncoded { .. } => {
                vec![
                    format!("{}_writer.ptr", self.name),
                    format!("{}_writer.len", self.name),
                ]
            }
        }
    }

    pub fn cleanup_code(&self) -> Option<String> {
        match &self.conversion {
            TsParamConversion::Direct => None,
            TsParamConversion::String => Some(format!("_module.freeAlloc({}_alloc);", self.name)),
            TsParamConversion::CodecEncoded { .. } | TsParamConversion::OtherEncoded { .. } => {
                Some(format!("_module.freeWriter({}_writer);", self.name))
            }
        }
    }

    pub fn needs_cleanup(&self) -> bool {
        !matches!(self.conversion, TsParamConversion::Direct)
    }
}

#[derive(Debug, Clone)]
pub enum TsParamConversion {
    Direct,
    String,
    CodecEncoded { codec_name: String },
    OtherEncoded { encode: WriteSeq },
}

#[derive(Debug, Clone)]
pub enum TsReturnAbi {
    Void,
    Direct { ts_cast: String },
    WireEncoded,
}

impl TsReturnAbi {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct { .. })
    }

    pub fn is_wire_encoded(&self) -> bool {
        matches!(self, Self::WireEncoded)
    }
}

#[derive(Debug, Clone)]
pub struct TsWasmImport {
    pub ffi_name: String,
    pub params: Vec<TsWasmParam>,
    pub return_wasm_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TsWasmParam {
    pub name: String,
    pub wasm_type: String,
}
