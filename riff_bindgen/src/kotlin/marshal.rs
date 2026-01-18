use super::primitives;
use super::{NamingConvention, TypeMapper};
use crate::model::{
    ClosureSignature, DataEnumLayout, Module, OptionInfo, Primitive, ReturnType, Type,
};

#[derive(Debug, Clone)]
pub struct OptionView {
    pub info: OptionInfo,
    pub is_data_enum: bool,
    pub struct_size: usize,
    pub c_out_type: String,
    pub kotlin_native_type: String,
    pub reader_name: Option<String>,
    pub codec_name: Option<String>,
}

impl OptionView {
    pub fn from_inner(inner: &Type, module: &Module) -> Self {
        let info = OptionInfo::from_type(inner);
        let is_data_enum = info.is_data_enum(module);
        let struct_size = info.struct_size(module);

        let c_out_type = Self::resolve_c_out_type(inner);
        let kotlin_native_type = Self::resolve_kotlin_native_type(inner, &info, is_data_enum);
        let reader_name = Self::resolve_reader_name(inner, &info, is_data_enum);
        let codec_name = Self::resolve_codec_name(inner, is_data_enum);

        Self {
            info,
            is_data_enum,
            struct_size,
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

    fn resolve_kotlin_native_type(inner: &Type, info: &OptionInfo, is_data_enum: bool) -> String {
        if info.is_vec {
            let vec_inner = inner.vec_inner().unwrap();
            match vec_inner {
                Type::Primitive(p) if p.is_unsigned() => {
                    format!("{}?", TypeMapper::jni_type(inner))
                }
                Type::Primitive(_) => format!("{}?", TypeMapper::jni_type(inner)),
                Type::String => "Array<String>?".to_string(),
                Type::Record(_) => "ByteBuffer?".to_string(),
                Type::Enum(_) if is_data_enum => "ByteBuffer?".to_string(),
                Type::Enum(_) => "IntArray?".to_string(),
                _ => "Any?".to_string(),
            }
        } else {
            match inner {
                Type::Primitive(p) if p.fits_in_32_bits() => "Long".to_string(),
                Type::Primitive(_) => format!("{}?", TypeMapper::map_type(inner)),
                Type::String => "String?".to_string(),
                Type::Record(_) => "ByteBuffer?".to_string(),
                Type::Enum(_) if is_data_enum => "ByteBuffer?".to_string(),
                Type::Enum(_) => "Int".to_string(),
                _ => "Any?".to_string(),
            }
        }
    }

    fn resolve_reader_name(inner: &Type, _info: &OptionInfo, is_data_enum: bool) -> Option<String> {
        match inner {
            Type::Record(name) => Some(format!("{}Reader", NamingConvention::class_name(name))),
            Type::Enum(name) if !is_data_enum => Some(NamingConvention::class_name(name)),
            Type::Vec(vec_inner) => match vec_inner.as_ref() {
                Type::Record(name) => Some(format!("{}Reader", NamingConvention::class_name(name))),
                Type::Enum(name) if !is_data_enum => Some(NamingConvention::class_name(name)),
                _ => None,
            },
            _ => None,
        }
    }

    fn resolve_codec_name(inner: &Type, is_data_enum: bool) -> Option<String> {
        match inner {
            Type::Enum(name) if is_data_enum => {
                Some(format!("{}Codec", NamingConvention::class_name(name)))
            }
            _ => None,
        }
    }

    pub fn is_packed(&self) -> bool {
        !self.info.is_vec
            && self
                .info
                .inner
                .primitive()
                .map(|p| p.fits_in_32_bits())
                .unwrap_or(false)
    }

    pub fn is_large_primitive(&self) -> bool {
        !self.info.is_vec
            && self
                .info
                .inner
                .primitive()
                .map(|p| !p.fits_in_32_bits())
                .unwrap_or(false)
    }

    pub fn is_string(&self) -> bool {
        !self.info.is_vec && self.info.inner.is_string()
    }

    pub fn is_record(&self) -> bool {
        !self.info.is_vec && self.info.inner.is_record()
    }

    pub fn is_enum(&self) -> bool {
        !self.info.is_vec && self.info.inner.is_enum() && !self.is_data_enum
    }

    pub fn is_data_enum(&self) -> bool {
        !self.info.is_vec && self.info.inner.is_enum() && self.is_data_enum
    }

    pub fn is_vec_primitive(&self) -> bool {
        self.info.is_vec
            && self
                .info
                .inner
                .vec_inner()
                .map(|t| t.is_primitive())
                .unwrap_or(false)
    }

    pub fn is_vec_record(&self) -> bool {
        self.info.is_vec
            && self
                .info
                .inner
                .vec_inner()
                .map(|t| t.is_record())
                .unwrap_or(false)
    }

    pub fn is_vec_string(&self) -> bool {
        self.info.is_vec
            && self
                .info
                .inner
                .vec_inner()
                .map(|t| t.is_string())
                .unwrap_or(false)
    }

    pub fn is_vec_enum(&self) -> bool {
        self.info.is_vec
            && self
                .info
                .inner
                .vec_inner()
                .map(|t| t.is_enum() && !self.is_data_enum)
                .unwrap_or(false)
    }

    pub fn is_vec_data_enum(&self) -> bool {
        self.info.is_vec
            && self
                .info
                .inner
                .vec_inner()
                .map(|t| t.is_enum() && self.is_data_enum)
                .unwrap_or(false)
    }

    pub fn jni_return_type(&self) -> &'static str {
        if self.is_packed() {
            "jlong"
        } else if self.is_large_primitive() {
            "jobject"
        } else if self.is_string() {
            "jstring"
        } else if self.is_record() || self.is_data_enum() {
            "jobject"
        } else if self.is_enum() {
            "jint"
        } else if self.is_vec_primitive() {
            self.info
                .inner
                .vec_inner()
                .and_then(|t| t.primitive())
                .map(|p| primitives::info(p).array_type)
                .unwrap_or("jobject")
        } else if self.is_vec_string() {
            "jobjectArray"
        } else if self.is_vec_enum() {
            "jintArray"
        } else if self.is_vec_record() || self.is_vec_data_enum() {
            "jobject"
        } else {
            "jobject"
        }
    }

    pub fn box_class(&self) -> &'static str {
        match self.info.inner.primitive() {
            Some(Primitive::I64 | Primitive::U64 | Primitive::Isize | Primitive::Usize) => {
                "java/lang/Long"
            }
            Some(Primitive::F64) => "java/lang/Double",
            _ => "",
        }
    }

    pub fn box_signature(&self) -> &'static str {
        match self.info.inner.primitive() {
            Some(Primitive::I64 | Primitive::U64 | Primitive::Isize | Primitive::Usize) => {
                "(J)Ljava/lang/Long;"
            }
            Some(Primitive::F64) => "(D)Ljava/lang/Double;",
            _ => "",
        }
    }

    pub fn box_jni_type(&self) -> &'static str {
        match self.info.inner.primitive() {
            Some(Primitive::I64 | Primitive::U64 | Primitive::Isize | Primitive::Usize) => "jlong",
            Some(Primitive::F64) => "jdouble",
            _ => "",
        }
    }

    pub fn vec_list_suffix(&self) -> &'static str {
        if !self.info.is_vec {
            return "";
        }
        match self.info.inner.vec_inner().and_then(|t| t.primitive()) {
            Some(Primitive::U8) => "?.map { it.toUByte() }",
            Some(Primitive::U16) => "?.map { it.toUShort() }",
            Some(Primitive::U32) => "?.map { it.toUInt() }",
            Some(Primitive::U64) => "?.map { it.toULong() }",
            Some(_) => "?.toList()",
            None => "",
        }
    }
}

#[derive(Debug, Clone)]
pub enum ResultOkKind {
    Void,
    Primitive {
        c_type: String,
        jni_type: String,
    },
    String,
    Record {
        name: String,
        struct_size: usize,
    },
    Enum {
        name: String,
    },
    DataEnum {
        name: String,
        struct_size: usize,
    },
    VecPrimitive {
        primitive: Primitive,
        len_fn: String,
        copy_fn: String,
    },
    VecRecord {
        name: String,
        struct_size: usize,
        len_fn: String,
        copy_fn: String,
    },
    Option(Box<OptionView>),
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
        Self {
            ok_type,
            ok_kind,
            err_type,
            err_kind,
        }
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
                let view = OptionView::from_inner(inner, module);
                ResultOkKind::Option(Box::new(view))
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

    pub fn option_view(&self) -> Option<&OptionView> {
        match &self.ok_kind {
            ResultOkKind::Option(view) => Some(view),
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
            ResultOkKind::VecPrimitive { primitive, .. } => primitives::info(*primitive).array_type,
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

pub struct ParamConversion;

impl ParamConversion {
    pub fn to_ffi(param_name: &str, ty: &Type, module: &Module) -> String {
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
            Type::BoxedTrait(name) => module
                .callback_traits
                .iter()
                .any(|t| t.name == *name)
                .then(|| format!("{}Bridge.create({})", NamingConvention::class_name(name), param_name))
                .unwrap_or_else(|| format!("{}.handle", param_name)),
            Type::Vec(inner) | Type::Slice(inner) => match inner.as_ref() {
                Type::Record(name) => {
                    format!(
                        "{}Writer.pack({})",
                        NamingConvention::class_name(name),
                        param_name
                    )
                }
                Type::Primitive(_) => param_name.to_string(),
                _ => param_name.to_string(),
            },
            Type::Closure(sig) => Self::closure_wrapper(param_name, sig),
            _ => param_name.to_string(),
        }
    }

    fn closure_wrapper(param_name: &str, sig: &ClosureSignature) -> String {
        let interface_name = format!("{}Callback", sig.signature_id());

        let wrapper_params: Vec<String> = sig
            .params
            .iter()
            .enumerate()
            .map(|(index, ty)| match ty {
                Type::Record(_) => format!("buf{}", index),
                _ => format!("p{}", index),
            })
            .collect();

        let setup_lines = sig
            .params
            .iter()
            .enumerate()
            .filter_map(|(index, ty)| matches!(ty, Type::Record(_)).then_some(index))
            .map(|index| format!("buf{}.order(java.nio.ByteOrder.nativeOrder()); ", index))
            .collect::<Vec<_>>()
            .join("");

        let inner_args: String = sig
            .params
            .iter()
            .enumerate()
            .map(|(index, ty)| match ty {
                Type::Record(name) => format!(
                    "{}Reader.read(buf{}, 0)",
                    NamingConvention::class_name(name),
                    index
                ),
                _ => format!("p{}", index),
            })
            .collect::<Vec<_>>()
            .join(", ");

        let call = if wrapper_params.is_empty() {
            format!("{}()", param_name)
        } else {
            format!("{}({})", param_name, inner_args)
        };

        if wrapper_params.is_empty() {
            format!("{} {{ {}{} }}", interface_name, setup_lines, call)
        } else {
            format!(
                "{} {{ {} -> {}{} }}",
                interface_name,
                wrapper_params.join(", "),
                setup_lines,
                call
            )
        }
    }
}

#[derive(Debug, Clone)]
pub enum JniReturnKind {
    Void,
    Primitive {
        jni_type: String,
    },
    String {
        ffi_name: String,
    },
    Vec {
        len_fn: String,
        copy_fn: String,
    },
    CStyleEnum,
    DataEnum {
        enum_name: String,
        struct_size: usize,
    },
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

    pub fn from_type_with_module(ty: Option<&Type>, func_name: &str, module: &Module) -> Self {
        match ty {
            Some(Type::Option(inner)) => Self::Option(OptionView::from_inner(inner, module)),
            Some(Type::Result { ok, err }) => {
                Self::Result(ResultView::from_result(ok, err, module, func_name))
            }
            Some(Type::Enum(enum_name)) => module
                .enums
                .iter()
                .find(|e| &e.name == enum_name)
                .filter(|e| e.is_data_enum())
                .and_then(|e| DataEnumLayout::from_enum(e))
                .map(|layout| Self::DataEnum {
                    enum_name: NamingConvention::class_name(enum_name),
                    struct_size: layout.struct_size().as_usize(),
                })
                .unwrap_or(Self::CStyleEnum),
            _ => Self::from_type(ty, func_name),
        }
    }

    pub fn from_returns(returns: &ReturnType, func_name: &str, module: &Module) -> Self {
        match returns {
            ReturnType::Void => Self::Void,
            ReturnType::Fallible { ok, err } => {
                Self::Result(ResultView::from_result(ok, err, module, func_name))
            }
            ReturnType::Value(ty) => Self::from_type_with_module(Some(ty), func_name, module),
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
            Self::Option(view) => view.jni_return_type(),
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
    pub is_wire_param: bool,
    pub array_primitive: Option<Primitive>,
    pub array_is_mutable: bool,
    pub record_name: Option<String>,
    pub record_struct_size: usize,
    pub record_is_mutable: bool,
    pub data_enum_name: Option<String>,
    pub data_enum_struct_size: usize,
    pub closure_info: Option<ClosureParamInfo>,
}

#[derive(Debug, Clone)]
pub struct ClosureParamInfo {
    pub trampoline_name: String,
    pub signature_id: String,
    pub param_types: Vec<Type>,
    pub return_type: Type,
}

impl JniParamInfo {
    pub fn from_param(name: &str, ty: &Type) -> Self {
        let array_info = Self::extract_array_info(ty);

        let (record_name, record_is_mutable) = match ty {
            Type::Vec(inner) | Type::Slice(inner) | Type::MutSlice(inner) => match inner.as_ref() {
                Type::Record(name) => (Some(name.clone()), matches!(ty, Type::MutSlice(_))),
                _ => (None, false),
            },
            _ => (None, false),
        };

        let closure_info = match ty {
            Type::Closure(sig) => Some(ClosureParamInfo {
                trampoline_name: format!("trampoline_{}", sig.signature_id()),
                signature_id: sig.signature_id(),
                param_types: sig.params.clone(),
                return_type: (*sig.returns).clone(),
            }),
            _ => None,
        };

        Self {
            name: name.to_string(),
            jni_type: TypeMapper::c_jni_type(ty),
            is_string: matches!(ty, Type::String),
            is_handle: matches!(ty, Type::Object(_) | Type::BoxedTrait(_)),
            is_wire_param: false,
            array_primitive: array_info.primitive,
            array_is_mutable: array_info.is_mutable,
            record_name,
            record_struct_size: 0,
            record_is_mutable,
            data_enum_name: None,
            data_enum_struct_size: 0,
            closure_info,
        }
    }

    pub fn from_param_with_module(name: &str, ty: &Type, module: &Module) -> Self {
        let array_info = Self::extract_array_info(ty);
        let record_info = Self::extract_record_info(ty, module);
        let enum_info = Self::extract_enum_info(ty, module);

        let jni_type = Self::compute_jni_type(ty, &enum_info, module);

        let closure_info = match ty {
            Type::Closure(sig) => Some(ClosureParamInfo {
                trampoline_name: format!("trampoline_{}", sig.signature_id()),
                signature_id: sig.signature_id(),
                param_types: sig.params.clone(),
                return_type: (*sig.returns).clone(),
            }),
            _ => None,
        };

        let is_wire_param = Self::needs_wire_encoding(ty, module);

        Self {
            name: name.to_string(),
            jni_type,
            is_string: matches!(ty, Type::String),
            is_handle: matches!(ty, Type::Object(_) | Type::BoxedTrait(_)),
            is_wire_param,
            array_primitive: array_info.primitive,
            array_is_mutable: array_info.is_mutable,
            record_name: record_info.name,
            record_struct_size: record_info.struct_size,
            record_is_mutable: record_info.is_mutable,
            data_enum_name: enum_info.name,
            data_enum_struct_size: enum_info.struct_size,
            closure_info,
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

    fn needs_wire_encoding(ty: &Type, _module: &Module) -> bool {
        match ty {
            Type::Record(_) => true,
            Type::Enum(_) => true,
            Type::Vec(inner) | Type::Slice(inner) | Type::MutSlice(inner) => {
                !matches!(inner.as_ref(), Type::Primitive(_))
            }
            Type::Option(_) => true,
            _ => false,
        }
    }

    fn compute_jni_type(ty: &Type, enum_info: &DataEnumInfo, module: &Module) -> String {
        if enum_info.name.is_some() {
            return "jobject".to_string();
        }
        if Self::needs_wire_encoding(ty, module) {
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
        } else if self.data_enum_name.is_some() {
            format!(
                "(const uint8_t*)_{}_ptr, (uintptr_t)_{}_len",
                self.name, self.name
            )
        } else if self.record_name.is_some() {
            format!(
                "(const uint8_t*)_{}_ptr, (uintptr_t)_{}_len",
                self.name, self.name
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
        } else if self.is_wire_param {
            format!(
                "(const uint8_t*)_{}_ptr, (uintptr_t)_{}_len",
                self.name, self.name
            )
        } else if self.is_handle {
            format!("(void*){}", self.name)
        } else if let Some(closure) = &self.closure_info {
            format!("{}, (void*)_{}_ref", closure.trampoline_name, self.name)
        } else {
            self.name.clone()
        }
    }

    pub fn is_closure(&self) -> bool {
        self.closure_info.is_some()
    }

    pub fn is_primitive_array(&self) -> bool {
        self.array_primitive.is_some()
    }

    pub fn is_wire_param(&self) -> bool {
        self.is_wire_param
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
    use crate::model::{Enumeration, Primitive, RecordField, Variant};

    #[test]
    fn test_param_conversion_string() {
        let module = Module::new("test");
        assert_eq!(ParamConversion::to_ffi("name", &Type::String, &module), "name");
    }

    #[test]
    fn test_param_conversion_enum() {
        let module = Module::new("test");
        assert_eq!(
            ParamConversion::to_ffi("status", &Type::Enum("Status".into()), &module),
            "status.value"
        );
    }

    #[test]
    fn test_param_conversion_object() {
        let module = Module::new("test");
        assert_eq!(
            ParamConversion::to_ffi("sensor", &Type::Object("Sensor".into()), &module),
            "sensor.handle"
        );
    }

    #[test]
    fn test_param_conversion_primitive() {
        let module = Module::new("test");
        assert_eq!(
            ParamConversion::to_ffi("count", &Type::Primitive(Primitive::I32), &module),
            "count"
        );
    }

    #[test]
    fn test_jni_param_data_enum() {
        let mut data_enum = Enumeration::new("Result");
        data_enum.variants.push(
            Variant::new("Ok")
                .with_field(RecordField::new("value", Type::Primitive(Primitive::I32))),
        );
        data_enum.variants.push(
            Variant::new("Err")
                .with_field(RecordField::new("code", Type::Primitive(Primitive::I32))),
        );

        let mut module = Module::new("test");
        module.enums.push(data_enum);

        let param =
            JniParamInfo::from_param_with_module("result", &Type::Enum("Result".into()), &module);

        assert!(param.data_enum_name.is_some());
        assert_eq!(param.data_enum_name.as_deref(), Some("Result"));
        assert!(param.data_enum_struct_size > 0);
        assert_eq!(param.jni_type, "jobject");
    }

    #[test]
    fn test_jni_param_c_style_enum() {
        let mut c_style_enum = Enumeration::new("Status");
        c_style_enum.variants.push(Variant::new("Ok"));
        c_style_enum.variants.push(Variant::new("Error"));

        let mut module = Module::new("test");
        module.enums.push(c_style_enum);

        let param =
            JniParamInfo::from_param_with_module("status", &Type::Enum("Status".into()), &module);

        assert!(param.data_enum_name.is_none());
        assert_eq!(param.data_enum_struct_size, 0);
        assert_eq!(param.jni_type, "jobject");
        assert!(param.is_wire_param);
    }

    #[test]
    fn test_jni_return_kind_data_enum() {
        let mut data_enum = Enumeration::new("Response");
        data_enum.variants.push(
            Variant::new("Success")
                .with_field(RecordField::new("data", Type::Primitive(Primitive::I64))),
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
