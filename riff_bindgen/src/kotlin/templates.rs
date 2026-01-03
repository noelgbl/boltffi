use askama::Template;
use heck::ToShoutySnakeCase;
use riff_ffi_rules::naming;

use crate::model::{Class, DataEnumLayout, Enumeration, Function, Module, Record, Type};

use super::layout::{KotlinBufferRead, KotlinBufferWrite};
use super::marshal::{OptionView, ParamConversion, ResultView, ReturnKind};
use super::{NamingConvention, TypeMapper};

#[derive(Template)]
#[template(path = "kotlin/preamble.txt", escape = "none")]
pub struct PreambleTemplate {
    pub package_name: String,
    pub prefix: String,
}

impl PreambleTemplate {
    pub fn from_module(module: &Module) -> Self {
        Self {
            package_name: NamingConvention::class_name(&module.name).to_lowercase(),
            prefix: naming::ffi_prefix().to_string(),
        }
    }

    pub fn with_package(package_name: &str) -> Self {
        Self {
            package_name: package_name.to_string(),
            prefix: naming::ffi_prefix().to_string(),
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/enum_c_style.txt", escape = "none")]
pub struct CStyleEnumTemplate {
    pub class_name: String,
    pub variants: Vec<EnumVariantView>,
}

pub struct EnumVariantView {
    pub name: String,
    pub value: i64,
}

impl CStyleEnumTemplate {
    pub fn from_enum(enumeration: &Enumeration) -> Self {
        let variants = enumeration
            .variants
            .iter()
            .enumerate()
            .map(|(index, variant)| EnumVariantView {
                name: NamingConvention::enum_entry_name(&variant.name),
                value: variant.discriminant.unwrap_or(index as i64),
            })
            .collect();

        Self {
            class_name: NamingConvention::class_name(&enumeration.name),
            variants,
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/enum_sealed.txt", escape = "none")]
pub struct SealedEnumTemplate {
    pub class_name: String,
    pub variants: Vec<SealedVariantView>,
}

pub struct SealedVariantView {
    pub name: String,
    pub is_tuple: bool,
    pub fields: Vec<SealedFieldView>,
}

pub struct SealedFieldView {
    pub name: String,
    pub index: usize,
    pub kotlin_type: String,
    pub is_tuple: bool,
}

#[derive(Template)]
#[template(path = "kotlin/enum_data_codec.txt", escape = "none")]
pub struct DataEnumCodecTemplate {
    pub codec_name: String,
    pub class_name: String,
    pub struct_size: usize,
    pub payload_offset: usize,
    pub variants: Vec<DataEnumVariantView>,
}

pub struct DataEnumVariantView {
    pub name: String,
    pub const_name: String,
    pub tag_value: i32,
    pub fields: Vec<DataEnumFieldView>,
}

pub struct DataEnumFieldView {
    pub param_name: String,
    pub offset: usize,
    pub getter: String,
    pub conversion: String,
    pub putter: String,
    pub value_expr: String,
}

impl DataEnumCodecTemplate {
    pub fn from_enum(enumeration: &Enumeration) -> Self {
        let layout = DataEnumLayout::from_enum(enumeration)
            .expect("DataEnumCodecTemplate used for c-style enum");
        let payload_offset = layout.payload_offset().as_usize();
        let struct_size = layout.struct_size().as_usize();

        let variants = enumeration
            .variants
            .iter()
            .enumerate()
            .map(|(variant_index, variant)| {
                let tag_value = variant
                    .discriminant
                    .unwrap_or(variant_index as i64)
                    .try_into()
                    .unwrap_or(variant_index as i32);

                let fields = variant
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(field_index, field)| {
                        let field_is_tuple = field.name.starts_with('_')
                            && field
                                .name
                                .chars()
                                .nth(1)
                                .map_or(false, |c| c.is_ascii_digit());
                        let param_name = if field_is_tuple {
                            format!("value{}", field_index)
                        } else {
                            NamingConvention::property_name(&field.name)
                        };

                        let raw_value_expr = format!("value.{}", param_name);
                        let offset = layout
                            .field_offset(variant_index, field_index)
                            .unwrap_or_default()
                            .as_usize();

                        let (getter, conversion, putter, value_expr) = match &field.field_type {
                            Type::Primitive(primitive) => (
                                primitive.buffer_getter().to_string(),
                                primitive.buffer_conversion().to_string(),
                                primitive.buffer_putter().to_string(),
                                primitive.buffer_value_expr(&raw_value_expr),
                            ),
                            _ => (
                                "getLong".to_string(),
                                String::new(),
                                "putLong".to_string(),
                                raw_value_expr,
                            ),
                        };

                        DataEnumFieldView {
                            param_name,
                            offset,
                            getter,
                            conversion,
                            putter,
                            value_expr,
                        }
                    })
                    .collect();

                DataEnumVariantView {
                    name: NamingConvention::class_name(&variant.name),
                    const_name: variant.name.to_shouty_snake_case(),
                    tag_value,
                    fields,
                }
            })
            .collect();

        let class_name = NamingConvention::class_name(&enumeration.name);

        Self {
            codec_name: format!("{}Codec", class_name),
            class_name,
            struct_size,
            payload_offset,
            variants,
        }
    }
}

impl SealedEnumTemplate {
    pub fn from_enum(enumeration: &Enumeration) -> Self {
        let variants = enumeration
            .variants
            .iter()
            .map(|variant| {
                let is_tuple = variant.fields.iter().any(|f| {
                    f.name.starts_with('_')
                        && f.name.chars().nth(1).map_or(false, |c| c.is_ascii_digit())
                });
                SealedVariantView {
                    name: NamingConvention::class_name(&variant.name),
                    is_tuple,
                    fields: variant
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(i, field)| {
                            let field_is_tuple = field.name.starts_with('_')
                                && field
                                    .name
                                    .chars()
                                    .nth(1)
                                    .map_or(false, |c| c.is_ascii_digit());
                            SealedFieldView {
                                name: NamingConvention::property_name(&field.name),
                                index: i,
                                kotlin_type: TypeMapper::map_type(&field.field_type),
                                is_tuple: field_is_tuple,
                            }
                        })
                        .collect(),
                }
            })
            .collect();

        Self {
            class_name: NamingConvention::class_name(&enumeration.name),
            variants,
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/record.txt", escape = "none")]
pub struct RecordTemplate {
    pub class_name: String,
    pub fields: Vec<FieldView>,
}

pub struct FieldView {
    pub name: String,
    pub kotlin_type: String,
}

impl RecordTemplate {
    pub fn from_record(record: &Record) -> Self {
        let fields = record
            .fields
            .iter()
            .map(|field| FieldView {
                name: NamingConvention::property_name(&field.name),
                kotlin_type: TypeMapper::map_type(&field.field_type),
            })
            .collect();

        Self {
            class_name: NamingConvention::class_name(&record.name),
            fields,
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/record_reader.txt", escape = "none")]
pub struct RecordReaderTemplate {
    pub reader_name: String,
    pub class_name: String,
    pub struct_size: usize,
    pub fields: Vec<ReaderFieldView>,
}

pub struct ReaderFieldView {
    pub name: String,
    pub const_name: String,
    pub offset: usize,
    pub getter: String,
    pub conversion: String,
}

impl RecordReaderTemplate {
    pub fn from_record(record: &Record) -> Self {
        let offsets = record.field_offsets();
        let fields = record
            .fields
            .iter()
            .zip(offsets)
            .map(|(field, offset)| {
                let (getter, conversion) = match &field.field_type {
                    Type::Primitive(primitive) => (
                        primitive.buffer_getter().to_string(),
                        primitive.buffer_conversion().to_string(),
                    ),
                    _ => ("getLong".to_string(), String::new()),
                };

                ReaderFieldView {
                    name: NamingConvention::property_name(&field.name),
                    const_name: field.name.to_shouty_snake_case(),
                    offset,
                    getter,
                    conversion,
                }
            })
            .collect();

        Self {
            reader_name: format!("{}Reader", NamingConvention::class_name(&record.name)),
            class_name: NamingConvention::class_name(&record.name),
            struct_size: record.struct_size().as_usize(),
            fields,
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/record_writer.txt", escape = "none")]
pub struct RecordWriterTemplate {
    pub writer_name: String,
    pub class_name: String,
    pub struct_size: usize,
    pub fields: Vec<WriterFieldView>,
}

pub struct WriterFieldView {
    pub name: String,
    pub const_name: String,
    pub offset: usize,
    pub putter: String,
    pub value_expr: String,
}

impl RecordWriterTemplate {
    pub fn from_record(record: &Record) -> Self {
        let offsets = record.field_offsets();
        let fields = record
            .fields
            .iter()
            .zip(offsets)
            .map(|(field, offset)| {
                let field_name = NamingConvention::property_name(&field.name);
                let item_expr = format!("item.{}", field_name);

                let (putter, value_expr) = match &field.field_type {
                    Type::Primitive(primitive) => (
                        primitive.buffer_putter().to_string(),
                        primitive.buffer_value_expr(&item_expr),
                    ),
                    _ => ("putLong".to_string(), item_expr),
                };

                WriterFieldView {
                    name: field_name,
                    const_name: field.name.to_shouty_snake_case(),
                    offset,
                    putter,
                    value_expr,
                }
            })
            .collect();

        Self {
            writer_name: format!("{}Writer", NamingConvention::class_name(&record.name)),
            class_name: NamingConvention::class_name(&record.name),
            struct_size: record.struct_size().as_usize(),
            fields,
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/function.txt", escape = "none")]
pub struct FunctionTemplate {
    pub func_name: String,
    pub ffi_name: String,
    pub prefix: String,
    pub params: Vec<ParamView>,
    pub return_type: Option<String>,
    pub return_kind: ReturnKind,
    pub enum_name: Option<String>,
    pub enum_codec_name: Option<String>,
    pub enum_is_data: bool,
    pub inner_type: Option<String>,
    pub len_fn: Option<String>,
    pub copy_fn: Option<String>,
    pub reader_name: Option<String>,
    pub is_async: bool,
    pub option: Option<OptionView>,
    pub result: Option<ResultView>,
}

pub struct ParamView {
    pub name: String,
    pub kotlin_type: String,
    pub conversion: String,
}

impl FunctionTemplate {
    pub fn from_function(function: &Function, _module: &Module) -> Self {
        let ffi_name = format!("{}_{}", naming::ffi_prefix(), function.name);

        let enum_output = function.output.as_ref().and_then(|ty| match ty {
            Type::Enum(name) => _module.enums.iter().find(|e| &e.name == name),
            _ => None,
        });

        let enum_name = function.output.as_ref().and_then(|ty| match ty {
            Type::Enum(name) => Some(NamingConvention::class_name(name)),
            _ => None,
        });

        let enum_is_data = enum_output.map(|e| e.is_data_enum()).unwrap_or(false);
        let enum_codec_name = if enum_is_data {
            enum_name.as_ref().map(|name| format!("{}Codec", name))
        } else {
            None
        };

        let return_kind = function
            .output
            .as_ref()
            .map(|ty| ReturnKind::from_type(ty, &ffi_name))
            .unwrap_or(ReturnKind::Void);

        let params: Vec<ParamView> = function
            .inputs
            .iter()
            .map(|param| {
                let param_name = NamingConvention::param_name(&param.name);

                let conversion = match &param.param_type {
                    Type::Enum(enum_name) => {
                        let is_data_enum = _module
                            .enums
                            .iter()
                            .find(|e| &e.name == enum_name)
                            .map(|e| e.is_data_enum())
                            .unwrap_or(false);

                        if is_data_enum {
                            format!(
                                "{}Codec.pack({})",
                                NamingConvention::class_name(enum_name),
                                param_name
                            )
                        } else {
                            ParamConversion::to_ffi(&param_name, &param.param_type)
                        }
                    }
                    _ => ParamConversion::to_ffi(&param_name, &param.param_type),
                };

                ParamView {
                    name: param_name,
                    kotlin_type: TypeMapper::map_type(&param.param_type),
                    conversion,
                }
            })
            .collect();

        let return_type = function.output.as_ref().map(|ty| match ty {
            Type::Result { ok, .. } => TypeMapper::map_type(ok),
            other => TypeMapper::map_type(other),
        });
        let inner_type = return_kind.inner_type().map(String::from);
        let len_fn = return_kind.len_fn().map(String::from);
        let copy_fn = return_kind.copy_fn().map(String::from);
        let reader_name = return_kind.reader_name().map(String::from);

        let option = function.output.as_ref().and_then(|ty| match ty {
            Type::Option(inner) => Some(OptionView::from_inner(inner, _module)),
            _ => None,
        });

        let result = function.output.as_ref().and_then(|ty| match ty {
            Type::Result { ok, err } => {
                Some(ResultView::from_result(ok, err, _module, &function.name))
            }
            _ => None,
        });

        Self {
            func_name: NamingConvention::method_name(&function.name),
            ffi_name,
            prefix: naming::ffi_prefix().to_string(),
            params,
            return_type,
            return_kind,
            enum_name,
            enum_codec_name,
            enum_is_data,
            inner_type,
            len_fn,
            copy_fn,
            reader_name,
            is_async: function.is_async,
            option,
            result,
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/class.txt", escape = "none")]
pub struct ClassTemplate {
    pub class_name: String,
    pub doc: Option<String>,
    pub ffi_free: String,
    pub constructors: Vec<ConstructorView>,
    pub methods: Vec<MethodView>,
}

pub struct ConstructorView {
    pub ffi_name: String,
    pub params: Vec<ParamView>,
}

pub struct MethodView {
    pub name: String,
    pub params: Vec<ParamView>,
    pub return_type: Option<String>,
    pub body: String,
}

impl ClassTemplate {
    pub fn from_class(class: &Class) -> Self {
        let class_name = NamingConvention::class_name(&class.name);
        let ffi_prefix = naming::class_ffi_prefix(&class.name);

        let constructors: Vec<ConstructorView> = class
            .constructors
            .iter()
            .filter(|ctor| {
                ctor.inputs
                    .iter()
                    .all(|param| matches!(&param.param_type, Type::Primitive(_)))
            })
            .map(|ctor| ConstructorView {
                ffi_name: format!("{}_new", ffi_prefix),
                params: ctor
                    .inputs
                    .iter()
                    .map(|param| ParamView {
                        name: NamingConvention::param_name(&param.name),
                        kotlin_type: TypeMapper::map_type(&param.param_type),
                        conversion: ParamConversion::to_ffi(
                            &NamingConvention::param_name(&param.name),
                            &param.param_type,
                        ),
                    })
                    .collect(),
            })
            .collect();

        let methods: Vec<MethodView> = class
            .methods
            .iter()
            .filter(|method| Self::is_supported_method(method))
            .map(|method| {
                let method_ffi = naming::method_ffi_name(&class.name, &method.name);
                let return_type = method.output.as_ref().map(TypeMapper::map_type);
                let body = Self::generate_method_body(method, &method_ffi);

                MethodView {
                    name: NamingConvention::method_name(&method.name),
                    params: method
                        .inputs
                        .iter()
                        .map(|param| ParamView {
                            name: NamingConvention::param_name(&param.name),
                            kotlin_type: TypeMapper::map_type(&param.param_type),
                            conversion: ParamConversion::to_ffi(
                                &NamingConvention::param_name(&param.name),
                                &param.param_type,
                            ),
                        })
                        .collect(),
                    return_type,
                    body,
                }
            })
            .collect();

        Self {
            class_name,
            doc: class.doc.clone(),
            ffi_free: format!("{}_free", ffi_prefix),
            constructors,
            methods,
        }
    }

    fn is_supported_method(method: &crate::model::Method) -> bool {
        let supported_output = match &method.output {
            None => true,
            Some(Type::Primitive(_)) => true,
            _ => false,
        };

        let supported_inputs = method
            .inputs
            .iter()
            .all(|param| matches!(&param.param_type, Type::Primitive(_)));

        supported_output && supported_inputs
    }

    fn generate_method_body(method: &crate::model::Method, ffi_name: &str) -> String {
        let args = std::iter::once("handle".to_string())
            .chain(method.inputs.iter().map(|p| {
                ParamConversion::to_ffi(&NamingConvention::param_name(&p.name), &p.param_type)
            }))
            .collect::<Vec<_>>()
            .join(", ");

        match &method.output {
            Some(Type::Primitive(primitive)) => {
                let call = format!("Native.{}({})", ffi_name, args);
                let converted = match primitive {
                    crate::model::Primitive::U8 => format!("{}.toUByte()", call),
                    crate::model::Primitive::U16 => format!("{}.toUShort()", call),
                    crate::model::Primitive::U32 => format!("{}.toUInt()", call),
                    crate::model::Primitive::U64 => format!("{}.toULong()", call),
                    _ => call,
                };
                format!("return {}", converted)
            }
            Some(_) => format!("return Native.{}({})", ffi_name, args),
            None => format!(
                "val status = Native.{}({})\n        checkStatus(status)",
                ffi_name, args
            ),
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/native.txt", escape = "none")]
pub struct NativeTemplate {
    pub lib_name: String,
    pub prefix: String,
    pub functions: Vec<NativeFunctionView>,
    pub classes: Vec<NativeClassView>,
}

pub struct NativeFunctionView {
    pub ffi_name: String,
    pub params: Vec<NativeParamView>,
    pub has_out_param: bool,
    pub out_type: String,
    pub return_jni_type: String,
}

pub struct NativeParamView {
    pub name: String,
    pub jni_type: String,
}

pub struct NativeClassView {
    pub ffi_new: String,
    pub ffi_free: String,
    pub ctor_params: Vec<NativeParamView>,
    pub methods: Vec<NativeMethodView>,
}

pub struct NativeMethodView {
    pub ffi_name: String,
    pub params: Vec<NativeParamView>,
    pub has_out_param: bool,
    pub out_type: String,
    pub return_jni_type: String,
}

impl NativeTemplate {
    pub fn from_module(module: &Module) -> Self {
        let prefix = naming::ffi_prefix().to_string();

        let functions: Vec<NativeFunctionView> = module
            .functions
            .iter()
            .map(|func| {
                let ffi_name = naming::function_ffi_name(&func.name);
                let (has_out_param, out_type, return_jni_type) =
                    Self::analyze_return(&func.output, module);

                NativeFunctionView {
                    ffi_name,
                    params: func
                        .inputs
                        .iter()
                        .map(|p| NativeParamView {
                            name: NamingConvention::param_name(&p.name),
                            jni_type: match &p.param_type {
                                Type::Vec(inner) | Type::Slice(inner)
                                    if matches!(inner.as_ref(), Type::Record(_)) =>
                                {
                                    "ByteArray".to_string()
                                }
                                Type::Enum(enum_name)
                                    if module
                                        .enums
                                        .iter()
                                        .find(|e| &e.name == enum_name)
                                        .map(|e| e.is_data_enum())
                                        .unwrap_or(false) =>
                                {
                                    "ByteArray".to_string()
                                }
                                _ => TypeMapper::jni_type(&p.param_type),
                            },
                        })
                        .collect(),
                    has_out_param,
                    out_type,
                    return_jni_type,
                }
            })
            .collect();

        let classes: Vec<NativeClassView> = module
            .classes
            .iter()
            .map(|class| {
                let ffi_prefix = naming::class_ffi_prefix(&class.name);

                let ctor_params: Vec<NativeParamView> = class
                    .constructors
                    .first()
                    .map(|ctor| {
                        ctor.inputs
                            .iter()
                            .filter(|param| matches!(&param.param_type, Type::Primitive(_)))
                            .map(|p| NativeParamView {
                                name: NamingConvention::param_name(&p.name),
                                jni_type: TypeMapper::jni_type(&p.param_type),
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let methods: Vec<NativeMethodView> = class
                    .methods
                    .iter()
                    .filter(|method| {
                        let supported_output = match &method.output {
                            None => true,
                            Some(Type::Primitive(_)) => true,
                            _ => false,
                        };

                        let supported_inputs = method
                            .inputs
                            .iter()
                            .all(|param| matches!(&param.param_type, Type::Primitive(_)));

                        supported_output && supported_inputs
                    })
                    .map(|method| {
                        let method_ffi = naming::method_ffi_name(&class.name, &method.name);
                        let (has_out_param, out_type, return_jni_type) =
                            Self::analyze_return(&method.output, module);

                        NativeMethodView {
                            ffi_name: method_ffi,
                            params: method
                                .inputs
                                .iter()
                                .map(|p| NativeParamView {
                                    name: NamingConvention::param_name(&p.name),
                                    jni_type: TypeMapper::jni_type(&p.param_type),
                                })
                                .collect(),
                            has_out_param,
                            out_type,
                            return_jni_type,
                        }
                    })
                    .collect();

                NativeClassView {
                    ffi_new: format!("{}_new", ffi_prefix),
                    ffi_free: format!("{}_free", ffi_prefix),
                    ctor_params,
                    methods,
                }
            })
            .collect();

        Self {
            lib_name: format!("{}_jni", module.name),
            prefix,
            functions,
            classes,
        }
    }

    fn analyze_return(output: &Option<Type>, module: &Module) -> (bool, String, String) {
        match output {
            None => (false, String::new(), "Int".to_string()),
            Some(ty) => match ty {
                Type::Primitive(_) => (false, String::new(), TypeMapper::jni_type(ty)),
                Type::String => (false, String::new(), "String?".to_string()),
                Type::Bytes => (false, String::new(), "ByteArray?".to_string()),
                Type::Option(inner) => {
                    let view = OptionView::from_inner(inner, module);
                    (false, String::new(), view.kotlin_native_type)
                }
                Type::Vec(inner) => match inner.as_ref() {
                    Type::Primitive(_) => (false, String::new(), TypeMapper::jni_type(ty)),
                    Type::Record(_) => (false, String::new(), "ByteBuffer".to_string()),
                    _ => (false, String::new(), "Long".to_string()),
                },
                Type::Result { ok, .. } => Self::analyze_result_return(ok, module),
                Type::Enum(enum_name)
                    if module
                        .enums
                        .iter()
                        .find(|e| &e.name == enum_name)
                        .map(|e| e.is_data_enum())
                        .unwrap_or(false) =>
                {
                    (false, String::new(), "ByteBuffer".to_string())
                }
                _ => (false, String::new(), TypeMapper::jni_type(ty)),
            },
        }
    }

    fn analyze_result_return(ok: &Type, module: &Module) -> (bool, String, String) {
        match ok {
            Type::Void => (false, String::new(), "Unit".to_string()),
            Type::Primitive(_) => (false, String::new(), TypeMapper::jni_type(ok)),
            Type::String => (false, String::new(), "String?".to_string()),
            Type::Record(_) => (false, String::new(), "ByteBuffer?".to_string()),
            Type::Enum(enum_name) => {
                let is_data_enum = module
                    .enums
                    .iter()
                    .find(|e| &e.name == enum_name)
                    .map(|e| e.is_data_enum())
                    .unwrap_or(false);
                if is_data_enum {
                    (false, String::new(), "ByteBuffer?".to_string())
                } else {
                    (false, String::new(), "Int".to_string())
                }
            }
            _ => (false, String::new(), TypeMapper::jni_type(ok)),
        }
    }
}
