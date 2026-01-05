use askama::Template;
use riff_ffi_rules::naming;

use crate::model::{
    CallbackTrait, Class, Enumeration, Function, Method, Module, Record, StreamMethod, StreamMode,
};

use super::body::BodyRenderer;
use super::names::NamingConvention;
use super::types::TypeMapper;

#[derive(Template)]
#[template(path = "swift/preamble.txt", escape = "none")]
pub struct PreambleTemplate {
    pub prefix: String,
    pub ffi_module_name: Option<String>,
    pub has_async: bool,
}

impl PreambleTemplate {
    pub fn for_generator(module: &Module) -> Self {
        let has_async = module.functions.iter().any(|function| function.is_async)
            || module
                .classes
                .iter()
                .any(|class_item| class_item.methods.iter().any(|method| method.is_async));
        Self {
            prefix: naming::ffi_prefix().to_string(),
            ffi_module_name: None,
            has_async,
        }
    }

    pub fn for_module(module: &Module) -> Self {
        let ffi_module_name = format!("{}FFI", NamingConvention::class_name(&module.name));
        let has_async = module.functions.iter().any(|function| function.is_async)
            || module
                .classes
                .iter()
                .any(|class_item| class_item.methods.iter().any(|method| method.is_async));
        Self {
            prefix: naming::ffi_prefix().to_string(),
            ffi_module_name: Some(ffi_module_name),
            has_async,
        }
    }
}

#[derive(Template)]
#[template(path = "swift/record.txt", escape = "none")]
pub struct RecordTemplate {
    pub class_name: String,
    pub ffi_module: String,
    pub fields: Vec<FieldView>,
    pub has_aliases: bool,
}

impl RecordTemplate {
    pub fn from_record(record: &Record, module: &Module) -> Self {
        let fields: Vec<FieldView> = record
            .fields
            .iter()
            .map(|field| {
                let swift_name = NamingConvention::property_name(&field.name);
                let c_name = naming::snake_to_camel(&field.name);
                let needs_alias = swift_name != c_name;
                FieldView {
                    needs_alias,
                    swift_name,
                    c_name,
                    swift_type: TypeMapper::map_type(&field.field_type),
                }
            })
            .collect();
        let has_aliases = fields.iter().any(|field| field.needs_alias);
        Self {
            class_name: NamingConvention::class_name(&record.name),
            ffi_module: NamingConvention::ffi_module_name(&module.name),
            fields,
            has_aliases,
        }
    }
}

pub struct StructuredError {
    pub swift_type: String,
    pub ffi_type: String,
    pub is_string_error: bool,
}

#[derive(Template)]
#[template(path = "swift/function.txt", escape = "none")]
pub struct FunctionTemplate {
    pub prefix: String,
    pub func_name: String,
    pub ffi_name: String,
    pub ffi_module_name: Option<String>,
    pub params: Vec<super::conversion::ParamInfo>,
    pub return_type: Option<String>,
    pub return_kind: super::marshal::ReturnKind,
    pub structured_error: Option<StructuredError>,
    pub result_ok_ffi_type: Option<String>,
    pub is_async: bool,
    pub throws: bool,
    pub has_callbacks: bool,
    pub callbacks: Vec<super::conversion::CallbackInfo>,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_free: String,
    pub ffi_cancel: String,
    pub ffi_free_vec: String,
    pub has_wrappers: bool,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
    pub callback_args: String,
}

impl FunctionTemplate {
    pub fn from_function(function: &Function, module: &Module) -> Self {
        use super::conversion::{ParamsInfo, ReturnInfo};
        use crate::model::Type;

        let ret = ReturnInfo::from_type(function.output.as_ref());
        let func_name_pascal = NamingConvention::class_name(&function.name);
        let params_info = ParamsInfo::from_inputs(
            function
                .inputs
                .iter()
                .map(|p| (p.name.as_str(), &p.param_type)),
            &func_name_pascal,
        );

        let ffi_name = naming::function_ffi_name(&function.name);
        let call_builder = super::marshal::SyncCallBuilder::new(&ffi_name, false).with_params(
            function
                .non_callback_params()
                .map(|p| (p.name.as_str(), &p.param_type)),
        );

        let callback_args = params_info
            .callbacks
            .iter()
            .map(|cb| format!("{}, {}", cb.trampoline_name, cb.ptr_name))
            .collect::<Vec<_>>()
            .join(", ");

        let ffi_prefix = naming::ffi_prefix().to_string();

        let return_type = if ret.is_void {
            None
        } else if ret.is_result {
            ret.result_ok_type.clone()
        } else {
            ret.swift_type.clone()
        };

        let ffi_free_vec = function
            .output
            .as_ref()
            .and_then(|output_type| match output_type {
                Type::Vec(inner) => Some(inner.as_ref()),
                Type::Result { ok, .. } => match ok.as_ref() {
                    Type::Vec(inner) => Some(inner.as_ref()),
                    _ => None,
                },
                _ => None,
            })
            .map(|inner_type| {
                let inner_ffi = TypeMapper::ffi_type_name(inner_type);
                format!("{}_free_buf_{}", ffi_prefix, inner_ffi)
            })
            .unwrap_or_default();

        let return_kind =
            super::marshal::ReturnKind::from_function(function.output.as_ref(), &function.name, module);

        let structured_error = Self::extract_structured_error(&function.output, module);
        let result_ok_ffi_type = Self::extract_result_ok_ffi_type(&function.output, module);

        let ffi_module_name = Some(NamingConvention::ffi_module_name(&module.name));

        Self {
            prefix: ffi_prefix,
            func_name: NamingConvention::method_name(&function.name),
            ffi_name,
            ffi_module_name,
            params: params_info.params,
            return_type,
            return_kind,
            structured_error,
            result_ok_ffi_type,
            is_async: function.is_async,
            throws: function.throws() || ret.is_result,
            has_callbacks: params_info.has_callbacks,
            callbacks: params_info.callbacks,
            ffi_poll: naming::function_ffi_poll(&function.name),
            ffi_complete: naming::function_ffi_complete(&function.name),
            ffi_free: naming::function_ffi_free(&function.name),
            ffi_cancel: naming::function_ffi_cancel(&function.name),
            ffi_free_vec,
            has_wrappers: call_builder.has_wrappers(),
            wrappers_open: call_builder.build_wrappers_open(),
            wrappers_close: call_builder.build_wrappers_close(),
            ffi_args: call_builder.build_ffi_args(),
            callback_args,
        }
    }

    fn extract_structured_error(
        output: &Option<crate::model::Type>,
        module: &Module,
    ) -> Option<StructuredError> {
        use crate::model::Type;
        let Type::Result { err, .. } = output.as_ref()? else {
            return None;
        };
        let ffi_module = NamingConvention::ffi_module_name(&module.name);
        match err.as_ref() {
            Type::Enum(err_name) => {
                let enum_def = module.enums.iter().find(|e| &e.name == err_name)?;
                if !enum_def.is_error {
                    return None;
                }
                Some(StructuredError {
                    swift_type: NamingConvention::class_name(err_name),
                    ffi_type: format!("{}.{}", ffi_module, err_name),
                    is_string_error: false,
                })
            }
            Type::String => Some(StructuredError {
                swift_type: "FfiError".to_string(),
                ffi_type: format!("{}.FfiError", ffi_module),
                is_string_error: true,
            }),
            _ => None,
        }
    }

    fn extract_result_ok_ffi_type(
        output: &Option<crate::model::Type>,
        module: &Module,
    ) -> Option<String> {
        use crate::model::Type;
        let Type::Result { ok, .. } = output.as_ref()? else {
            return None;
        };
        let ffi_module = NamingConvention::ffi_module_name(&module.name);
        match ok.as_ref() {
            Type::Void => None,
            Type::String => Some(format!("{}.FfiString", ffi_module)),
            Type::Record(name) => Some(NamingConvention::class_name(name)),
            Type::Enum(name) => {
                let enum_def = module.enums.iter().find(|e| &e.name == name);
                if enum_def.map(|e| e.is_data_enum()).unwrap_or(false) {
                    Some(format!("{}.{}", ffi_module, name))
                } else {
                    Some("Int32".to_string())
                }
            }
            _ => Some(TypeMapper::map_type(ok)),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/enum_c_style.txt", escape = "none")]
pub struct CStyleEnumTemplate {
    pub class_name: String,
    pub variants: Vec<CStyleVariantView>,
    pub is_error: bool,
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
            is_error: enumeration.is_error,
        }
    }
}

#[derive(Template)]
#[template(path = "swift/enum_data.txt", escape = "none")]
pub struct DataEnumTemplate {
    pub class_name: String,
    pub ffi_type: String,
    pub variants: Vec<DataVariantView>,
    pub is_error: bool,
}

impl DataEnumTemplate {
    pub fn from_enum(enumeration: &Enumeration, module: &Module) -> Self {
        let ffi_module = NamingConvention::ffi_module_name(&module.name);
        Self {
            class_name: NamingConvention::class_name(&enumeration.name),
            ffi_type: format!("{}.{}", ffi_module, enumeration.name),
            is_error: enumeration.is_error,
            variants: enumeration
                .variants
                .iter()
                .map(|variant| {
                    let is_single_tuple =
                        variant.fields.len() == 1 && variant.fields[0].name.starts_with('_');
                    DataVariantView {
                        swift_name: NamingConvention::enum_case_name(&variant.name),
                        c_name: variant.name.clone(),
                        tag_constant: format!("{}_TAG_{}", enumeration.name, variant.name),
                        is_single_tuple,
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
                    }
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
        Self {
            class_name: NamingConvention::class_name(&class.name),
            doc: class.doc.clone(),
            deprecated: class.deprecated.is_some(),
            deprecated_message: class.deprecated.as_ref().and_then(|d| d.message.clone()),
            ffi_free: naming::class_ffi_free(&class.name),
            constructors: class
                .constructors
                .iter()
                .map(|ctor| {
                    let params_info = super::conversion::ParamsInfo::from_inputs(
                        ctor.inputs.iter().map(|p| (p.name.as_str(), &p.param_type)),
                        &NamingConvention::class_name(&class.name),
                    );
                    ConstructorView {
                        doc: ctor.doc.clone(),
                        ffi_name: naming::class_ffi_new(&class.name),
                        is_failable: false,
                        params: params_info.params,
                    }
                })
                .collect(),
            methods: class
                .methods
                .iter()
                .map(|method| {
                    let params_info = super::conversion::ParamsInfo::from_inputs(
                        method
                            .inputs
                            .iter()
                            .map(|p| (p.name.as_str(), &p.param_type)),
                        &NamingConvention::class_name(&method.name),
                    );
                    MethodView {
                        doc: method.doc.clone(),
                        deprecated: method.deprecated.is_some(),
                        deprecated_message: method
                            .deprecated
                            .as_ref()
                            .and_then(|d| d.message.clone()),
                        swift_name: NamingConvention::method_name(&method.name),
                        is_static: method.is_static(),
                        is_async: method.is_async,
                        throws: method.throws(),
                        return_type: method
                            .output
                            .as_ref()
                            .filter(|ty| !ty.is_void())
                            .map(TypeMapper::map_type),
                        params: params_info.params,
                        body: BodyRenderer::method(method, class, module),
                    }
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
    pub c_name: String,
    pub tag_constant: String,
    pub is_single_tuple: bool,
    pub fields: Vec<FieldView>,
}

pub struct ConstructorView {
    pub doc: Option<String>,
    pub ffi_name: String,
    pub is_failable: bool,
    pub params: Vec<super::conversion::ParamInfo>,
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
    pub params: Vec<super::conversion::ParamInfo>,
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
    pub atomic_cas_fn: String,
}

impl StreamAsyncBodyTemplate {
    pub fn from_stream(stream: &StreamMethod, class: &Class, _module: &Module) -> Self {
        Self {
            item_type: TypeMapper::map_type(&stream.item_type),
            subscribe_fn: naming::stream_ffi_subscribe(&class.name, &stream.name),
            pop_batch_fn: naming::stream_ffi_pop_batch(&class.name, &stream.name),
            poll_fn: naming::stream_ffi_poll(&class.name, &stream.name),
            unsubscribe_fn: naming::stream_ffi_unsubscribe(&class.name, &stream.name),
            free_fn: naming::stream_ffi_free(&class.name, &stream.name),
            atomic_cas_fn: format!("{}_atomic_u8_cas", naming::ffi_prefix()),
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
    pub fn from_stream(stream: &StreamMethod, class: &Class, _module: &Module) -> Self {
        Self {
            class_name: NamingConvention::class_name(&class.name),
            method_name_pascal: NamingConvention::class_name(&stream.name),
            subscribe_fn: naming::stream_ffi_subscribe(&class.name, &stream.name),
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
    pub atomic_cas_fn: String,
}

impl StreamCallbackBodyTemplate {
    pub fn from_stream(stream: &StreamMethod, class: &Class, _module: &Module) -> Self {
        Self {
            item_type: TypeMapper::map_type(&stream.item_type),
            class_name: NamingConvention::class_name(&class.name),
            method_name_pascal: NamingConvention::class_name(&stream.name),
            subscribe_fn: naming::stream_ffi_subscribe(&class.name, &stream.name),
            pop_batch_fn: naming::stream_ffi_pop_batch(&class.name, &stream.name),
            poll_fn: naming::stream_ffi_poll(&class.name, &stream.name),
            unsubscribe_fn: naming::stream_ffi_unsubscribe(&class.name, &stream.name),
            free_fn: naming::stream_ffi_free(&class.name, &stream.name),
            atomic_cas_fn: format!("{}_atomic_u8_cas", naming::ffi_prefix()),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/method_sync.txt", escape = "none")]
pub struct SyncMethodBodyTemplate {
    pub ffi_name: String,
    pub has_return: bool,
    pub has_wrappers: bool,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
}

impl SyncMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, _module: &Module) -> Self {
        let ffi_name = naming::method_ffi_name(&class.name, &method.name);
        let call_builder = super::marshal::SyncCallBuilder::new(&ffi_name, true).with_params(
            method
                .non_callback_params()
                .map(|p| (p.name.as_str(), &p.param_type)),
        );

        Self {
            ffi_name,
            has_return: method.output.as_ref().is_some_and(|t| !t.is_void()),
            has_wrappers: call_builder.has_wrappers(),
            wrappers_open: call_builder.build_wrappers_open(),
            wrappers_close: call_builder.build_wrappers_close(),
            ffi_args: call_builder.build_ffi_args(),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/method_callback.txt", escape = "none")]
pub struct CallbackMethodBodyTemplate {
    pub ffi_name: String,
    pub has_return: bool,
    pub callbacks: Vec<super::conversion::CallbackInfo>,
    pub has_wrappers: bool,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
    pub callback_args: String,
}

impl CallbackMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, _module: &Module) -> Self {
        let ffi_name = naming::method_ffi_name(&class.name, &method.name);
        let call_builder = super::marshal::SyncCallBuilder::new(&ffi_name, true).with_params(
            method
                .non_callback_params()
                .map(|p| (p.name.as_str(), &p.param_type)),
        );

        let params_info = super::conversion::ParamsInfo::from_inputs(
            method
                .inputs
                .iter()
                .map(|p| (p.name.as_str(), &p.param_type)),
            &NamingConvention::class_name(&method.name),
        );

        let callback_args = params_info
            .callbacks
            .iter()
            .map(|cb| format!("{}, {}", cb.trampoline_name, cb.ptr_name))
            .collect::<Vec<_>>()
            .join(", ");

        Self {
            ffi_name,
            has_return: method.output.as_ref().is_some_and(|t| !t.is_void()),
            callbacks: params_info.callbacks,
            has_wrappers: call_builder.has_wrappers(),
            wrappers_open: call_builder.build_wrappers_open(),
            wrappers_close: call_builder.build_wrappers_close(),
            ffi_args: call_builder.build_ffi_args(),
            callback_args,
        }
    }
}

#[derive(Template)]
#[template(path = "swift/method_throwing.txt", escape = "none")]
pub struct ThrowingMethodBodyTemplate {
    pub ffi_name: String,
    pub return_type: String,
    pub has_wrappers: bool,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
}

impl ThrowingMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, _module: &Module) -> Self {
        let ffi_name = naming::method_ffi_name(&class.name, &method.name);
        let call_builder = super::marshal::SyncCallBuilder::new(&ffi_name, true).with_params(
            method
                .inputs
                .iter()
                .map(|p| (p.name.as_str(), &p.param_type)),
        );

        Self {
            ffi_name,
            return_type: method
                .output
                .as_ref()
                .map(TypeMapper::map_type)
                .unwrap_or_else(|| "Void".into()),
            has_wrappers: call_builder.has_wrappers(),
            wrappers_open: call_builder.build_wrappers_open(),
            wrappers_close: call_builder.build_wrappers_close(),
            ffi_args: call_builder.build_ffi_args(),
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
    pub fn from_method(method: &Method, class: &Class, _module: &Module) -> Self {
        Self {
            ffi_name: naming::method_ffi_name(&class.name, &method.name),
            ffi_poll: naming::method_ffi_poll(&class.name, &method.name),
            ffi_complete: naming::method_ffi_complete(&class.name, &method.name),
            ffi_cancel: naming::method_ffi_cancel(&class.name, &method.name),
            ffi_free: naming::method_ffi_free(&class.name, &method.name),
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
    pub fn from_method(method: &Method, class: &Class, _module: &Module) -> Self {
        Self {
            ffi_name: naming::method_ffi_name(&class.name, &method.name),
            ffi_poll: naming::method_ffi_poll(&class.name, &method.name),
            ffi_complete: naming::method_ffi_complete(&class.name, &method.name),
            ffi_cancel: naming::method_ffi_cancel(&class.name, &method.name),
            ffi_free: naming::method_ffi_free(&class.name, &method.name),
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
    pub fn from_stream(stream: &StreamMethod, class: &Class, _module: &Module) -> Self {
        Self {
            class_name: NamingConvention::class_name(&class.name),
            method_name_pascal: NamingConvention::class_name(&stream.name),
            item_type: TypeMapper::map_type(&stream.item_type),
            pop_batch_fn: naming::stream_ffi_pop_batch(&class.name, &stream.name),
            wait_fn: naming::stream_ffi_wait(&class.name, &stream.name),
            unsubscribe_fn: naming::stream_ffi_unsubscribe(&class.name, &stream.name),
            free_fn: naming::stream_ffi_free(&class.name, &stream.name),
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
    pub foreign_type: String,
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
    pub fn from_trait(callback_trait: &CallbackTrait, _module: &Module) -> Self {
        let trait_name = &callback_trait.name;

        Self {
            doc: callback_trait.doc.clone(),
            protocol_name: format!("{}Protocol", trait_name),
            wrapper_class: format!("{}Wrapper", trait_name),
            vtable_var: format!("{}VTableInstance", to_camel_case(trait_name)),
            vtable_type: naming::callback_vtable_name(trait_name),
            bridge_name: format!("{}Bridge", trait_name),
            foreign_type: naming::callback_foreign_name(trait_name),
            register_fn: naming::callback_register_fn(trait_name),
            create_fn: naming::callback_create_fn(trait_name),
            methods: callback_trait
                .methods
                .iter()
                .map(|method| {
                    let has_return = method.has_return();
                    TraitMethodView {
                        swift_name: NamingConvention::method_name(&method.name),
                        ffi_name: naming::to_snake_case(&method.name),
                        params: method
                            .inputs
                            .iter()
                            .map(|param| {
                                let swift_name = NamingConvention::param_name(&param.name);
                                TraitParamView {
                                    label: swift_name.clone(),
                                    ffi_name: param.name.clone(),
                                    swift_type: TypeMapper::map_type(&param.param_type),
                                    conversion: param.name.clone(),
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
