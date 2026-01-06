use super::{NamingConvention, TypeMapper};
use crate::model::{DataEnumLayout, Module, OptionAbi, Primitive, Type};

pub trait OptionAbiKotlin {
    fn optional_vec_list_suffix(&self) -> &'static str;
    fn jni_return_type(&self) -> &'static str;
    fn box_class(&self) -> &'static str;
    fn box_signature(&self) -> &'static str;
    fn box_jni_type(&self) -> &'static str;
}

impl OptionAbiKotlin for OptionAbi {
    fn optional_vec_list_suffix(&self) -> &'static str {
        match self {
            Self::VecPrimitive {
                primitive: Primitive::U8,
            } => "?.map { it.toUByte() }",
            Self::VecPrimitive {
                primitive: Primitive::U16,
            } => "?.map { it.toUShort() }",
            Self::VecPrimitive {
                primitive: Primitive::U32,
            } => "?.map { it.toUInt() }",
            Self::VecPrimitive {
                primitive: Primitive::U64,
            } => "?.map { it.toULong() }",
            Self::VecPrimitive { .. } => "?.toList()",
            _ => "",
        }
    }

    fn jni_return_type(&self) -> &'static str {
        match self {
            Self::Packed { .. } => "jlong",
            Self::Primitive { .. } => "jobject",
            Self::String => "jstring",
            Self::Record { .. }
            | Self::DataEnum { .. }
            | Self::VecRecord { .. }
            | Self::VecDataEnum { .. } => "jobject",
            Self::Enum => "jint",
            Self::VecPrimitive { primitive } => primitive.jni_array_type(),
            Self::VecString => "jobjectArray",
            Self::VecEnum => "jintArray",
        }
    }

    fn box_class(&self) -> &'static str {
        match self {
            Self::Primitive {
                primitive: Primitive::I64 | Primitive::U64 | Primitive::Isize | Primitive::Usize,
            } => "java/lang/Long",
            Self::Primitive {
                primitive: Primitive::F64,
            } => "java/lang/Double",
            _ => "",
        }
    }

    fn box_signature(&self) -> &'static str {
        match self {
            Self::Primitive {
                primitive: Primitive::I64 | Primitive::U64 | Primitive::Isize | Primitive::Usize,
            } => "(J)Ljava/lang/Long;",
            Self::Primitive {
                primitive: Primitive::F64,
            } => "(D)Ljava/lang/Double;",
            _ => "",
        }
    }

    fn box_jni_type(&self) -> &'static str {
        match self {
            Self::Primitive {
                primitive: Primitive::I64 | Primitive::U64 | Primitive::Isize | Primitive::Usize,
            } => "jlong",
            Self::Primitive {
                primitive: Primitive::F64,
            } => "jdouble",
            _ => "",
        }
    }
}

#[derive(Debug, Clone)]
pub struct OptionView {
    pub strategy: OptionAbi,
    pub c_out_type: String,
    pub kotlin_native_type: String,
    pub reader_name: Option<String>,
    pub codec_name: Option<String>,
}

impl OptionView {
    pub fn from_inner(inner: &Type, module: &Module) -> Self {
        let strategy = OptionAbi::from_type(
            inner,
            |name| module.struct_size(name),
            |name| module.is_data_enum(name),
        );
        let c_out_type = Self::resolve_c_out_type(inner);
        let kotlin_native_type = Self::resolve_kotlin_native_type(&strategy, inner);
        let reader_name = Self::resolve_reader_name(inner, &strategy);
        let codec_name = Self::resolve_codec_name(inner, &strategy);

        Self {
            strategy,
            c_out_type,
            kotlin_native_type,
            reader_name,
            codec_name,
        }
    }

    fn resolve_c_out_type(inner: &Type) -> String {
        match inner {
            Type::Primitive(p) => p.c_type_name().to_string(),
            Type::String => "FfiString".to_string(),
            Type::Record(name) | Type::Enum(name) => NamingConvention::class_name(name),
            Type::Vec(_) => "void".to_string(),
            _ => "void".to_string(),
        }
    }

    fn resolve_kotlin_native_type(abi: &OptionAbi, inner: &Type) -> String {
        match abi {
            OptionAbi::Packed { .. } => "Long".to_string(),
            OptionAbi::Primitive { .. } => format!("{}?", TypeMapper::map_type(inner)),
            OptionAbi::String => "String?".to_string(),
            OptionAbi::Record { .. } | OptionAbi::DataEnum { .. } => "ByteBuffer?".to_string(),
            OptionAbi::Enum => "Int".to_string(),
            OptionAbi::VecPrimitive { .. } => format!("{}?", TypeMapper::jni_type(inner)),
            OptionAbi::VecRecord { .. } | OptionAbi::VecDataEnum { .. } => "ByteBuffer?".to_string(),
            OptionAbi::VecString => "Array<String>?".to_string(),
            OptionAbi::VecEnum => "IntArray?".to_string(),
        }
    }

    fn resolve_reader_name(inner: &Type, strategy: &OptionAbi) -> Option<String> {
        match inner {
            Type::Record(name) => Some(format!("{}Reader", NamingConvention::class_name(name))),
            Type::Enum(name) if strategy.is_enum() => Some(NamingConvention::class_name(name)),
            Type::Vec(vec_inner) => match vec_inner.as_ref() {
                Type::Record(name) => Some(format!("{}Reader", NamingConvention::class_name(name))),
                Type::Enum(name) if strategy.is_vec_enum() => {
                    Some(NamingConvention::class_name(name))
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn resolve_codec_name(inner: &Type, strategy: &OptionAbi) -> Option<String> {
        match inner {
            Type::Enum(name) if strategy.is_data_enum() => {
                Some(format!("{}Codec", NamingConvention::class_name(name)))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ResultOkKind {
    Void,
    Primitive { c_type: String, jni_type: String },
    String,
    Record { name: String, struct_size: usize },
    Enum { name: String },
    DataEnum { name: String, struct_size: usize },
    VecPrimitive { primitive: Primitive, len_fn: String, copy_fn: String },
    VecRecord { name: String, struct_size: usize, len_fn: String, copy_fn: String },
    Option(Box<OptionAbi>),
}

#[derive(Debug, Clone)]
pub enum ResultErrKind {
    String,
    Enum { name: String },
    DataEnum { name: String, struct_size: usize },
}

#[derive(Debug, Clone)]
pub struct ResultView {
    pub ok_type: String,
    pub ok_kind: ResultOkKind,
    pub err_type: String,
    pub err_kind: ResultErrKind,
}

impl ResultView {
    pub fn from_result(ok: &Type, err: &Type, module: &Module, func_name: &str) -> Self {
        let ok_type = TypeMapper::map_type(ok);
        let ok_kind = Self::resolve_ok_kind(ok, module, func_name);
        let err_type = TypeMapper::map_type(err);
        let err_kind = Self::resolve_err_kind(err, module);
        Self { ok_type, ok_kind, err_type, err_kind }
    }

    fn resolve_err_kind(err: &Type, module: &Module) -> ResultErrKind {
        match err {
            Type::Enum(name) => {
                let enum_def = module.enums.iter().find(|e| &e.name == name);
                let is_data_or_error = enum_def
                    .map(|e| e.is_data_enum() || e.is_error)
                    .unwrap_or(false);
                if is_data_or_error {
                    let struct_size = enum_def
                        .and_then(|e| DataEnumLayout::from_enum(e))
                        .map(|l| l.struct_size().as_usize())
                        .unwrap_or(4);
                    ResultErrKind::DataEnum {
                        name: NamingConvention::class_name(name),
                        struct_size,
                    }
                } else {
                    ResultErrKind::Enum {
                        name: NamingConvention::class_name(name),
                    }
                }
            }
            _ => ResultErrKind::String,
        }
    }

    fn resolve_ok_kind(ok: &Type, module: &Module, func_name: &str) -> ResultOkKind {
        match ok {
            Type::Void => ResultOkKind::Void,
            Type::Primitive(p) => ResultOkKind::Primitive {
                c_type: p.c_type_name().to_string(),
                jni_type: TypeMapper::c_jni_type(ok),
            },
            Type::String => ResultOkKind::String,
            Type::Record(name) => {
                let struct_size = module
                    .records
                    .iter()
                    .find(|r| &r.name == name)
                    .map(|r| r.struct_size().as_usize())
                    .unwrap_or(0);
                ResultOkKind::Record {
                    name: NamingConvention::class_name(name),
                    struct_size,
                }
            }
            Type::Enum(name) => {
                let is_data_enum = module
                    .enums
                    .iter()
                    .find(|e| &e.name == name)
                    .map(|e| e.is_data_enum())
                    .unwrap_or(false);
                if is_data_enum {
                    let struct_size = module
                        .enums
                        .iter()
                        .find(|e| &e.name == name)
                        .and_then(|e| DataEnumLayout::from_enum(e))
                        .map(|l| l.struct_size().as_usize())
                        .unwrap_or(0);
                    ResultOkKind::DataEnum {
                        name: NamingConvention::class_name(name),
                        struct_size,
                    }
                } else {
                    ResultOkKind::Enum {
                        name: NamingConvention::class_name(name),
                    }
                }
            }
            Type::Vec(inner) => match inner.as_ref() {
                Type::Primitive(p) => ResultOkKind::VecPrimitive {
                    primitive: *p,
                    len_fn: riff_ffi_rules::naming::function_ffi_vec_len(func_name),
                    copy_fn: riff_ffi_rules::naming::function_ffi_vec_copy_into(func_name),
                },
                Type::Record(name) => {
                    let struct_size = module
                        .records
                        .iter()
                        .find(|r| &r.name == name)
                        .map(|r| r.struct_size().as_usize())
                        .unwrap_or(0);
                    ResultOkKind::VecRecord {
                        name: NamingConvention::class_name(name),
                        struct_size,
                        len_fn: riff_ffi_rules::naming::function_ffi_vec_len(func_name),
                        copy_fn: riff_ffi_rules::naming::function_ffi_vec_copy_into(func_name),
                    }
                }
                _ => ResultOkKind::Void,
            },
            Type::Option(inner) => {
                let abi = OptionAbi::from_type(
                    inner,
                    |name| module.struct_size(name),
                    |name| module.is_data_enum(name),
                );
                ResultOkKind::Option(Box::new(abi))
            }
            _ => ResultOkKind::Void,
        }
    }

    pub fn is_void(&self) -> bool {
        matches!(self.ok_kind, ResultOkKind::Void)
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self.ok_kind, ResultOkKind::Primitive { .. })
    }

    pub fn is_string(&self) -> bool {
        matches!(self.ok_kind, ResultOkKind::String)
    }

    pub fn is_record(&self) -> bool {
        matches!(self.ok_kind, ResultOkKind::Record { .. })
    }

    pub fn is_enum(&self) -> bool {
        matches!(self.ok_kind, ResultOkKind::Enum { .. })
    }

    pub fn is_data_enum(&self) -> bool {
        matches!(self.ok_kind, ResultOkKind::DataEnum { .. })
    }

    pub fn is_vec_primitive(&self) -> bool {
        matches!(self.ok_kind, ResultOkKind::VecPrimitive { .. })
    }

    pub fn is_vec_record(&self) -> bool {
        matches!(self.ok_kind, ResultOkKind::VecRecord { .. })
    }

    pub fn is_option(&self) -> bool {
        matches!(self.ok_kind, ResultOkKind::Option(_))
    }

    pub fn primitive_c_type(&self) -> &str {
        match &self.ok_kind {
            ResultOkKind::Primitive { c_type, .. } => c_type,
            _ => "",
        }
    }

    pub fn primitive_jni_type(&self) -> &str {
        match &self.ok_kind {
            ResultOkKind::Primitive { jni_type, .. } => jni_type,
            _ => "",
        }
    }

    pub fn record_name(&self) -> &str {
        match &self.ok_kind {
            ResultOkKind::Record { name, .. } => name,
            _ => "",
        }
    }

    pub fn record_struct_size(&self) -> usize {
        match &self.ok_kind {
            ResultOkKind::Record { struct_size, .. } => *struct_size,
            _ => 0,
        }
    }

    pub fn enum_name(&self) -> &str {
        match &self.ok_kind {
            ResultOkKind::Enum { name } | ResultOkKind::DataEnum { name, .. } => name,
            _ => "",
        }
    }

    pub fn data_enum_struct_size(&self) -> usize {
        match &self.ok_kind {
            ResultOkKind::DataEnum { struct_size, .. } => *struct_size,
            _ => 0,
        }
    }

    pub fn vec_primitive(&self) -> Option<Primitive> {
        match &self.ok_kind {
            ResultOkKind::VecPrimitive { primitive, .. } => Some(*primitive),
            _ => None,
        }
    }

    pub fn vec_len_fn(&self) -> &str {
        match &self.ok_kind {
            ResultOkKind::VecPrimitive { len_fn, .. } | ResultOkKind::VecRecord { len_fn, .. } => {
                len_fn
            }
            _ => "",
        }
    }

    pub fn vec_copy_fn(&self) -> &str {
        match &self.ok_kind {
            ResultOkKind::VecPrimitive { copy_fn, .. }
            | ResultOkKind::VecRecord { copy_fn, .. } => copy_fn,
            _ => "",
        }
    }

    pub fn vec_record_name(&self) -> &str {
        match &self.ok_kind {
            ResultOkKind::VecRecord { name, .. } => name,
            _ => "",
        }
    }

    pub fn vec_record_struct_size(&self) -> usize {
        match &self.ok_kind {
            ResultOkKind::VecRecord { struct_size, .. } => *struct_size,
            _ => 0,
        }
    }

    pub fn option_abi(&self) -> Option<&OptionAbi> {
        match &self.ok_kind {
            ResultOkKind::Option(abi) => Some(abi),
            _ => None,
        }
    }

    pub fn jni_return_type(&self) -> &str {
        match &self.ok_kind {
            ResultOkKind::Void => "void",
            ResultOkKind::Primitive { jni_type, .. } => jni_type,
            ResultOkKind::String => "jstring",
            ResultOkKind::Record { .. } => "jobject",
            ResultOkKind::Enum { .. } => "jint",
            ResultOkKind::DataEnum { .. } => "jobject",
            ResultOkKind::VecPrimitive { primitive, .. } => primitive.jni_array_type(),
            ResultOkKind::VecRecord { .. } => "jobject",
            ResultOkKind::Option(s) => s.jni_return_type(),
        }
    }

    pub fn has_structured_error(&self) -> bool {
        matches!(self.err_kind, ResultErrKind::DataEnum { .. })
    }

    pub fn err_is_data_enum(&self) -> bool {
        matches!(self.err_kind, ResultErrKind::DataEnum { .. })
    }

    pub fn err_is_ffi_error(&self) -> bool {
        matches!(self.err_kind, ResultErrKind::String)
    }

    pub fn err_enum_name(&self) -> &str {
        match &self.err_kind {
            ResultErrKind::Enum { name } | ResultErrKind::DataEnum { name, .. } => name,
            _ => "",
        }
    }

    pub fn err_struct_size(&self) -> usize {
        match &self.err_kind {
            ResultErrKind::DataEnum { struct_size, .. } => *struct_size,
            ResultErrKind::String => 24,
            _ => 0,
        }
    }

    pub fn err_exception_name(&self) -> String {
        match &self.err_kind {
            ResultErrKind::Enum { name } | ResultErrKind::DataEnum { name, .. } => name.clone(),
            _ => "FfiException".to_string(),
        }
    }

    pub fn err_codec_name(&self) -> String {
        match &self.err_kind {
            ResultErrKind::Enum { name } | ResultErrKind::DataEnum { name, .. } => {
                format!("{}Codec", name)
            }
            _ => String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ReturnKind {
    Void,
    Primitive,
    String,
    Vec {
        inner: String,
        len_fn: String,
        copy_fn: String,
        primitive: Option<Primitive>,
    },
    VecRecord {
        inner: String,
        reader: String,
    },
    Option {
        inner: String,
    },
    Result {
        ok: String,
    },
    Enum {
        name: String,
    },
    Record {
        name: String,
    },
}

impl ReturnKind {
    pub fn from_type(ty: &Type, ffi_base: &str) -> Self {
        match ty {
            Type::Void => Self::Void,
            Type::Primitive(_) => Self::Primitive,
            Type::String => Self::String,
            Type::Vec(inner) => match inner.as_ref() {
                Type::Record(name) => Self::VecRecord {
                    inner: NamingConvention::class_name(name),
                    reader: format!("{}Reader", NamingConvention::class_name(name)),
                },
                _ => Self::Vec {
                    inner: TypeMapper::map_type(inner),
                    len_fn: format!("{}_len", ffi_base),
                    copy_fn: format!("{}_copy_into", ffi_base),
                    primitive: match inner.as_ref() {
                        Type::Primitive(p) => Some(*p),
                        _ => None,
                    },
                },
            },
            Type::Option(inner) => Self::Option {
                inner: TypeMapper::map_type(inner),
            },
            Type::Result { ok, .. } => Self::Result {
                ok: TypeMapper::map_type(ok),
            },
            Type::Enum(name) => Self::Enum {
                name: NamingConvention::class_name(name),
            },
            Type::Record(name) => Self::Record {
                name: NamingConvention::class_name(name),
            },
            Type::Bytes => panic!("Bytes return type not yet supported in Kotlin bindings"),
            Type::Slice(_) => panic!("Slice return type not yet supported in Kotlin bindings"),
            Type::MutSlice(_) => {
                panic!("MutSlice return type not yet supported in Kotlin bindings")
            }
            Type::Object(name) => panic!(
                "Object return type '{}' not yet supported in Kotlin bindings",
                name
            ),
            Type::BoxedTrait(name) => panic!(
                "BoxedTrait return type '{}' not yet supported in Kotlin bindings",
                name
            ),
            Type::Callback(_) => {
                panic!("Callback return type not yet supported in Kotlin bindings")
            }
        }
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self, Self::Primitive)
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String)
    }

    pub fn is_vec(&self) -> bool {
        matches!(self, Self::Vec { .. })
    }

    pub fn is_vec_record(&self) -> bool {
        matches!(self, Self::VecRecord { .. })
    }

    pub fn reader_name(&self) -> Option<&str> {
        match self {
            Self::VecRecord { reader, .. } => Some(reader),
            _ => None,
        }
    }

    pub fn is_option(&self) -> bool {
        matches!(self, Self::Option { .. })
    }

    pub fn is_result(&self) -> bool {
        matches!(self, Self::Result { .. })
    }

    pub fn is_unit(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_enum(&self) -> bool {
        matches!(self, Self::Enum { .. })
    }

    pub fn inner_type(&self) -> Option<&str> {
        match self {
            Self::Vec { inner, .. } => Some(inner),
            Self::VecRecord { inner, .. } => Some(inner),
            Self::Option { inner } => Some(inner),
            Self::Result { ok } => Some(ok),
            _ => None,
        }
    }

    pub fn len_fn(&self) -> Option<&str> {
        match self {
            Self::Vec { len_fn, .. } => Some(len_fn),
            _ => None,
        }
    }

    pub fn copy_fn(&self) -> Option<&str> {
        match self {
            Self::Vec { copy_fn, .. } => Some(copy_fn),
            _ => None,
        }
    }

    pub fn vec_list_suffix(&self) -> &str {
        match self {
            Self::Vec {
                primitive: Some(Primitive::U8),
                ..
            } => ".map { it.toUByte() }",
            Self::Vec {
                primitive: Some(Primitive::U16),
                ..
            } => ".map { it.toUShort() }",
            Self::Vec {
                primitive: Some(Primitive::U32),
                ..
            } => ".map { it.toUInt() }",
            Self::Vec {
                primitive: Some(Primitive::U64),
                ..
            } => ".map { it.toULong() }",
            Self::Vec { .. } => ".toList()",
            _ => "",
        }
    }
}

pub struct ParamConversion;

impl ParamConversion {
    pub fn to_ffi(param_name: &str, ty: &Type) -> String {
        match ty {
            Type::String => param_name.to_string(),
            Type::Bytes => param_name.to_string(),
            Type::Primitive(primitive) => match primitive {
                Primitive::U8 => format!("{}.toByte()", param_name),
                Primitive::U16 => format!("{}.toShort()", param_name),
                Primitive::U32 => format!("{}.toInt()", param_name),
                Primitive::U64 => format!("{}.toLong()", param_name),
                _ => param_name.to_string(),
            },
            Type::Record(_) => param_name.to_string(),
            Type::Enum(_) => format!("{}.value", param_name),
            Type::Object(_) => format!("{}.handle", param_name),
            Type::Vec(inner) | Type::Slice(inner) => match inner.as_ref() {
                Type::Record(name) => {
                    format!(
                        "{}Writer.pack({})",
                        NamingConvention::class_name(name),
                        param_name
                    )
                }
                Type::Primitive(Primitive::I8) => format!("{}.toByteArray()", param_name),
                Type::Primitive(Primitive::U8) => {
                    format!("{}.map {{ it.toByte() }}.toByteArray()", param_name)
                }
                Type::Primitive(Primitive::I16) => format!("{}.toShortArray()", param_name),
                Type::Primitive(Primitive::U16) => {
                    format!("{}.map {{ it.toShort() }}.toShortArray()", param_name)
                }
                Type::Primitive(Primitive::I32) => format!("{}.toIntArray()", param_name),
                Type::Primitive(Primitive::U32) => {
                    format!("{}.map {{ it.toInt() }}.toIntArray()", param_name)
                }
                Type::Primitive(Primitive::I64) => format!("{}.toLongArray()", param_name),
                Type::Primitive(Primitive::U64) => {
                    format!("{}.map {{ it.toLong() }}.toLongArray()", param_name)
                }
                Type::Primitive(Primitive::F32) => format!("{}.toFloatArray()", param_name),
                Type::Primitive(Primitive::F64) => format!("{}.toDoubleArray()", param_name),
                Type::Primitive(Primitive::Bool) => format!("{}.toBooleanArray()", param_name),
                _ => param_name.to_string(),
            },
            _ => param_name.to_string(),
        }
    }
}

// JNI-specific types for C glue generation

#[derive(Debug, Clone)]
pub enum JniReturnKind {
    Void,
    Primitive { jni_type: String },
    String { ffi_name: String },
    Vec { len_fn: String, copy_fn: String },
    CStyleEnum,
    DataEnum { enum_name: String, struct_size: usize },
    Option(OptionView),
    Result(ResultView),
}

impl JniReturnKind {
    pub fn from_type(ty: Option<&Type>, func_name: &str) -> Self {
        match ty {
            None | Some(Type::Void) => Self::Void,
            Some(Type::Primitive(primitive)) => Self::Primitive {
                jni_type: TypeMapper::c_jni_type(&Type::Primitive(*primitive)),
            },
            Some(Type::String) => Self::String {
                ffi_name: riff_ffi_rules::naming::function_ffi_name(func_name),
            },
            Some(Type::Vec(_)) => Self::Vec {
                len_fn: riff_ffi_rules::naming::function_ffi_vec_len(func_name),
                copy_fn: riff_ffi_rules::naming::function_ffi_vec_copy_into(func_name),
            },
            Some(Type::Enum(_)) => Self::CStyleEnum,
            _ => Self::Void,
        }
    }

    pub fn from_type_with_module(
        ty: Option<&Type>,
        func_name: &str,
        module: &Module,
    ) -> Self {
        match ty {
            Some(Type::Option(inner)) => Self::Option(OptionView::from_inner(inner, module)),
            Some(Type::Result { ok, err }) => {
                Self::Result(ResultView::from_result(ok, err, module, func_name))
            }
            Some(Type::Enum(enum_name)) => {
                module
                    .enums
                    .iter()
                    .find(|e| &e.name == enum_name)
                    .filter(|e| e.is_data_enum())
                    .and_then(|e| DataEnumLayout::from_enum(e))
                    .map(|layout| Self::DataEnum {
                        enum_name: NamingConvention::class_name(enum_name),
                        struct_size: layout.struct_size().as_usize(),
                    })
                    .unwrap_or(Self::CStyleEnum)
            }
            _ => Self::from_type(ty, func_name),
        }
    }

    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String { .. })
    }

    pub fn is_vec(&self) -> bool {
        matches!(self, Self::Vec { .. })
    }

    pub fn is_c_style_enum(&self) -> bool {
        matches!(self, Self::CStyleEnum)
    }

    pub fn is_data_enum(&self) -> bool {
        matches!(self, Self::DataEnum { .. })
    }

    pub fn is_option(&self) -> bool {
        matches!(self, Self::Option(_))
    }

    pub fn option_view(&self) -> Option<&OptionView> {
        match self {
            Self::Option(view) => Some(view),
            _ => None,
        }
    }

    pub fn jni_return_type(&self) -> &str {
        match self {
            Self::Void => "void",
            Self::Primitive { jni_type } => jni_type,
            Self::String { .. } => "jstring",
            Self::Vec { .. } => "jlong",
            Self::CStyleEnum => "jint",
            Self::DataEnum { .. } => "jobject",
            Self::Option(view) => view.strategy.jni_return_type(),
            Self::Result(view) => view.jni_return_type(),
        }
    }

    pub fn is_result(&self) -> bool {
        matches!(self, Self::Result(_))
    }

    pub fn result_view(&self) -> Option<&ResultView> {
        match self {
            Self::Result(view) => Some(view),
            _ => None,
        }
    }

    pub fn data_enum_struct_size(&self) -> usize {
        match self {
            Self::DataEnum { struct_size, .. } => *struct_size,
            _ => 0,
        }
    }

    pub fn data_enum_name(&self) -> Option<&str> {
        match self {
            Self::DataEnum { enum_name, .. } => Some(enum_name),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ArrayInfo {
    primitive: Option<Primitive>,
    is_mutable: bool,
}

#[derive(Debug, Clone, Default)]
struct RecordInfo {
    name: Option<String>,
    struct_size: usize,
    is_mutable: bool,
}

#[derive(Debug, Clone, Default)]
struct DataEnumInfo {
    name: Option<String>,
    struct_size: usize,
}

#[derive(Debug, Clone)]
pub struct JniParamInfo {
    pub name: String,
    pub jni_type: String,
    pub is_string: bool,
    pub is_handle: bool,
    pub array_primitive: Option<Primitive>,
    pub array_is_mutable: bool,
    pub record_name: Option<String>,
    pub record_struct_size: usize,
    pub record_is_mutable: bool,
    pub data_enum_name: Option<String>,
    pub data_enum_struct_size: usize,
}

impl JniParamInfo {
    /// Simple constructor for class methods - doesn't detect data enums
    pub fn from_param(name: &str, ty: &Type) -> Self {
        let array_info = Self::extract_array_info(ty);

        let (record_name, record_is_mutable) = match ty {
            Type::Vec(inner) | Type::Slice(inner) | Type::MutSlice(inner) => match inner.as_ref() {
                Type::Record(name) => (Some(name.clone()), matches!(ty, Type::MutSlice(_))),
                _ => (None, false),
            },
            _ => (None, false),
        };

        Self {
            name: name.to_string(),
            jni_type: TypeMapper::c_jni_type(ty),
            is_string: matches!(ty, Type::String),
            is_handle: matches!(ty, Type::Object(_) | Type::BoxedTrait(_)),
            array_primitive: array_info.primitive,
            array_is_mutable: array_info.is_mutable,
            record_name,
            record_struct_size: 0,
            record_is_mutable,
            data_enum_name: None,
            data_enum_struct_size: 0,
        }
    }

    pub fn from_param_with_module(
        name: &str,
        ty: &Type,
        module: &Module,
    ) -> Self {
        let array_info = Self::extract_array_info(ty);
        let record_info = Self::extract_record_info(ty, module);
        let enum_info = Self::extract_enum_info(ty, module);

        let jni_type = Self::compute_jni_type(ty, &enum_info);

        Self {
            name: name.to_string(),
            jni_type,
            is_string: matches!(ty, Type::String),
            is_handle: matches!(ty, Type::Object(_) | Type::BoxedTrait(_)),
            array_primitive: array_info.primitive,
            array_is_mutable: array_info.is_mutable,
            record_name: record_info.name,
            record_struct_size: record_info.struct_size,
            record_is_mutable: record_info.is_mutable,
            data_enum_name: enum_info.name,
            data_enum_struct_size: enum_info.struct_size,
        }
    }

    fn extract_array_info(ty: &Type) -> ArrayInfo {
        match ty {
            Type::Vec(inner) | Type::Slice(inner) | Type::MutSlice(inner) => match inner.as_ref() {
                Type::Primitive(primitive) => ArrayInfo {
                    primitive: Some(*primitive),
                    is_mutable: matches!(ty, Type::MutSlice(_)),
                },
                _ => ArrayInfo::default(),
            },
            _ => ArrayInfo::default(),
        }
    }

    fn extract_record_info(ty: &Type, module: &Module) -> RecordInfo {
        match ty {
            Type::Vec(inner) | Type::Slice(inner) | Type::MutSlice(inner) => match inner.as_ref() {
                Type::Record(record_name) => {
                    let struct_size = module
                        .records
                        .iter()
                        .find(|r| &r.name == record_name)
                        .map(|r| r.struct_size().as_usize())
                        .unwrap_or(0);

                    RecordInfo {
                        name: Some(record_name.clone()),
                        struct_size,
                        is_mutable: matches!(ty, Type::MutSlice(_)),
                    }
                }
                _ => RecordInfo::default(),
            },
            _ => RecordInfo::default(),
        }
    }

    fn extract_enum_info(ty: &Type, module: &Module) -> DataEnumInfo {
        let Type::Enum(enum_name) = ty else {
            return DataEnumInfo::default();
        };

        let Some(enumeration) = module.enums.iter().find(|e| &e.name == enum_name) else {
            return DataEnumInfo::default();
        };

        if !enumeration.is_data_enum() {
            return DataEnumInfo::default();
        }

        let struct_size = DataEnumLayout::from_enum(enumeration)
            .map(|layout| layout.struct_size().as_usize())
            .unwrap_or(0);

        DataEnumInfo {
            name: Some(enum_name.clone()),
            struct_size,
        }
    }

    fn compute_jni_type(ty: &Type, enum_info: &DataEnumInfo) -> String {
        if enum_info.name.is_some() {
            return "jobject".to_string();
        }
        TypeMapper::c_jni_type(ty)
    }

    pub fn jni_param_decl(&self) -> String {
        format!("{} {}", self.jni_type, self.name)
    }

    pub fn ffi_arg(&self) -> String {
        if self.is_string {
            format!(
                "(const uint8_t*)_{}_c, {} ? strlen(_{}_c) : 0",
                self.name, self.name, self.name
            )
        } else if let Some(enum_name) = &self.data_enum_name {
            let c_name = NamingConvention::class_name(enum_name);
            format!("*({}*)_{}_ptr", c_name, self.name)
        } else if let Some(record_name) = &self.record_name {
            let c_name = NamingConvention::class_name(record_name);
            let ptr_type = if self.record_is_mutable {
                format!("{}*", c_name)
            } else {
                format!("const {}*", c_name)
            };

            format!(
                "({})_{}_ptr, (uintptr_t)_{}_len",
                ptr_type, self.name, self.name
            )
        } else if let Some(primitive) = self.array_primitive {
            let c_type = primitive.c_type_name();
            let ptr_type = if self.array_is_mutable {
                format!("{}*", c_type)
            } else {
                format!("const {}*", c_type)
            };

            format!(
                "({})_{}_ptr, (uintptr_t)_{}_len",
                ptr_type, self.name, self.name
            )
        } else if self.is_handle {
            format!("(void*){}", self.name)
        } else {
            self.name.clone()
        }
    }

    pub fn is_primitive_array(&self) -> bool {
        self.array_primitive.is_some()
    }

    pub fn is_record_buffer(&self) -> bool {
        self.record_name.is_some()
    }

    pub fn is_data_enum(&self) -> bool {
        self.data_enum_name.is_some()
    }

    pub fn data_enum_c_type(&self) -> String {
        self.data_enum_name
            .as_ref()
            .map(|name| NamingConvention::class_name(name))
            .unwrap_or_default()
    }

    pub fn array_c_type(&self) -> &'static str {
        self.array_primitive
            .expect("array_c_type called on non-array param")
            .c_type_name()
    }

    pub fn array_release_mode(&self) -> &'static str {
        if self.array_is_mutable {
            "0"
        } else {
            "JNI_ABORT"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Primitive;

    #[test]
    fn test_return_kind_primitives() {
        assert!(ReturnKind::from_type(&Type::Primitive(Primitive::I32), "test").is_primitive());
        assert!(ReturnKind::from_type(&Type::Primitive(Primitive::Bool), "test").is_primitive());
    }

    #[test]
    fn test_return_kind_string() {
        assert!(ReturnKind::from_type(&Type::String, "test").is_string());
    }

    #[test]
    fn test_return_kind_vec() {
        let vec_type = Type::Vec(Box::new(Type::Primitive(Primitive::I32)));
        let kind = ReturnKind::from_type(&vec_type, "test_fn");
        assert!(kind.is_vec());
        assert_eq!(kind.len_fn(), Some("test_fn_len"));
        assert_eq!(kind.copy_fn(), Some("test_fn_copy_into"));
        assert_eq!(kind.inner_type(), Some("Int"));
    }

    #[test]
    fn test_return_kind_void() {
        assert!(ReturnKind::from_type(&Type::Void, "test").is_unit());
    }

    #[test]
    fn test_param_conversion_string() {
        assert_eq!(
            ParamConversion::to_ffi("name", &Type::String),
            "name"
        );
    }

    #[test]
    fn test_param_conversion_enum() {
        assert_eq!(
            ParamConversion::to_ffi("status", &Type::Enum("Status".into())),
            "status.value"
        );
    }

    #[test]
    fn test_param_conversion_object() {
        assert_eq!(
            ParamConversion::to_ffi("sensor", &Type::Object("Sensor".into())),
            "sensor.handle"
        );
    }

    #[test]
    fn test_param_conversion_primitive() {
        assert_eq!(
            ParamConversion::to_ffi("count", &Type::Primitive(Primitive::I32)),
            "count"
        );
    }

    #[test]
    fn test_jni_param_data_enum() {
        use crate::model::{Enumeration, RecordField, Variant};

        let mut data_enum = Enumeration::new("Result");
        data_enum.variants.push(
            Variant::new("Ok").with_field(RecordField::new("value", Type::Primitive(Primitive::I32)))
        );
        data_enum.variants.push(
            Variant::new("Err").with_field(RecordField::new("code", Type::Primitive(Primitive::I32)))
        );

        let mut module = Module::new("test");
        module.enums.push(data_enum);

        let param = JniParamInfo::from_param_with_module("result", &Type::Enum("Result".into()), &module);

        assert!(param.data_enum_name.is_some());
        assert_eq!(param.data_enum_name.as_deref(), Some("Result"));
        assert!(param.data_enum_struct_size > 0);
        assert_eq!(param.jni_type, "jobject");
    }

    #[test]
    fn test_jni_param_c_style_enum() {
        use crate::model::{Enumeration, Variant};

        let mut c_style_enum = Enumeration::new("Status");
        c_style_enum.variants.push(Variant::new("Ok"));
        c_style_enum.variants.push(Variant::new("Error"));

        let mut module = Module::new("test");
        module.enums.push(c_style_enum);

        let param = JniParamInfo::from_param_with_module("status", &Type::Enum("Status".into()), &module);

        assert!(param.data_enum_name.is_none());
        assert_eq!(param.data_enum_struct_size, 0);
        assert_eq!(param.jni_type, "jint");
    }

    #[test]
    fn test_jni_return_kind_data_enum() {
        use crate::model::{Enumeration, RecordField, Variant};

        let mut data_enum = Enumeration::new("Response");
        data_enum.variants.push(
            Variant::new("Success").with_field(RecordField::new("data", Type::Primitive(Primitive::I64)))
        );

        let mut module = Module::new("test");
        module.enums.push(data_enum);

        let return_kind = JniReturnKind::from_type_with_module(
            Some(&Type::Enum("Response".into())),
            "get_response",
            &module,
        );

        assert!(return_kind.is_data_enum());
        assert_eq!(return_kind.data_enum_name(), Some("Response"));
        assert!(return_kind.data_enum_struct_size() > 0);
        assert_eq!(return_kind.jni_return_type(), "jobject");
    }

    #[test]
    fn test_jni_return_kind_c_style_enum() {
        use crate::model::{Enumeration, Variant};

        let mut c_style_enum = Enumeration::new("Status");
        c_style_enum.variants.push(Variant::new("Active"));

        let mut module = Module::new("test");
        module.enums.push(c_style_enum);

        let return_kind = JniReturnKind::from_type_with_module(
            Some(&Type::Enum("Status".into())),
            "get_status",
            &module,
        );

        assert!(return_kind.is_c_style_enum());
        assert!(!return_kind.is_data_enum());
        assert_eq!(return_kind.jni_return_type(), "jint");
    }
}
