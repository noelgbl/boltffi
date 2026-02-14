use crate::ir::ops::{ReadSeq, WriteSeq};
use crate::ir::plan::AbiType;
use crate::render::typescript::emit;

#[derive(Debug, Clone)]
pub struct TsModule {
    pub module_name: String,
    pub abi_version: u32,
    pub records: Vec<TsRecord>,
    pub enums: Vec<TsEnum>,
    pub error_exceptions: Vec<TsErrorException>,
    pub functions: Vec<TsFunction>,
    pub async_functions: Vec<TsAsyncFunction>,
    pub classes: Vec<TsClass>,
    pub callbacks: Vec<TsCallback>,
    pub wasm_imports: Vec<TsWasmImport>,
}

#[derive(Debug, Clone)]
pub struct TsErrorException {
    pub type_name: String,
    pub class_name: String,
    pub is_c_style_enum: bool,
}

#[derive(Debug, Clone)]
pub struct TsAsyncFunction {
    pub name: String,
    pub entry_ffi_name: String,
    pub poll_sync_ffi_name: String,
    pub complete_ffi_name: String,
    pub panic_message_ffi_name: String,
    pub cancel_ffi_name: String,
    pub free_ffi_name: String,
    pub params: Vec<TsParam>,
    pub return_type: Option<String>,
    pub decode_expr: String,
    pub throws: bool,
    pub err_type: String,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TsClass {
    pub class_name: String,
    pub ffi_free: String,
    pub constructors: Vec<TsClassConstructor>,
    pub methods: Vec<TsClassMethod>,
    pub doc: Option<String>,
}

impl TsClass {
    pub fn has_default_constructor(&self) -> bool {
        self.constructors
            .iter()
            .any(|constructor| constructor.is_default)
    }

    pub fn default_constructor(&self) -> Option<&TsClassConstructor> {
        self.constructors
            .iter()
            .find(|constructor| constructor.is_default)
    }

    pub fn named_constructors(&self) -> Vec<&TsClassConstructor> {
        self.constructors
            .iter()
            .filter(|constructor| !constructor.is_default)
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct TsClassConstructor {
    pub ts_name: String,
    pub ffi_name: String,
    pub is_default: bool,
    pub params: Vec<TsParam>,
    pub returns_nullable_handle: bool,
    pub doc: Option<String>,
}

impl TsClassConstructor {
    pub fn wrapper_code(&self) -> String {
        self.params
            .iter()
            .filter_map(TsParam::wrapper_code)
            .collect::<Vec<_>>()
            .join("\n    ")
    }

    pub fn cleanup_code(&self) -> String {
        self.params
            .iter()
            .filter_map(TsParam::cleanup_code)
            .collect::<Vec<_>>()
            .join("\n      ")
    }

    pub fn ffi_call_args(&self) -> String {
        flatten_ffi_args(&self.params).join(", ")
    }
}

#[derive(Debug, Clone)]
pub struct TsClassMethod {
    pub ts_name: String,
    pub ffi_name: String,
    pub is_static: bool,
    pub params: Vec<TsParam>,
    pub return_type: Option<String>,
    pub return_handle: Option<TsHandleReturn>,
    pub mode: TsClassMethodMode,
    pub doc: Option<String>,
}

impl TsClassMethod {
    pub fn wrapper_code(&self) -> String {
        self.params
            .iter()
            .filter_map(TsParam::wrapper_code)
            .collect::<Vec<_>>()
            .join("\n    ")
    }

    pub fn cleanup_code(&self) -> String {
        self.params
            .iter()
            .filter_map(TsParam::cleanup_code)
            .collect::<Vec<_>>()
            .join("\n      ")
    }

    pub fn ffi_call_args(&self) -> String {
        let mut call_args = Vec::new();
        if !self.is_static {
            call_args.push("this._handle".to_string());
        }
        call_args.extend(flatten_ffi_args(&self.params));
        call_args.join(", ")
    }

    pub fn ffi_call_args_with_out(&self) -> String {
        let call_args = self.ffi_call_args();
        if call_args.is_empty() {
            "outPtr".to_string()
        } else {
            format!("outPtr, {call_args}")
        }
    }

    pub fn is_async(&self) -> bool {
        matches!(self.mode, TsClassMethodMode::Async(_))
    }
}

#[derive(Debug, Clone)]
pub struct TsHandleReturn {
    pub class_name: String,
    pub nullable: bool,
}

#[derive(Debug, Clone)]
pub enum TsClassMethodMode {
    Sync(TsClassSyncMethod),
    Async(TsClassAsyncMethod),
}

#[derive(Debug, Clone)]
pub struct TsClassSyncMethod {
    pub return_abi: TsReturnAbi,
    pub decode_expr: String,
}

#[derive(Debug, Clone)]
pub struct TsClassAsyncMethod {
    pub poll_sync_ffi_name: String,
    pub complete_ffi_name: String,
    pub panic_message_ffi_name: String,
    pub cancel_ffi_name: String,
    pub free_ffi_name: String,
    pub decode_expr: String,
}

#[derive(Debug, Clone)]
pub struct TsCallback {
    pub interface_name: String,
    pub trait_name_snake: String,
    pub create_handle_fn: String,
    pub methods: Vec<TsCallbackMethod>,
    pub async_methods: Vec<TsAsyncCallbackMethod>,
    pub closure_fn_type: Option<String>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TsCallbackMethod {
    pub ts_name: String,
    pub import_name: String,
    pub params: Vec<TsCallbackParam>,
    pub return_kind: TsCallbackReturnKind,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub enum TsCallbackReturnKind {
    Void,
    Primitive {
        ts_type: String,
    },
    WireEncoded {
        ts_type: String,
        encode_expr: String,
        size_expr: String,
    },
}

#[derive(Debug, Clone)]
pub struct TsCallbackParam {
    pub name: String,
    pub ts_type: String,
    pub kind: TsCallbackParamKind,
}

#[derive(Debug, Clone)]
pub enum TsCallbackParamKind {
    Primitive {
        import_ts_type: String,
        call_expr: String,
    },
    WireEncoded {
        decode_expr: String,
    },
}

#[derive(Debug, Clone)]
pub struct TsAsyncCallbackMethod {
    pub ts_name: String,
    pub start_import_name: String,
    pub complete_export_name: String,
    pub params: Vec<TsCallbackParam>,
    pub return_type: Option<String>,
    pub encode_expr: Option<String>,
    pub size_expr: Option<String>,
    pub direct_write_method: Option<String>,
    pub direct_write_value_expr: Option<String>,
    pub direct_size: Option<usize>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TsRecord {
    pub name: String,
    pub fields: Vec<TsField>,
    pub is_blittable: bool,
    pub wire_size: Option<usize>,
    pub tail_padding: usize,
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
            TsParamConversion::Bytes => Some(format!(
                "const {}_alloc = _module.allocBytes({});",
                self.name, self.name
            )),
            TsParamConversion::PrimitiveBuffer { element_abi } => Some(format!(
                "const {}_alloc = _module.allocPrimitiveBuffer({}, \"{}\");",
                self.name,
                self.name,
                primitive_buffer_runtime_tag(*element_abi)
            )),
            TsParamConversion::Callback { interface_name } => Some(format!(
                "const {}_handle = register{}({});",
                self.name, interface_name, self.name
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
            TsParamConversion::String | TsParamConversion::Bytes => {
                vec![
                    format!("{}_alloc.ptr", self.name),
                    format!("{}_alloc.len", self.name),
                ]
            }
            TsParamConversion::PrimitiveBuffer { .. } => {
                vec![
                    format!("{}_alloc.ptr", self.name),
                    format!("{}_alloc.len", self.name),
                ]
            }
            TsParamConversion::Callback { .. } => {
                vec![format!("{}_handle", self.name)]
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
            TsParamConversion::Direct | TsParamConversion::Callback { .. } => None,
            TsParamConversion::String | TsParamConversion::Bytes => {
                Some(format!("_module.freeAlloc({}_alloc);", self.name))
            }
            TsParamConversion::PrimitiveBuffer { .. } => {
                Some(format!("_module.freePrimitiveBuffer({}_alloc);", self.name))
            }
            TsParamConversion::CodecEncoded { .. } | TsParamConversion::OtherEncoded { .. } => {
                Some(format!("_module.freeWriter({}_writer);", self.name))
            }
        }
    }

    pub fn needs_cleanup(&self) -> bool {
        !matches!(self.conversion, TsParamConversion::Direct)
    }
}

fn flatten_ffi_args(params: &[TsParam]) -> Vec<String> {
    params.iter().flat_map(TsParam::ffi_args).collect()
}

#[derive(Debug, Clone)]
pub enum TsParamConversion {
    Direct,
    String,
    Bytes,
    PrimitiveBuffer { element_abi: AbiType },
    Callback { interface_name: String },
    CodecEncoded { codec_name: String },
    OtherEncoded { encode: WriteSeq },
}

fn primitive_buffer_runtime_tag(abi_type: AbiType) -> &'static str {
    match abi_type {
        AbiType::Bool => "bool",
        AbiType::I8 => "i8",
        AbiType::U8 => "u8",
        AbiType::I16 => "i16",
        AbiType::U16 => "u16",
        AbiType::I32 => "i32",
        AbiType::U32 => "u32",
        AbiType::I64 => "i64",
        AbiType::U64 => "u64",
        AbiType::ISize => "isize",
        AbiType::USize => "usize",
        AbiType::F32 => "f32",
        AbiType::F64 => "f64",
        AbiType::Void | AbiType::Pointer => {
            panic!("unsupported primitive buffer abi type: {abi_type:?}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_buffer_param_generates_expected_wrapper_and_cleanup() {
        let param = TsParam {
            name: "values".to_string(),
            ts_type: "number[]".to_string(),
            conversion: TsParamConversion::PrimitiveBuffer {
                element_abi: AbiType::I32,
            },
        };

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
        assert!(param.needs_cleanup());
    }

    #[test]
    fn primitive_buffer_param_uses_i64_runtime_tag_for_bigint_vectors() {
        let param = TsParam {
            name: "values".to_string(),
            ts_type: "bigint[]".to_string(),
            conversion: TsParamConversion::PrimitiveBuffer {
                element_abi: AbiType::I64,
            },
        };

        assert_eq!(
            param.wrapper_code(),
            Some("const values_alloc = _module.allocPrimitiveBuffer(values, \"i64\");".to_string())
        );
    }
}

#[derive(Debug, Clone)]
pub enum TsReturnAbi {
    Void,
    Direct { ts_cast: String },
    Packed,
    RawPacked { decode_expr: String },
}

impl TsReturnAbi {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct { .. })
    }

    pub fn is_packed(&self) -> bool {
        matches!(self, Self::Packed)
    }

    pub fn is_raw_packed(&self) -> bool {
        matches!(self, Self::RawPacked { .. })
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
