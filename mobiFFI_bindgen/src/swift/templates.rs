use askama::Template;

use crate::model::{Class, Enumeration, Method, Module, Record, StreamMethod};

use super::names::NamingConvention;
use super::types::TypeMapper;

#[derive(Template)]
#[template(path = "swift/record.txt", escape = "none")]
pub struct RecordTemplate {
    pub class_name: String,
    pub fields: Vec<FieldView>,
}

impl RecordTemplate {
    pub fn from_record(record: &Record) -> Self {
        Self {
            class_name: NamingConvention::class_name(&record.name),
            fields: record
                .fields
                .iter()
                .map(|field| FieldView {
                    swift_name: NamingConvention::property_name(&field.name),
                    swift_type: TypeMapper::map_type(&field.field_type),
                })
                .collect(),
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
                        .map(|field| FieldView {
                            swift_name: NamingConvention::param_name(&field.name),
                            swift_type: TypeMapper::map_type(&field.field_type),
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
                        })
                        .collect(),
                    body: render_method_body(method, class, module),
                })
                .collect(),
            streams: class
                .streams
                .iter()
                .map(|stream| StreamView {
                    doc: stream.doc.clone(),
                    swift_name: NamingConvention::method_name(&stream.name),
                    item_type: TypeMapper::map_type(&stream.item_type),
                    body: StreamBodyTemplate::from_stream(stream, class, module)
                        .render()
                        .expect("stream body template failed"),
                })
                .collect(),
        }
    }
}

pub struct FieldView {
    pub swift_name: String,
    pub swift_type: String,
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
    pub item_type: String,
    pub body: String,
}

#[derive(Template)]
#[template(path = "swift/stream_body.txt", escape = "none")]
pub struct StreamBodyTemplate {
    pub item_type: String,
    pub subscribe_fn: String,
    pub pop_batch_fn: String,
    pub wait_fn: String,
    pub free_fn: String,
}

impl StreamBodyTemplate {
    pub fn from_stream(stream: &StreamMethod, class: &Class, module: &Module) -> Self {
        let class_prefix = class.ffi_prefix(&module.ffi_prefix());
        Self {
            item_type: TypeMapper::map_type(&stream.item_type),
            subscribe_fn: stream.ffi_subscribe(&class_prefix),
            pop_batch_fn: stream.ffi_pop_batch(&class_prefix),
            wait_fn: stream.ffi_wait(&class_prefix),
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
                .inputs
                .iter()
                .map(|p| NamingConvention::param_name(&p.name))
                .collect(),
            has_return: method.output.as_ref().map_or(false, |t| !t.is_void()),
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

fn render_method_body(method: &Method, class: &Class, module: &Module) -> String {
    match (method.is_async, method.throws()) {
        (true, true) => AsyncThrowingMethodBodyTemplate::from_method(method, class, module)
            .render()
            .expect("async throwing method template failed"),
        (true, false) => AsyncMethodBodyTemplate::from_method(method, class, module)
            .render()
            .expect("async method template failed"),
        (false, true) => ThrowingMethodBodyTemplate::from_method(method, class, module)
            .render()
            .expect("throwing method template failed"),
        (false, false) => SyncMethodBodyTemplate::from_method(method, class, module)
            .render()
            .expect("sync method template failed"),
    }
}
