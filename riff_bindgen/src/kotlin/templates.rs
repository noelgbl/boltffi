use std::collections::HashSet;

use askama::Template;
use heck::ToShoutySnakeCase;
use riff_ffi_rules::naming;

use crate::model::{
    CallbackTrait, Class, ClosureSignature, DataEnumLayout, Enumeration, Function, Method, Module,
    Primitive, Record, RecordField, ReturnType, TraitMethod, TraitMethodParam, Type,
};

use super::layout::{KotlinBufferRead, KotlinBufferWrite};
use super::call_plan::{AsyncCallPlan, WireFunctionPlan};
use super::marshal::{OptionView, ParamConversion};
use super::primitives;
use super::return_abi::ReturnAbi;
use super::{FactoryStyle, KotlinOptions, NamingConvention, TypeMapper};

use self::MethodImpl::{AsyncMethod, SyncMethod};

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
    pub tag: usize,
    pub is_tuple: bool,
    pub fields: Vec<SealedFieldView>,
}

pub struct SealedFieldView {
    pub name: String,
    pub local_name: String,
    pub index: usize,
    pub kotlin_type: String,
    pub is_tuple: bool,
    pub wire_decode_inline: String,
    pub wire_size_expr: String,
    pub wire_encode: String,
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
        Self::from_enum_with_module(enumeration, &Module::new(""))
    }

    pub fn from_enum_with_module(enumeration: &Enumeration, module: &Module) -> Self {
        let variants = enumeration
            .variants
            .iter()
            .enumerate()
            .map(|(tag, variant)| {
                let is_tuple = variant.fields.iter().any(|f| {
                    f.name.starts_with('_')
                        && f.name.chars().nth(1).map_or(false, |c| c.is_ascii_digit())
                });
                SealedVariantView {
                    name: NamingConvention::class_name(&variant.name),
                    tag,
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
                            let name = if field_is_tuple {
                                format!("value{}", i)
                            } else {
                                NamingConvention::property_name(&field.name)
                            };
                            let encoder =
                                super::wire::encode_type(&field.field_type, &name, module);
                            let local_name = format!("_{}_", field.name.to_lowercase());
                            SealedFieldView {
                                name: name.clone(),
                                local_name,
                                index: i,
                                kotlin_type: TypeMapper::map_type(&field.field_type),
                                is_tuple: field_is_tuple,
                                wire_decode_inline: Self::make_decode_inline(
                                    &field.field_type,
                                    module,
                                ),
                                wire_size_expr: encoder.size_expr,
                                wire_encode: encoder.encode_expr,
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

    fn make_decode_inline(ty: &Type, module: &Module) -> String {
        let codec = super::wire::decode_type(ty, module);
        let reader = codec.reader_expr.replace("OFFSET", "pos");
        match &codec.size_kind {
            super::wire::SizeKind::Fixed(size) => {
                format!("run {{ val v = {}; pos += {}; v }}", reader, size)
            }
            super::wire::SizeKind::Variable => {
                format!("run {{ val (v, s) = {}; pos += s; v }}", reader)
            }
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/record.txt", escape = "none")]
pub struct RecordTemplate {
    pub class_name: String,
    pub fields: Vec<FieldView>,
    pub is_blittable: bool,
    pub struct_size: usize,
}

pub struct FieldView {
    pub name: String,
    pub local_name: String,
    pub kotlin_type: String,
    pub wire_decode_inline: String,
    pub wire_size_expr: String,
    pub wire_encode: String,
    pub offset: usize,
    pub size: usize,
    pub read_expr: String,
    pub padding_after: usize,
}

impl RecordTemplate {
    pub fn from_record(record: &Record) -> Self {
        Self::from_record_with_module(record, &Module::new(""))
    }

    pub fn from_record_with_module(record: &Record, module: &Module) -> Self {
        let is_blittable = record.is_blittable();
        let layout = record.layout();
        let struct_size = layout.total_size().as_usize();
        let offsets = layout.offsets().map(|offset| offset.as_usize()).collect::<Vec<_>>();

        let fields = record
            .fields
            .iter()
            .zip(offsets.iter().copied())
            .enumerate()
            .map(|(index, (field, offset))| {
                let next_start = offsets.get(index + 1).copied().unwrap_or(struct_size);
                let mut view = Self::make_field(field, module, offset);
                let field_end = view.offset + view.size;
                view.padding_after = next_start.saturating_sub(field_end);
                view
            })
            .collect();

        Self {
            class_name: NamingConvention::class_name(&record.name),
            fields,
            is_blittable,
            struct_size,
        }
    }

    fn make_field(field: &RecordField, module: &Module, offset: usize) -> FieldView {
        let name = NamingConvention::property_name(&field.name);
        let local_name = format!("_{}_", field.name.to_lowercase());
        let encoder = super::wire::encode_type(&field.field_type, &name, module);
        let size = match &field.field_type {
            Type::Primitive(p) => p.size_bytes(),
            _ => 0,
        };
        let read_expr = Self::make_blittable_read(&field.field_type, offset);

        FieldView {
            name: name.clone(),
            local_name,
            kotlin_type: TypeMapper::map_type(&field.field_type),
            wire_decode_inline: Self::make_decode_inline(&field.field_type, module),
            wire_size_expr: encoder.size_expr,
            wire_encode: encoder.encode_expr,
            offset,
            size,
            read_expr,
            padding_after: 0,
        }
    }

    fn make_blittable_read(ty: &Type, offset: usize) -> String {
        match ty {
            Type::Primitive(p) => {
                let read_fn = match p {
                    Primitive::Bool => "readBool",
                    Primitive::I8 => "readI8",
                    Primitive::U8 => "readU8",
                    Primitive::I16 => "readI16",
                    Primitive::U16 => "readU16",
                    Primitive::I32 => "readI32",
                    Primitive::U32 => "readU32",
                    Primitive::I64 | Primitive::Isize => "readI64",
                    Primitive::U64 | Primitive::Usize => "readU64",
                    Primitive::F32 => "readF32",
                    Primitive::F64 => "readF64",
                };
                format!("wire.{}(offset + {})", read_fn, offset)
            }
            _ => String::new(),
        }
    }

    fn make_decode_inline(ty: &Type, module: &Module) -> String {
        let codec = super::wire::decode_type(ty, module);
        let reader = codec.reader_expr.replace("OFFSET", "pos");
        match &codec.size_kind {
            super::wire::SizeKind::Fixed(size) => {
                format!("run {{ val v = {}; pos += {}; v }}", reader, size)
            }
            super::wire::SizeKind::Variable => {
                format!("run {{ val (v, s) = {}; pos += s; v }}", reader)
            }
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
        let layout = record.layout();
        let offsets: Vec<_> = layout.offsets().map(|o| o.as_usize()).collect();
        let struct_size = layout.total_size().as_usize();

        let fields: Vec<ReaderFieldView> = record
            .fields
            .iter()
            .zip(offsets.iter().copied())
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
            struct_size,
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

pub struct ParamView {
    pub name: String,
    pub kotlin_type: String,
    pub conversion: String,
}

pub struct SignatureParamView {
    pub name: String,
    pub kotlin_type: String,
}

pub struct WireWriterView {
    pub binding_name: String,
    pub size_expr: String,
    pub encode_expr: String,
}

#[derive(Template)]
#[template(path = "kotlin/function_wire.txt", escape = "none")]
pub struct WireFunctionTemplate {
    pub func_name: String,
    pub ffi_name: String,
    pub signature_params: Vec<SignatureParamView>,
    pub native_args: Vec<String>,
    pub wire_writers: Vec<WireWriterView>,
    pub wire_writer_closes: Vec<String>,
    pub return_type: Option<String>,
    pub return_abi: ReturnAbi,
    pub decode_expr: String,
    pub throws: bool,
    pub err_type: String,
    pub is_blittable_return: bool,
}

impl WireFunctionTemplate {
    pub fn from_function(function: &Function, module: &Module) -> Self {
        let plan =
            WireFunctionPlan::for_function(&function.name, &function.inputs, &function.returns, module);
        let signature_params = plan
            .signature_params
            .into_iter()
            .map(|param| SignatureParamView {
                name: param.name,
                kotlin_type: param.kotlin_type,
            })
            .collect();

        let wire_writers = plan
            .wire_writers
            .into_iter()
            .map(|binding| WireWriterView {
                binding_name: binding.binding_name,
                size_expr: binding.size_expr,
                encode_expr: binding.encode_expr,
            })
            .collect();

        Self {
            func_name: plan.func_name,
            ffi_name: plan.ffi_name,
            signature_params,
            native_args: plan.native_args,
            wire_writers,
            wire_writer_closes: plan.wire_writer_closes,
            return_type: plan.return_type,
            return_abi: plan.return_abi,
            decode_expr: plan.decode_expr,
            throws: plan.throws,
            err_type: plan.err_type,
            is_blittable_return: plan.is_blittable_return,
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
    pub signature_params: Vec<SignatureParamView>,
    pub native_args: Vec<String>,
    pub wire_writers: Vec<WireWriterView>,
    pub wire_writer_closes: Vec<String>,
    pub return_type: Option<String>,
    pub return_abi: ReturnAbi,
    pub decode_expr: String,
    pub throws: bool,
    pub err_type: String,
    pub is_blittable_return: bool,
}

impl AsyncFunctionTemplate {
    pub fn from_function(function: &Function, module: &Module) -> Self {
        let plan = AsyncCallPlan::for_function(&function.name, &function.inputs, &function.returns, module);
        let signature_params = plan
            .signature_params
            .into_iter()
            .map(|param| SignatureParamView {
                name: param.name,
                kotlin_type: param.kotlin_type,
            })
            .collect();

        let wire_writers = plan
            .wire_writers
            .into_iter()
            .map(|binding| WireWriterView {
                binding_name: binding.binding_name,
                size_expr: binding.size_expr,
                encode_expr: binding.encode_expr,
            })
            .collect();

        Self {
            func_name: plan.func_name,
            ffi_name: plan.ffi_name,
            ffi_poll: plan.ffi_poll,
            ffi_complete: plan.ffi_complete,
            ffi_free: plan.ffi_free,
            ffi_cancel: plan.ffi_cancel,
            signature_params,
            native_args: plan.native_args,
            wire_writers,
            wire_writer_closes: plan.wire_writer_closes,
            return_type: plan.return_type,
            return_abi: plan.return_abi,
            decode_expr: plan.decode_expr,
            throws: plan.throws,
            err_type: plan.err_type,
            is_blittable_return: plan.is_blittable_return,
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
    pub impl_: MethodImpl,
}

pub enum MethodImpl {
    AsyncMethod(AsyncMethodTemplate),
    SyncMethod(WireMethodTemplate),
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
                                module,
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
                let impl_ = if method.is_async {
                    MethodImpl::AsyncMethod(AsyncMethodTemplate::from_method(class, method, module))
                } else {
                    MethodImpl::SyncMethod(WireMethodTemplate::from_method(class, method, module))
                };
                MethodView {
                    name: NamingConvention::method_name(&method.name),
                    impl_,
                }
            })
            .collect();

        let has_factory_ctors = if use_companion_methods {
            constructors.iter().any(|c| c.is_factory)
        } else {
            constructors
                .iter()
                .any(|c| c.is_factory && c.params.is_empty())
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
        if method.is_async {
            AsyncCallPlan::supports_call(&method.inputs, &method.returns, module)
        } else {
            WireFunctionPlan::supports_call(&method.inputs, &method.returns, module)
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/method_wire.txt", escape = "none")]
pub struct WireMethodTemplate {
    pub method_name: String,
    pub ffi_name: String,
    pub signature_params: Vec<SignatureParamView>,
    pub native_args: Vec<String>,
    pub wire_writers: Vec<WireWriterView>,
    pub wire_writer_closes: Vec<String>,
    pub return_type: Option<String>,
    pub return_abi: ReturnAbi,
    pub decode_expr: String,
    pub throws: bool,
    pub err_type: String,
    pub is_blittable_return: bool,
}

impl WireMethodTemplate {
    pub fn from_method(class: &Class, method: &Method, module: &Module) -> Self {
        let plan = WireFunctionPlan::for_function(
            &method.name,
            &method.inputs,
            &method.returns,
            module,
        );

        Self {
            method_name: NamingConvention::method_name(&method.name),
            ffi_name: naming::method_ffi_name(&class.name, &method.name),
            signature_params: plan
                .signature_params
                .into_iter()
                .map(|param| SignatureParamView {
                    name: param.name,
                    kotlin_type: param.kotlin_type,
                })
                .collect(),
            native_args: plan.native_args,
            wire_writers: plan
                .wire_writers
                .into_iter()
                .map(|binding| WireWriterView {
                    binding_name: binding.binding_name,
                    size_expr: binding.size_expr,
                    encode_expr: binding.encode_expr,
                })
                .collect(),
            wire_writer_closes: plan.wire_writer_closes,
            return_type: plan.return_type,
            return_abi: plan.return_abi,
            decode_expr: plan.decode_expr,
            throws: plan.throws,
            err_type: plan.err_type,
            is_blittable_return: plan.is_blittable_return,
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/method_async.txt", escape = "none")]
pub struct AsyncMethodTemplate {
    pub method_name: String,
    pub ffi_name: String,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub signature_params: Vec<SignatureParamView>,
    pub native_args: Vec<String>,
    pub wire_writers: Vec<WireWriterView>,
    pub wire_writer_closes: Vec<String>,
    pub return_type: Option<String>,
    pub return_abi: ReturnAbi,
    pub decode_expr: String,
    pub throws: bool,
    pub err_type: String,
    pub is_blittable_return: bool,
}

impl AsyncMethodTemplate {
    pub fn from_method(class: &Class, method: &Method, module: &Module) -> Self {
        let plan = AsyncCallPlan::for_method(class, method, module);
        Self {
            method_name: NamingConvention::method_name(&method.name),
            ffi_name: plan.ffi_name,
            ffi_poll: plan.ffi_poll,
            ffi_complete: plan.ffi_complete,
            ffi_cancel: plan.ffi_cancel,
            ffi_free: plan.ffi_free,
            signature_params: plan
                .signature_params
                .into_iter()
                .map(|param| SignatureParamView {
                    name: param.name,
                    kotlin_type: param.kotlin_type,
                })
                .collect(),
            native_args: plan.native_args,
            wire_writers: plan
                .wire_writers
                .into_iter()
                .map(|binding| WireWriterView {
                    binding_name: binding.binding_name,
                    size_expr: binding.size_expr,
                    encode_expr: binding.encode_expr,
                })
                .collect(),
            wire_writer_closes: plan.wire_writer_closes,
            return_type: plan.return_type,
            return_abi: plan.return_abi,
            decode_expr: plan.decode_expr,
            throws: plan.throws,
            err_type: plan.err_type,
            is_blittable_return: plan.is_blittable_return,
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/native.txt", escape = "none")]
pub struct NativeTemplate {
    pub lib_name: String,
    pub prefix: String,
    pub functions: Vec<NativeFunctionView>,
    pub wire_functions: Vec<NativeWireFunctionView>,
    pub classes: Vec<NativeClassView>,
    pub async_callback_invokers: Vec<AsyncCallbackInvokerView>,
}

pub struct NativeWireFunctionView {
    pub ffi_name: String,
    pub params: Vec<NativeParamView>,
    pub return_jni_type: String,
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
    pub sync_methods: Vec<NativeSyncMethodView>,
    pub async_methods: Vec<NativeAsyncMethodView>,
}

pub struct NativeFactoryCtorView {
    pub ffi_name: String,
    pub params: Vec<NativeParamView>,
}

pub struct NativeSyncMethodView {
    pub ffi_name: String,
    pub params: Vec<NativeParamView>,
    pub return_jni_type: String,
}

pub struct NativeAsyncMethodView {
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
            .filter(|func| {
                (func.is_async && AsyncCallPlan::supports_call(&func.inputs, &func.returns, module))
                    || (!func.is_async && Self::is_primitive_return(func))
            })
            .map(|func| {
                let ffi_name = naming::function_ffi_name(&func.name);
                let (has_out_param, out_type, return_jni_type) =
                    Self::analyze_return(&func.returns, module);
                let return_abi = ReturnAbi::from_return_type(&func.returns, module);
                let complete_return_jni_type = if func.is_async {
                    if return_abi.is_wire_encoded() {
                        "ByteBuffer?".to_string()
                    } else {
                        return_abi
                            .kotlin_type()
                            .unwrap_or("Unit")
                            .to_string()
                    }
                } else {
                    String::new()
                };

                NativeFunctionView {
                    ffi_name: ffi_name.clone(),
                    params: func
                        .inputs
                        .iter()
                        .map(|p| NativeParamView {
                            name: NamingConvention::param_name(&p.name),
                            jni_type: WireFunctionPlan::jni_param_type_for_wire_param(&p.param_type),
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
                    complete_return_jni_type,
                }
            })
            .collect();

        let wire_functions: Vec<NativeWireFunctionView> = module
            .functions
            .iter()
            .filter(|func| !func.is_async && !Self::is_primitive_return(func))
            .map(|func| {
                let return_abi = ReturnAbi::from_return_type(&func.returns, module);
                NativeWireFunctionView {
                    ffi_name: naming::function_ffi_name(&func.name),
                    params: func
                        .inputs
                        .iter()
                        .map(|p| NativeParamView {
                            name: NamingConvention::param_name(&p.name),
                            jni_type: WireFunctionPlan::jni_param_type_for_wire_param(&p.param_type),
                        })
                        .collect(),
                    return_jni_type: Self::wire_return_jni_type(&return_abi),
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
                    .filter(|c| {
                        c.inputs
                            .iter()
                            .all(|p| matches!(&p.param_type, Type::Primitive(_)))
                    })
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

                let methods: Vec<NativeAsyncMethodView> = class
                    .methods
                    .iter()
                    .filter(|method| method.is_async)
                    .filter(|method| AsyncCallPlan::supports_call(&method.inputs, &method.returns, module))
                    .map(|method| {
                        let method_ffi = naming::method_ffi_name(&class.name, &method.name);
                        let (has_out_param, out_type, _return_jni_type) =
                            Self::analyze_return(&method.returns, module);
                        let return_abi = ReturnAbi::from_return_type(&method.returns, module);
                        let complete_return_jni_type = if return_abi.is_wire_encoded() {
                            "ByteBuffer?".to_string()
                        } else {
                            return_abi.kotlin_type().unwrap_or("Unit").to_string()
                        };

                        NativeAsyncMethodView {
                            ffi_name: method_ffi.clone(),
                            params: method
                                .inputs
                                .iter()
                                .map(|p| NativeParamView {
                                    name: NamingConvention::param_name(&p.name),
                                    jni_type: WireFunctionPlan::jni_param_type_for_wire_param(
                                        &p.param_type,
                                    ),
                                })
                                .collect(),
                            has_out_param,
                            out_type,
                            return_jni_type: complete_return_jni_type,
                            is_async: method.is_async,
                            ffi_poll: naming::method_ffi_poll(&class.name, &method.name),
                            ffi_complete: naming::method_ffi_complete(&class.name, &method.name),
                            ffi_cancel: naming::method_ffi_cancel(&class.name, &method.name),
                            ffi_free: naming::method_ffi_free(&class.name, &method.name),
                        }
                    })
                    .collect();

                let sync_methods: Vec<NativeSyncMethodView> = class
                    .methods
                    .iter()
                    .filter(|method| !method.is_async)
                    .filter(|method| WireFunctionPlan::supports_call(&method.inputs, &method.returns, module))
                    .map(|method| {
                        let return_abi = ReturnAbi::from_return_type(&method.returns, module);
                        NativeSyncMethodView {
                            ffi_name: naming::method_ffi_name(&class.name, &method.name),
                            params: method
                                .inputs
                                .iter()
                                .map(|param| NativeParamView {
                                    name: NamingConvention::param_name(&param.name),
                                    jni_type: WireFunctionPlan::jni_param_type_for_wire_param(
                                        &param.param_type,
                                    ),
                                })
                                .collect(),
                            return_jni_type: Self::wire_return_jni_type(&return_abi),
                        }
                    })
                    .collect();

                NativeClassView {
                    ffi_new: format!("{}_new", ffi_prefix),
                    ffi_free: format!("{}_free", ffi_prefix),
                    ctor_params,
                    factory_ctors,
                    sync_methods,
                    async_methods: methods,
                }
            })
            .collect();

        let async_callback_invokers = Self::collect_async_callback_invokers(module);

        Self {
            lib_name: format!("{}_jni", module.name),
            prefix,
            functions,
            wire_functions,
            classes,
            async_callback_invokers,
        }
    }

    fn is_primitive_return(func: &Function) -> bool {
        super::is_primitive_only(func)
    }

    fn wire_return_jni_type(return_abi: &ReturnAbi) -> String {
        match return_abi {
            ReturnAbi::Unit => "Unit".into(),
            ReturnAbi::Direct { kotlin_type } => match kotlin_type.as_str() {
                "Boolean" => "Boolean".into(),
                "Byte" | "UByte" => "Byte".into(),
                "Short" | "UShort" => "Short".into(),
                "Int" | "UInt" => "Int".into(),
                "Long" | "ULong" => "Long".into(),
                "Float" => "Float".into(),
                "Double" => "Double".into(),
                _ => "Long".into(),
            },
            ReturnAbi::WireEncoded { .. } => "ByteBuffer?".into(),
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

    fn build_invoker_view(suffix: &str, _returns: &ReturnType) -> AsyncCallbackInvokerView {
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

        let supported_params = method
            .inputs
            .iter()
            .all(|param| matches!(&param.param_type, Type::Primitive(_)));

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Parameter;

    fn make_function(name: &str, inputs: Vec<Parameter>, returns: ReturnType) -> Function {
        Function {
            name: name.into(),
            inputs,
            returns,
            is_async: false,
            wire_encoded: true,
            doc: None,
            deprecated: None,
        }
    }

    fn make_record(name: &str, fields: Vec<RecordField>) -> Record {
        Record {
            name: name.into(),
            fields,
            doc: None,
            deprecated: None,
        }
    }

    #[test]
    fn test_wire_function_unit_return() {
        let module = Module::new("test");
        let func = make_function("do_something", vec![], ReturnType::Void);

        let template = WireFunctionTemplate::from_function(&func, &module);
        let output = template.render().unwrap();

        assert!(output.contains("fun doSomething()"));
        assert!(output.contains("Native.riff_do_something()"));
        assert!(!output.contains("return"));
    }

    #[test]
    fn test_wire_function_primitive_return_with_complex_params() {
        let mut module = Module::new("test");
        module.records.push(Record {
            name: "Item".into(),
            fields: vec![],
            doc: None,
            deprecated: None,
        });
        let func = make_function(
            "count_items",
            vec![Parameter {
                name: "items".into(),
                param_type: Type::Vec(Box::new(Type::Record("Item".into()))),
            }],
            ReturnType::Value(Type::Primitive(Primitive::I32)),
        );

        let template = WireFunctionTemplate::from_function(&func, &module);
        let output = template.render().unwrap();

        assert!(output.contains("fun countItems(items: List<Item>): Int"));
        assert!(output.contains("return Native.riff_count_items"));
    }

    #[test]
    fn test_wire_function_string_return() {
        let module = Module::new("test");
        let func = make_function("get_name", vec![], ReturnType::Value(Type::String));

        let template = WireFunctionTemplate::from_function(&func, &module);
        let output = template.render().unwrap();

        assert!(output.contains("fun getName(): String"));
        assert!(output.contains("WireBuffer"));
        assert!(output.contains("wire.readString(0).first"));
    }

    #[test]
    fn test_wire_function_fallible_return() {
        let module = Module::new("test");
        let func = make_function(
            "try_something",
            vec![],
            ReturnType::Fallible {
                ok: Type::String,
                err: Type::String,
            },
        );

        let template = WireFunctionTemplate::from_function(&func, &module);
        let output = template.render().unwrap();

        assert!(output.contains("@Throws(FfiException::class)"));
        assert!(output.contains("fun trySomething(): String"));
        assert!(output.contains("readResult"));
        assert!(output.contains("getOrThrow()"));
    }

    #[test]
    fn test_wire_function_with_params() {
        let module = Module::new("test");
        let func = make_function(
            "add",
            vec![
                Parameter::new("a", Type::Primitive(Primitive::I32)),
                Parameter::new("b", Type::Primitive(Primitive::I32)),
            ],
            ReturnType::Value(Type::Primitive(Primitive::I32)),
        );

        let template = WireFunctionTemplate::from_function(&func, &module);
        let output = template.render().unwrap();

        assert!(output.contains("fun add(a: Int, b: Int): Int"));
        assert!(output.contains("Native.riff_add(a, b)"));
    }

    #[test]
    fn test_wire_function_vec_return() {
        let module = Module::new("test");
        let func = make_function(
            "get_items",
            vec![],
            ReturnType::Value(Type::Vec(Box::new(Type::Primitive(Primitive::I32)))),
        );

        let template = WireFunctionTemplate::from_function(&func, &module);
        let output = template.render().unwrap();

        assert!(output.contains("fun getItems(): IntArray"));
        assert!(output.contains("WireBuffer"));
    }

    #[test]
    fn test_wire_function_record_return() {
        let mut module = Module::new("test");
        module.records.push(make_record(
            "Point",
            vec![
                RecordField::new("x", Type::Primitive(Primitive::I32)),
                RecordField::new("y", Type::Primitive(Primitive::I32)),
            ],
        ));

        let func = make_function(
            "get_point",
            vec![],
            ReturnType::Value(Type::Record("Point".into())),
        );

        let template = WireFunctionTemplate::from_function(&func, &module);
        let output = template.render().unwrap();

        assert!(output.contains("fun getPoint(): Point"));
        assert!(output.contains("PointReader.read(buffer, 0)"));
        assert!(!output.contains("WireBuffer"));
    }
}
