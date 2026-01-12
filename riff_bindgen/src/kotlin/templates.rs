use std::collections::HashSet;

use askama::Template;
use heck::ToShoutySnakeCase;
use riff_ffi_rules::naming;

use crate::model::{
    CallbackTrait, Class, ClosureSignature, DataEnumLayout, Enumeration, Function, Method, Module,
    Primitive, Record, ReturnType, TraitMethod, TraitMethodParam, Type,
};

use super::layout::{KotlinBufferRead, KotlinBufferWrite};
use super::marshal::{OptionView, ParamConversion, ResultView, ReturnKind};
use super::primitives;
use super::{FactoryStyle, KotlinOptions, NamingConvention, TypeMapper};

#[derive(Template)]
#[template(path = "kotlin/preamble.txt", escape = "none")]
pub struct PreambleTemplate {
    pub package_name: String,
    pub prefix: String,
    pub extra_imports: Vec<String>,
}

impl PreambleTemplate {
    pub fn from_module(module: &Module) -> Self {
        let extra_imports = Self::collect_imports(module);
        Self {
            package_name: NamingConvention::class_name(&module.name).to_lowercase(),
            prefix: naming::ffi_prefix().to_string(),
            extra_imports,
        }
    }

    pub fn with_package(package_name: &str) -> Self {
        Self {
            package_name: package_name.to_string(),
            prefix: naming::ffi_prefix().to_string(),
            extra_imports: Vec::new(),
        }
    }

    pub fn with_package_and_module(package_name: &str, module: &Module) -> Self {
        let extra_imports = Self::collect_imports(module);
        Self {
            package_name: package_name.to_string(),
            prefix: naming::ffi_prefix().to_string(),
            extra_imports,
        }
    }

    fn collect_imports(module: &Module) -> Vec<String> {
        let has_async_callbacks = module
            .callback_traits
            .iter()
            .any(|t| t.async_methods().count() > 0);

        if has_async_callbacks {
            vec![
                "kotlinx.coroutines.DelicateCoroutinesApi".to_string(),
                "kotlinx.coroutines.GlobalScope".to_string(),
                "kotlinx.coroutines.launch".to_string(),
            ]
        } else {
            Vec::new()
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/closure_interface.txt", escape = "none")]
pub struct ClosureInterfaceTemplate {
    pub interface_name: String,
    pub params: Vec<ClosureParamView>,
    pub is_void_return: bool,
    pub return_type: String,
}

pub struct ClosureParamView {
    pub name: String,
    pub kotlin_type: String,
}

impl ClosureInterfaceTemplate {
    pub fn from_signature(sig: &ClosureSignature, _prefix: &str) -> Self {
        let interface_name = format!("{}Callback", sig.signature_id());
        let params: Vec<ClosureParamView> = sig
            .params
            .iter()
            .enumerate()
            .map(|(i, ty)| ClosureParamView {
                name: format!("p{}", i),
                kotlin_type: Self::closure_param_type(ty),
            })
            .collect();
        let is_void_return = sig.returns.is_void();
        let return_type = if is_void_return {
            "Unit".to_string()
        } else {
            TypeMapper::map_type(&sig.returns)
        };

        Self {
            interface_name,
            params,
            is_void_return,
            return_type,
        }
    }

    pub fn interface_name_for_signature(sig: &ClosureSignature) -> String {
        format!("{}Callback", sig.signature_id())
    }

    fn closure_param_type(ty: &Type) -> String {
        match ty {
            Type::Record(_) => "java.nio.ByteBuffer".to_string(),
            _ => TypeMapper::map_type(ty),
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/enum_c_style.txt", escape = "none")]
pub struct CStyleEnumTemplate {
    pub class_name: String,
    pub variants: Vec<EnumVariantView>,
    pub is_error: bool,
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
            is_error: enumeration.is_error,
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/enum_sealed.txt", escape = "none")]
pub struct SealedEnumTemplate {
    pub class_name: String,
    pub variants: Vec<SealedVariantView>,
    pub is_error: bool,
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
            is_error: enumeration.is_error,
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

        let enum_output = function.returns.ok_type().and_then(|ty| match ty {
            Type::Enum(name) => _module.enums.iter().find(|e| e.name == *name),
            _ => None,
        });

        let enum_name = function.returns.ok_type().and_then(|ty| match ty {
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
            .returns
            .ok_type()
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

        let return_type = function.returns.ok_type().map(TypeMapper::map_type);
        let inner_type = return_kind.inner_type().map(String::from);
        let len_fn = return_kind.len_fn().map(String::from);
        let copy_fn = return_kind.copy_fn().map(String::from);
        let reader_name = return_kind.reader_name().map(String::from);

        let option = function.returns.ok_type().and_then(|ty| match ty {
            Type::Option(inner) => Some(OptionView::from_inner(inner, _module)),
            _ => None,
        });

        let result = function.returns.as_result_types().map(|(ok, err)| {
            ResultView::from_result(ok, err, _module, &function.name)
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
#[template(path = "kotlin/function_async.txt", escape = "none")]
pub struct AsyncFunctionTemplate {
    pub func_name: String,
    pub ffi_name: String,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_free: String,
    pub ffi_cancel: String,
    pub params: Vec<ParamView>,
    pub return_type: Option<String>,
    pub complete_expr: String,
    pub has_structured_error: bool,
    pub error_codec: String,
}

impl AsyncFunctionTemplate {
    pub fn from_function(function: &Function, _module: &Module) -> Self {
        let ffi_name = naming::function_ffi_name(&function.name);

        let params: Vec<ParamView> = function
            .inputs
            .iter()
            .map(|param| {
                let param_name = NamingConvention::param_name(&param.name);
                let conversion = ParamConversion::to_ffi(&param_name, &param.param_type);
                ParamView {
                    name: param_name,
                    kotlin_type: TypeMapper::map_type(&param.param_type),
                    conversion,
                }
            })
            .collect();

        let return_type = function.returns.ok_type().map(TypeMapper::map_type);

        let complete_expr = Self::generate_complete_expr_for_returns(&function.returns, &function.name, None);

        let (has_structured_error, error_codec) = match &function.returns {
            ReturnType::Fallible { err, .. } => match err {
                Type::Enum(name) => (true, format!("{}Codec", NamingConvention::class_name(name))),
                _ => (false, String::new()),
            },
            _ => (false, String::new()),
        };

        Self {
            func_name: NamingConvention::method_name(&function.name),
            ffi_name,
            ffi_poll: naming::function_ffi_poll(&function.name),
            ffi_complete: naming::function_ffi_complete(&function.name),
            ffi_free: naming::function_ffi_free(&function.name),
            ffi_cancel: naming::function_ffi_cancel(&function.name),
            params,
            return_type,
            complete_expr,
            has_structured_error,
            error_codec,
        }
    }

    pub fn vec_primitive_conversion(call: &str, primitive: &Primitive) -> String {
        primitives::info(*primitive)
            .vec_to_unsigned
            .map(|conv| format!("({}).{}", call, conv))
            .unwrap_or_else(|| format!("({}).toList()", call))
    }

    pub fn generate_complete_expr_for_returns(
        returns: &ReturnType,
        func_name: &str,
        class_name: Option<&str>,
    ) -> String {
        let ffi_complete = match class_name {
            Some(class) => naming::method_ffi_complete(class, func_name),
            None => naming::function_ffi_complete(func_name),
        };
        let args = match class_name {
            Some(_) => "handle, future",
            None => "future",
        };
        let call = format!("Native.{}({})", ffi_complete, args);

        let ok_type = match returns {
            ReturnType::Void => return call,
            ReturnType::Value(ty) => ty,
            ReturnType::Fallible { ok, .. } => ok,
        };

        Self::apply_type_conversion(&call, ok_type)
    }

    fn apply_type_conversion(call: &str, ty: &Type) -> String {
        match ty {
            Type::Void => call.to_string(),
            Type::Primitive(p) => Self::apply_unsigned_conversion(call, p),
            Type::String => format!("{} ?: throw FfiException(-1, \"Null string\")", call),
            Type::Vec(inner) => {
                let null_check = format!("{} ?: throw FfiException(-1, \"Null array\")", call);
                match inner.as_ref() {
                    Type::Primitive(p) => Self::vec_primitive_conversion(&null_check, p),
                    _ => null_check,
                }
            }
            Type::Record(name) => {
                let reader_name = format!("{}Reader", NamingConvention::class_name(name));
                format!(
                    "useNativeBuffer({} ?: throw FfiException(-1, \"Null record\")) {{ buf -> buf.order(ByteOrder.nativeOrder()); {}.read(buf, 0) }}",
                    call, reader_name
                )
            }
            _ => call.to_string(),
        }
    }

    fn apply_unsigned_conversion(call: &str, p: &Primitive) -> String {
        primitives::info(*p)
            .to_unsigned
            .map(|conv| format!("{}.{}", call, conv))
            .unwrap_or_else(|| call.to_string())
    }
}

#[derive(Template)]
#[template(path = "kotlin/class.txt", escape = "none")]
pub struct ClassTemplate {
    pub class_name: String,
    pub doc: Option<String>,
    pub ffi_free: String,
    pub constructors: Vec<ConstructorView>,
    pub has_factory_ctors: bool,
    pub use_companion_methods: bool,
    pub methods: Vec<MethodView>,
}

pub struct ConstructorView {
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<ParamView>,
    pub is_factory: bool,
}

pub struct MethodView {
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<ParamView>,
    pub return_type: Option<String>,
    pub body: String,
    pub is_async: bool,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub complete_expr: String,
}

impl ClassTemplate {
    pub fn from_class(class: &Class, module: &Module, options: &KotlinOptions) -> Self {
        let class_name = NamingConvention::class_name(&class.name);
        let ffi_prefix = naming::class_ffi_prefix(&class.name);
        let use_companion_methods = options.factory_style == FactoryStyle::CompanionMethods;

        let constructors: Vec<ConstructorView> = class
            .constructors
            .iter()
            .filter(|ctor| {
                ctor.inputs
                    .iter()
                    .all(|param| matches!(&param.param_type, Type::Primitive(_)))
            })
            .map(|ctor| {
                let is_factory = !ctor.is_default();
                let ffi_name = if is_factory {
                    naming::method_ffi_name(&class.name, &ctor.name)
                } else {
                    format!("{}_new", ffi_prefix)
                };
                ConstructorView {
                    name: NamingConvention::method_name(&ctor.name),
                    ffi_name,
                    is_factory,
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
                }
            })
            .collect();

        let methods: Vec<MethodView> = class
            .methods
            .iter()
            .filter(|method| Self::is_supported_method(method, module))
            .map(|method| {
                let method_ffi = naming::method_ffi_name(&class.name, &method.name);
                let return_type = method.returns.ok_type().map(TypeMapper::map_type);
                let body = Self::generate_method_body(method, &method_ffi);
                let complete_expr = if method.is_async {
                    AsyncFunctionTemplate::generate_complete_expr_for_returns(&method.returns, &method.name, Some(&class.name))
                } else {
                    String::new()
                };

                MethodView {
                    name: NamingConvention::method_name(&method.name),
                    ffi_name: method_ffi.clone(),
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
                    is_async: method.is_async,
                    ffi_poll: naming::method_ffi_poll(&class.name, &method.name),
                    ffi_complete: naming::method_ffi_complete(&class.name, &method.name),
                    ffi_cancel: naming::method_ffi_cancel(&class.name, &method.name),
                    ffi_free: naming::method_ffi_free(&class.name, &method.name),
                    complete_expr,
                }
            })
            .collect();

        let has_factory_ctors = if use_companion_methods {
            constructors.iter().any(|c| c.is_factory)
        } else {
            constructors.iter().any(|c| c.is_factory && c.params.is_empty())
        };

        Self {
            class_name,
            doc: class.doc.clone(),
            ffi_free: format!("{}_free", ffi_prefix),
            constructors,
            has_factory_ctors,
            use_companion_methods,
            methods,
        }
    }

    fn is_supported_method(method: &Method, module: &Module) -> bool {
        let supported_output = if method.is_async {
            super::Kotlin::is_supported_async_output(&method.returns, module)
        } else {
            match method.returns.ok_type() {
                None => true,
                Some(Type::Void) => true,
                Some(Type::Primitive(_)) => true,
                _ => false,
            }
        };

        let supported_inputs = method
            .inputs
            .iter()
            .all(|param| matches!(&param.param_type, Type::Primitive(_) | Type::Closure(_)));

        supported_output && supported_inputs
    }

    fn generate_method_body(method: &Method, ffi_name: &str) -> String {
        let args = std::iter::once("handle".to_string())
            .chain(method.inputs.iter().map(|p| {
                ParamConversion::to_ffi(&NamingConvention::param_name(&p.name), &p.param_type)
            }))
            .collect::<Vec<_>>()
            .join(", ");

        match method.returns.ok_type() {
            Some(Type::Primitive(primitive)) => {
                let call = format!("Native.{}({})", ffi_name, args);
                let converted = primitives::info(*primitive)
                    .to_unsigned
                    .map(|conv| format!("{}.{}", call, conv))
                    .unwrap_or(call);
                format!("return {}", converted)
            }
            Some(_) => format!("return Native.{}({})", ffi_name, args),
            None => format!("Native.{}({})", ffi_name, args),
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
    pub async_callback_invokers: Vec<AsyncCallbackInvokerView>,
}

pub struct AsyncCallbackInvokerView {
    pub name: String,
    pub jni_type: String,
    pub has_result: bool,
}

pub struct NativeFunctionView {
    pub ffi_name: String,
    pub params: Vec<NativeParamView>,
    pub has_out_param: bool,
    pub out_type: String,
    pub return_jni_type: String,
    pub is_async: bool,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub complete_return_jni_type: String,
}

pub struct NativeParamView {
    pub name: String,
    pub jni_type: String,
}

pub struct NativeClassView {
    pub ffi_new: String,
    pub ffi_free: String,
    pub ctor_params: Vec<NativeParamView>,
    pub factory_ctors: Vec<NativeFactoryCtorView>,
    pub methods: Vec<NativeMethodView>,
}

pub struct NativeFactoryCtorView {
    pub ffi_name: String,
    pub params: Vec<NativeParamView>,
}

pub struct NativeMethodView {
    pub ffi_name: String,
    pub params: Vec<NativeParamView>,
    pub has_out_param: bool,
    pub out_type: String,
    pub return_jni_type: String,
    pub is_async: bool,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
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
                    Self::analyze_return(&func.returns, module);

                NativeFunctionView {
                    ffi_name: ffi_name.clone(),
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
                    return_jni_type: return_jni_type.clone(),
                    is_async: func.is_async,
                    ffi_poll: naming::function_ffi_poll(&func.name),
                    ffi_complete: naming::function_ffi_complete(&func.name),
                    ffi_cancel: naming::function_ffi_cancel(&func.name),
                    ffi_free: naming::function_ffi_free(&func.name),
                    complete_return_jni_type: Self::async_complete_return_type(&func.returns, &return_jni_type),
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
                    .iter()
                    .find(|c| c.is_default())
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

                let factory_ctors: Vec<NativeFactoryCtorView> = class
                    .constructors
                    .iter()
                    .filter(|c| !c.is_default())
                    .filter(|c| c.inputs.iter().all(|p| matches!(&p.param_type, Type::Primitive(_))))
                    .map(|ctor| NativeFactoryCtorView {
                        ffi_name: naming::method_ffi_name(&class.name, &ctor.name),
                        params: ctor
                            .inputs
                            .iter()
                            .map(|p| NativeParamView {
                                name: NamingConvention::param_name(&p.name),
                                jni_type: TypeMapper::jni_type(&p.param_type),
                            })
                            .collect(),
                    })
                    .collect();

                let methods: Vec<NativeMethodView> = class
                    .methods
                    .iter()
                    .filter(|method| {
                        let supported_output = if method.is_async {
                            super::Kotlin::is_supported_async_output(&method.returns, module)
                        } else {
                            match method.returns.ok_type() {
                                None => true,
                                Some(Type::Void) => true,
                                Some(Type::Primitive(_)) => true,
                                _ => false,
                            }
                        };

                        let supported_inputs = method
                            .inputs
                            .iter()
                            .all(|param| matches!(&param.param_type, Type::Primitive(_) | Type::Closure(_)));

                        supported_output && supported_inputs
                    })
                    .map(|method| {
                        let method_ffi = naming::method_ffi_name(&class.name, &method.name);
                        let (has_out_param, out_type, return_jni_type) =
                            Self::analyze_return(&method.returns, module);

                        NativeMethodView {
                            ffi_name: method_ffi.clone(),
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
                            is_async: method.is_async,
                            ffi_poll: naming::method_ffi_poll(&class.name, &method.name),
                            ffi_complete: naming::method_ffi_complete(&class.name, &method.name),
                            ffi_cancel: naming::method_ffi_cancel(&class.name, &method.name),
                            ffi_free: naming::method_ffi_free(&class.name, &method.name),
                        }
                    })
                    .collect();

                NativeClassView {
                    ffi_new: format!("{}_new", ffi_prefix),
                    ffi_free: format!("{}_free", ffi_prefix),
                    ctor_params,
                    factory_ctors,
                    methods,
                }
            })
            .collect();

        let async_callback_invokers = Self::collect_async_callback_invokers(module);

        Self {
            lib_name: format!("{}_jni", module.name),
            prefix,
            functions,
            classes,
            async_callback_invokers,
        }
    }

    fn collect_async_callback_invokers(module: &Module) -> Vec<AsyncCallbackInvokerView> {
        let mut seen = HashSet::new();
        module
            .callback_traits
            .iter()
            .flat_map(|t| t.async_methods())
            .filter_map(|method| {
                let suffix = Self::async_invoker_suffix_for_type(&method.returns);
                if seen.insert(suffix.clone()) {
                    Some(Self::build_invoker_view(&suffix, &method.returns))
                } else {
                    None
                }
            })
            .collect()
    }

    fn async_invoker_suffix_for_type(returns: &ReturnType) -> String {
        match returns.ok_type() {
            None => "Void".to_string(),
            Some(Type::Void) => "Void".to_string(),
            Some(Type::Primitive(p)) => primitives::info(*p).invoker_suffix.to_string(),
            Some(Type::String) => "String".to_string(),
            _ => "Object".to_string(),
        }
    }

    fn build_invoker_view(suffix: &str, returns: &ReturnType) -> AsyncCallbackInvokerView {
        let (jni_type, has_result) = match suffix {
            "Void" => ("Unit".to_string(), false),
            "Bool" => ("Boolean".to_string(), true),
            "I8" => ("Byte".to_string(), true),
            "I16" => ("Short".to_string(), true),
            "I32" => ("Int".to_string(), true),
            "I64" => ("Long".to_string(), true),
            "F32" => ("Float".to_string(), true),
            "F64" => ("Double".to_string(), true),
            _ => ("Any".to_string(), true),
        };

        AsyncCallbackInvokerView {
            name: format!("invokeAsyncCallback{}", suffix),
            jni_type,
            has_result,
        }
    }

    fn analyze_return(returns: &ReturnType, module: &Module) -> (bool, String, String) {
        match returns {
            ReturnType::Void => (false, String::new(), "Unit".to_string()),
            ReturnType::Fallible { ok, .. } => Self::analyze_result_return(ok, module),
            ReturnType::Value(ty) => match ty {
                Type::Void => (false, String::new(), "Unit".to_string()),
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
                Type::Record(_) => (false, String::new(), "ByteBuffer?".to_string()),
                Type::Enum(enum_name)
                    if module
                        .enums
                        .iter()
                        .find(|e| e.name == *enum_name)
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

    fn async_complete_return_type(returns: &ReturnType, base_type: &str) -> String {
        match returns.ok_type() {
            Some(Type::Vec(inner)) => match inner.as_ref() {
                Type::Primitive(_) => format!("{}?", base_type),
                _ => base_type.to_string(),
            },
            _ => base_type.to_string(),
        }
    }

}

#[derive(Template)]
#[template(path = "kotlin/callback_trait.txt", escape = "none")]
pub struct CallbackTraitTemplate {
    pub doc: Option<String>,
    pub interface_name: String,
    pub wrapper_class: String,
    pub handle_map_name: String,
    pub callbacks_object: String,
    pub bridge_name: String,
    pub vtable_type: String,
    pub register_fn: String,
    pub create_fn: String,
    pub sync_methods: Vec<SyncMethodView>,
    pub async_methods: Vec<AsyncMethodView>,
    pub has_async: bool,
}

pub struct CallbackReturnInfo {
    pub kotlin_type: String,
    pub jni_type: String,
    pub default_value: String,
    pub to_jni: String,
}

pub struct SyncMethodView {
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<TraitParamView>,
    pub return_info: Option<CallbackReturnInfo>,
}

pub struct AsyncMethodView {
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<TraitParamView>,
    pub return_info: Option<CallbackReturnInfo>,
    pub invoker_name: String,
}

pub struct TraitParamView {
    pub name: String,
    pub ffi_name: String,
    pub kotlin_type: String,
    pub jni_type: String,
    pub conversion: String,
}

impl CallbackTraitTemplate {
    pub fn from_trait(callback_trait: &CallbackTrait, _module: &Module) -> Self {
        let trait_name = &callback_trait.name;
        let interface_name = NamingConvention::class_name(trait_name);

        let sync_methods: Vec<SyncMethodView> = callback_trait
            .sync_methods()
            .filter(|method| Self::is_supported_callback_method(method))
            .map(|method| Self::build_sync_method(method))
            .collect();

        let async_methods: Vec<AsyncMethodView> = callback_trait
            .async_methods()
            .filter(|method| Self::is_supported_callback_method(method))
            .map(|method| Self::build_async_method(method))
            .collect();

        let has_async = !async_methods.is_empty();

        Self {
            doc: callback_trait.doc.clone(),
            interface_name: interface_name.clone(),
            wrapper_class: format!("{}Wrapper", interface_name),
            handle_map_name: format!("{}HandleMap", interface_name),
            callbacks_object: format!("{}Callbacks", interface_name),
            bridge_name: format!("{}Bridge", interface_name),
            vtable_type: naming::callback_vtable_name(trait_name),
            register_fn: naming::callback_register_fn(trait_name),
            create_fn: naming::callback_create_fn(trait_name),
            sync_methods,
            async_methods,
            has_async,
        }
    }

    fn build_sync_method(method: &TraitMethod) -> SyncMethodView {
        let return_info = Self::build_return_info(&method.returns);
        SyncMethodView {
            name: NamingConvention::method_name(&method.name),
            ffi_name: naming::to_snake_case(&method.name),
            params: Self::build_params(&method.inputs),
            return_info,
        }
    }

    fn build_async_method(method: &TraitMethod) -> AsyncMethodView {
        let return_info = Self::build_return_info(&method.returns);
        let invoker_suffix = Self::async_invoker_suffix(&method.returns);
        AsyncMethodView {
            name: NamingConvention::method_name(&method.name),
            ffi_name: naming::to_snake_case(&method.name),
            params: Self::build_params(&method.inputs),
            return_info,
            invoker_name: format!("invokeAsyncCallback{}", invoker_suffix),
        }
    }

    fn build_return_info(returns: &ReturnType) -> Option<CallbackReturnInfo> {
        returns.ok_type().and_then(|ty| {
            if matches!(ty, Type::Void) {
                None
            } else {
                Some(CallbackReturnInfo {
                    kotlin_type: TypeMapper::map_type(ty),
                    jni_type: TypeMapper::jni_type(ty),
                    default_value: Self::default_value(ty),
                    to_jni: Self::jni_return_conversion(ty),
                })
            }
        })
    }

    fn build_params(inputs: &[TraitMethodParam]) -> Vec<TraitParamView> {
        inputs
            .iter()
            .map(|param| {
                let kotlin_name = NamingConvention::param_name(&param.name);
                TraitParamView {
                    name: kotlin_name.clone(),
                    ffi_name: param.name.clone(),
                    kotlin_type: TypeMapper::map_type(&param.param_type),
                    jni_type: TypeMapper::jni_type(&param.param_type),
                    conversion: Self::jni_param_conversion(&kotlin_name, &param.param_type),
                }
            })
            .collect()
    }

    fn async_invoker_suffix(returns: &ReturnType) -> String {
        match returns.ok_type() {
            None => "Void".to_string(),
            Some(Type::Void) => "Void".to_string(),
            Some(Type::Primitive(p)) => primitives::info(*p).invoker_suffix.to_string(),
            Some(Type::String) => "String".to_string(),
            _ => "Object".to_string(),
        }
    }

    fn jni_return_conversion(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => primitives::info(*p)
                .jni_return_cast
                .map(String::from)
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn jni_param_conversion(name: &str, ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => primitives::info(*p)
                .jni_param_cast
                .map(|cast| format!("{}.{}", name, cast))
                .unwrap_or_else(|| name.to_string()),
            _ => name.to_string(),
        }
    }

    fn is_supported_callback_method(method: &TraitMethod) -> bool {
        let supported_return = match method.returns.ok_type() {
            None => true,
            Some(Type::Void) => true,
            Some(Type::Primitive(_)) => true,
            _ => false,
        };

        let supported_params = method.inputs.iter().all(|param| {
            matches!(&param.param_type, Type::Primitive(_))
        });

        supported_return && supported_params
    }

    fn default_value(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => primitives::info(*p).callback_default.to_string(),
            Type::String => "\"\"".to_string(),
            Type::Void => "Unit".to_string(),
            _ => "throw IllegalStateException(\"Handle not found\")".to_string(),
        }
    }
}
