use askama::Template;

use crate::model::{
    CallbackTrait, Class, Enumeration, Method, Module, Record, StreamMethod, StreamMode,
};

use super::body::BodyRenderer;
use super::names::NamingConvention;
use super::types::TypeMapper;

#[derive(Template)]
#[template(path = "swift/record.txt", escape = "none")]
pub struct RecordTemplate {
    pub class_name: String,
    pub fields: Vec<FieldView>,
    pub has_aliases: bool,
}

impl RecordTemplate {
    pub fn from_record(record: &Record) -> Self {
        let fields: Vec<FieldView> = record
            .fields
            .iter()
            .map(|field| {
                let swift_name = NamingConvention::property_name(&field.name);
                let c_name = field.name.clone();
                FieldView {
                    needs_alias: swift_name != c_name,
                    swift_name,
                    c_name,
                    swift_type: TypeMapper::map_type(&field.field_type),
                }
            })
            .collect();
        let has_aliases = fields.iter().any(|f| f.needs_alias);
        Self {
            class_name: NamingConvention::class_name(&record.name),
            fields,
            has_aliases,
        }
    }
}

#[derive(Template)]
#[template(path = "swift/enum_c_style.txt", escape = "none")]
pub struct CStyleEnumTemplate {
    pub class_name: String,
    pub variants: Vec<CStyleVariantView>,
}

impl CStyleEnumTemplate {
    pub fn from_enum(enumeration: &Enumeration) -> Self {
        Self {
            class_name: NamingConvention::class_name(&enumeration.name),
            variants: enumeration
                .variants
                .iter()
                .enumerate()
                .map(|(index, variant)| CStyleVariantView {
                    swift_name: NamingConvention::enum_case_name(&variant.name),
                    discriminant: variant.discriminant.unwrap_or(index as i64),
                })
                .collect(),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/enum_data.txt", escape = "none")]
pub struct DataEnumTemplate {
    pub class_name: String,
    pub variants: Vec<DataVariantView>,
}

impl DataEnumTemplate {
    pub fn from_enum(enumeration: &Enumeration) -> Self {
        Self {
            class_name: NamingConvention::class_name(&enumeration.name),
            variants: enumeration
                .variants
                .iter()
                .map(|variant| DataVariantView {
                    swift_name: NamingConvention::enum_case_name(&variant.name),
                    fields: variant
                        .fields
                        .iter()
                        .map(|field| {
                            let swift_name = NamingConvention::param_name(&field.name);
                            let c_name = field.name.clone();
                            FieldView {
                                needs_alias: swift_name != c_name,
                                swift_name,
                                c_name,
                                swift_type: TypeMapper::map_type(&field.field_type),
                            }
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/class.txt", escape = "none")]
pub struct ClassTemplate {
    pub class_name: String,
    pub doc: Option<String>,
    pub deprecated: bool,
    pub deprecated_message: Option<String>,
    pub ffi_free: String,
    pub constructors: Vec<ConstructorView>,
    pub methods: Vec<MethodView>,
    pub streams: Vec<StreamView>,
}

impl ClassTemplate {
    pub fn from_class(class: &Class, module: &Module) -> Self {
        let class_prefix = class.ffi_prefix(&module.ffi_prefix());

        Self {
            class_name: NamingConvention::class_name(&class.name),
            doc: class.doc.clone(),
            deprecated: class.deprecated.is_some(),
            deprecated_message: class.deprecated.as_ref().and_then(|d| d.message.clone()),
            ffi_free: class.ffi_free(&module.ffi_prefix()),
            constructors: class
                .constructors
                .iter()
                .map(|ctor| ConstructorView {
                    doc: ctor.doc.clone(),
                    ffi_name: ctor.ffi_name(&class_prefix),
                    is_failable: false,
                    params: ctor
                        .inputs
                        .iter()
                        .map(|param| ParamView {
                            swift_name: NamingConvention::param_name(&param.name),
                            swift_type: TypeMapper::map_type(&param.param_type),
                            is_escaping: matches!(param.param_type, crate::model::Type::Callback(_)),
                        })
                        .collect(),
                })
                .collect(),
            methods: class
                .methods
                .iter()
                .map(|method| MethodView {
                    doc: method.doc.clone(),
                    deprecated: method.deprecated.is_some(),
                    deprecated_message: method.deprecated.as_ref().and_then(|d| d.message.clone()),
                    swift_name: NamingConvention::method_name(&method.name),
                    is_static: method.is_static(),
                    is_async: method.is_async,
                    throws: method.throws(),
                    return_type: method
                        .output
                        .as_ref()
                        .filter(|ty| !ty.is_void())
                        .map(TypeMapper::map_type),
                    params: method
                        .inputs
                        .iter()
                        .map(|param| ParamView {
                            swift_name: NamingConvention::param_name(&param.name),
                            swift_type: TypeMapper::map_type(&param.param_type),
                            is_escaping: matches!(param.param_type, crate::model::Type::Callback(_)),
                        })
                        .collect(),
                    body: BodyRenderer::method(method, class, module),
                })
                .collect(),
            streams: class
                .streams
                .iter()
                .map(|stream| StreamView {
                    doc: stream.doc.clone(),
                    swift_name: NamingConvention::method_name(&stream.name),
                    swift_name_pascal: NamingConvention::class_name(&stream.name),
                    item_type: TypeMapper::map_type(&stream.item_type),
                    mode: match stream.mode {
                        StreamMode::Async => StreamModeView::Async,
                        StreamMode::Batch => StreamModeView::Batch,
                        StreamMode::Callback => StreamModeView::Callback,
                    },
                    body: BodyRenderer::stream(stream, class, module),
                })
                .collect(),
        }
    }
}

pub struct FieldView {
    pub swift_name: String,
    pub c_name: String,
    pub swift_type: String,
    pub needs_alias: bool,
}

pub struct CStyleVariantView {
    pub swift_name: String,
    pub discriminant: i64,
}

pub struct DataVariantView {
    pub swift_name: String,
    pub fields: Vec<FieldView>,
}

pub struct ParamView {
    pub swift_name: String,
    pub swift_type: String,
    pub is_escaping: bool,
}

pub struct ConstructorView {
    pub doc: Option<String>,
    pub ffi_name: String,
    pub is_failable: bool,
    pub params: Vec<ParamView>,
}

pub struct MethodView {
    pub doc: Option<String>,
    pub deprecated: bool,
    pub deprecated_message: Option<String>,
    pub swift_name: String,
    pub is_static: bool,
    pub is_async: bool,
    pub throws: bool,
    pub return_type: Option<String>,
    pub params: Vec<ParamView>,
    pub body: String,
}

pub struct StreamView {
    pub doc: Option<String>,
    pub swift_name: String,
    pub swift_name_pascal: String,
    pub item_type: String,
    pub mode: StreamModeView,
    pub body: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum StreamModeView {
    Async,
    Batch,
    Callback,
}

#[derive(Template)]
#[template(path = "swift/stream_async.txt", escape = "none")]
pub struct StreamAsyncBodyTemplate {
    pub item_type: String,
    pub subscribe_fn: String,
    pub pop_batch_fn: String,
    pub poll_fn: String,
    pub unsubscribe_fn: String,
    pub free_fn: String,
}

impl StreamAsyncBodyTemplate {
    pub fn from_stream(stream: &StreamMethod, class: &Class, module: &Module) -> Self {
        let class_prefix = class.ffi_prefix(&module.ffi_prefix());
        Self {
            item_type: TypeMapper::map_type(&stream.item_type),
            subscribe_fn: stream.ffi_subscribe(&class_prefix),
            pop_batch_fn: stream.ffi_pop_batch(&class_prefix),
            poll_fn: stream.ffi_poll(&class_prefix),
            unsubscribe_fn: stream.ffi_unsubscribe(&class_prefix),
            free_fn: stream.ffi_free(&class_prefix),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/stream_batch.txt", escape = "none")]
pub struct StreamBatchBodyTemplate {
    pub class_name: String,
    pub method_name_pascal: String,
    pub subscribe_fn: String,
}

impl StreamBatchBodyTemplate {
    pub fn from_stream(stream: &StreamMethod, class: &Class, module: &Module) -> Self {
        let class_prefix = class.ffi_prefix(&module.ffi_prefix());
        Self {
            class_name: NamingConvention::class_name(&class.name),
            method_name_pascal: NamingConvention::class_name(&stream.name),
            subscribe_fn: stream.ffi_subscribe(&class_prefix),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/stream_callback.txt", escape = "none")]
pub struct StreamCallbackBodyTemplate {
    pub item_type: String,
    pub class_name: String,
    pub method_name_pascal: String,
    pub subscribe_fn: String,
    pub pop_batch_fn: String,
    pub poll_fn: String,
    pub unsubscribe_fn: String,
    pub free_fn: String,
}

impl StreamCallbackBodyTemplate {
    pub fn from_stream(stream: &StreamMethod, class: &Class, module: &Module) -> Self {
        let class_prefix = class.ffi_prefix(&module.ffi_prefix());
        Self {
            item_type: TypeMapper::map_type(&stream.item_type),
            class_name: NamingConvention::class_name(&class.name),
            method_name_pascal: NamingConvention::class_name(&stream.name),
            subscribe_fn: stream.ffi_subscribe(&class_prefix),
            pop_batch_fn: stream.ffi_pop_batch(&class_prefix),
            poll_fn: stream.ffi_poll(&class_prefix),
            unsubscribe_fn: stream.ffi_unsubscribe(&class_prefix),
            free_fn: stream.ffi_free(&class_prefix),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/method_sync.txt", escape = "none")]
pub struct SyncMethodBodyTemplate {
    pub ffi_name: String,
    pub args: Vec<String>,
    pub has_return: bool,
}

impl SyncMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let class_prefix = class.ffi_prefix(&module.ffi_prefix());
        Self {
            ffi_name: method.ffi_name(&class_prefix),
            args: method
                .non_callback_params()
                .map(|p| NamingConvention::param_name(&p.name))
                .collect(),
            has_return: method.output.as_ref().map_or(false, |t| !t.is_void()),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/method_callback.txt", escape = "none")]
pub struct CallbackMethodBodyTemplate {
    pub ffi_name: String,
    pub args: Vec<String>,
    pub has_return: bool,
    pub callbacks: Vec<CallbackView>,
}

pub struct CallbackView {
    pub param_name: String,
    pub swift_type: String,
    pub ffi_arg_type: String,
    pub context_type: String,
    pub box_type: String,
    pub box_name: String,
    pub ptr_name: String,
    pub trampoline_name: String,
}

impl CallbackMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let class_prefix = class.ffi_prefix(&module.ffi_prefix());
        let method_name_pascal = NamingConvention::class_name(&method.name);

        Self {
            ffi_name: method.ffi_name(&class_prefix),
            args: method
                .non_callback_params()
                .map(|p| NamingConvention::param_name(&p.name))
                .collect(),
            has_return: method.output.as_ref().map_or(false, |t| !t.is_void()),
            callbacks: method
                .callback_params()
                .enumerate()
                .map(|(idx, param)| {
                    let param_name = NamingConvention::param_name(&param.name);
                    let inner_type = match &param.param_type {
                        crate::model::Type::Callback(inner) => TypeMapper::map_type(inner),
                        _ => "Void".into(),
                    };
                    let ffi_inner = match &param.param_type {
                        crate::model::Type::Callback(inner) => TypeMapper::ffi_type(inner),
                        _ => "Void".into(),
                    };
                    let suffix = if idx > 0 { format!("{}", idx + 1) } else { String::new() };

                    CallbackView {
                        param_name: param_name.clone(),
                        swift_type: inner_type.clone(),
                        ffi_arg_type: ffi_inner,
                        context_type: format!("{}{}CallbackFn{}", method_name_pascal, suffix, ""),
                        box_type: format!("{}{}CallbackBox{}", method_name_pascal, suffix, ""),
                        box_name: format!("{}Box{}", param_name, suffix),
                        ptr_name: format!("{}Ptr{}", param_name, suffix),
                        trampoline_name: format!("{}Trampoline{}", param_name, suffix),
                    }
                })
                .collect(),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/method_throwing.txt", escape = "none")]
pub struct ThrowingMethodBodyTemplate {
    pub ffi_name: String,
    pub args: Vec<String>,
    pub return_type: String,
}

impl ThrowingMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let class_prefix = class.ffi_prefix(&module.ffi_prefix());
        Self {
            ffi_name: method.ffi_name(&class_prefix),
            args: method
                .inputs
                .iter()
                .map(|p| NamingConvention::param_name(&p.name))
                .collect(),
            return_type: method
                .output
                .as_ref()
                .map(TypeMapper::map_type)
                .unwrap_or_else(|| "Void".into()),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/method_async.txt", escape = "none")]
pub struct AsyncMethodBodyTemplate {
    pub ffi_name: String,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub args: Vec<String>,
    pub return_type: String,
}

impl AsyncMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let class_prefix = class.ffi_prefix(&module.ffi_prefix());
        Self {
            ffi_name: method.ffi_name(&class_prefix),
            ffi_poll: method.ffi_poll(&class_prefix),
            ffi_complete: method.ffi_complete(&class_prefix),
            ffi_cancel: method.ffi_cancel(&class_prefix),
            ffi_free: method.ffi_free(&class_prefix),
            args: method
                .inputs
                .iter()
                .map(|p| NamingConvention::param_name(&p.name))
                .collect(),
            return_type: method
                .output
                .as_ref()
                .map(TypeMapper::map_type)
                .unwrap_or_else(|| "Void".into()),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/method_async_throwing.txt", escape = "none")]
pub struct AsyncThrowingMethodBodyTemplate {
    pub ffi_name: String,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub args: Vec<String>,
    pub return_type: String,
}

impl AsyncThrowingMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let class_prefix = class.ffi_prefix(&module.ffi_prefix());
        Self {
            ffi_name: method.ffi_name(&class_prefix),
            ffi_poll: method.ffi_poll(&class_prefix),
            ffi_complete: method.ffi_complete(&class_prefix),
            ffi_cancel: method.ffi_cancel(&class_prefix),
            ffi_free: method.ffi_free(&class_prefix),
            args: method
                .inputs
                .iter()
                .map(|p| NamingConvention::param_name(&p.name))
                .collect(),
            return_type: method
                .output
                .as_ref()
                .map(TypeMapper::map_type)
                .unwrap_or_else(|| "Void".into()),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/stream_subscription.txt", escape = "none")]
pub struct StreamSubscriptionTemplate {
    pub class_name: String,
    pub method_name_pascal: String,
    pub item_type: String,
    pub pop_batch_fn: String,
    pub wait_fn: String,
    pub unsubscribe_fn: String,
    pub free_fn: String,
}

impl StreamSubscriptionTemplate {
    pub fn from_stream(stream: &StreamMethod, class: &Class, module: &Module) -> Self {
        let class_prefix = class.ffi_prefix(&module.ffi_prefix());
        Self {
            class_name: NamingConvention::class_name(&class.name),
            method_name_pascal: NamingConvention::class_name(&stream.name),
            item_type: TypeMapper::map_type(&stream.item_type),
            pop_batch_fn: stream.ffi_pop_batch(&class_prefix),
            wait_fn: stream.ffi_wait(&class_prefix),
            unsubscribe_fn: stream.ffi_unsubscribe(&class_prefix),
            free_fn: stream.ffi_free(&class_prefix),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/stream_cancellable.txt", escape = "none")]
pub struct StreamCancellableTemplate {
    pub class_name: String,
    pub method_name_pascal: String,
}

impl StreamCancellableTemplate {
    pub fn from_stream(stream: &StreamMethod, class: &Class, _module: &Module) -> Self {
        Self {
            class_name: NamingConvention::class_name(&class.name),
            method_name_pascal: NamingConvention::class_name(&stream.name),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/callback_trait.txt", escape = "none")]
pub struct CallbackTraitTemplate {
    pub doc: Option<String>,
    pub protocol_name: String,
    pub wrapper_class: String,
    pub vtable_var: String,
    pub vtable_type: String,
    pub bridge_name: String,
    pub register_fn: String,
    pub create_fn: String,
    pub methods: Vec<TraitMethodView>,
}

pub struct TraitMethodView {
    pub swift_name: String,
    pub ffi_name: String,
    pub params: Vec<TraitParamView>,
    pub return_type: Option<String>,
    pub is_async: bool,
    pub throws: bool,
    pub has_return: bool,
    pub has_out_param: bool,
}

pub struct TraitParamView {
    pub label: String,
    pub ffi_name: String,
    pub swift_type: String,
    pub conversion: String,
}

impl CallbackTraitTemplate {
    pub fn from_trait(callback_trait: &CallbackTrait, module: &Module) -> Self {
        let prefix = module.ffi_prefix();
        let trait_name = &callback_trait.name;

        Self {
            doc: callback_trait.doc.clone(),
            protocol_name: format!("{}Protocol", trait_name),
            wrapper_class: format!("{}Wrapper", trait_name),
            vtable_var: format!("{}VTableInstance", to_camel_case(trait_name)),
            vtable_type: callback_trait.ffi_vtable_name(),
            bridge_name: format!("{}Bridge", trait_name),
            register_fn: callback_trait.ffi_register_fn(&prefix),
            create_fn: callback_trait.ffi_create_fn(&prefix),
            methods: callback_trait
                .methods
                .iter()
                .map(|method| {
                    let has_return = method.has_return();
                    TraitMethodView {
                        swift_name: NamingConvention::method_name(&method.name),
                        ffi_name: to_snake_case(&method.name),
                        params: method
                            .inputs
                            .iter()
                            .map(|param| {
                                let swift_name = NamingConvention::param_name(&param.name);
                                TraitParamView {
                                    label: swift_name.clone(),
                                    ffi_name: param.name.clone(),
                                    swift_type: TypeMapper::map_type(&param.param_type),
                                    conversion: swift_name,
                                }
                            })
                            .collect(),
                        return_type: method.output.as_ref().map(TypeMapper::map_type),
                        is_async: method.is_async,
                        throws: method.throws(),
                        has_return,
                        has_out_param: has_return && !method.is_async,
                    }
                })
                .collect(),
        }
    }
}

fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

fn to_camel_case(name: &str) -> String {
    let mut result = String::new();
    let mut first = true;
    for ch in name.chars() {
        if first {
            result.push(ch.to_ascii_lowercase());
            first = false;
        } else {
            result.push(ch);
        }
    }
    result
}


