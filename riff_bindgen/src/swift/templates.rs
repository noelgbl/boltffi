use askama::Template;
use riff_ffi_rules::naming;

use crate::model::{
    CallbackTrait, Class, Enumeration, Function, Method, Module, Record,
    StreamMethod, StreamMode, Type,
};

use super::body::BodyRenderer;
use super::conversion::ParamInfo;
use super::marshal::{ReturnAbi, SyncCallBuilder};
use super::names::NamingConvention;
use super::types::TypeMapper;
use super::wire;

struct MethodContext {
    ffi_name: String,
    wrappers_open: String,
    wrappers_close: String,
    ffi_args: String,
}

impl MethodContext {
    fn from_method(method: &Method, class: &Class, module: &Module, include_handle: bool) -> Self {
        let call_builder = SyncCallBuilder::new(include_handle).with_params(
            method.non_callback_params().map(|p| (p.name.as_str(), &p.param_type)),
            module,
        );
        Self {
            ffi_name: naming::method_ffi_name(&class.name, &method.name),
            wrappers_open: call_builder.build_wrappers_open(),
            wrappers_close: call_builder.build_wrappers_close(),
            ffi_args: call_builder.build_ffi_args(),
        }
    }

    fn from_method_all_params(method: &Method, class: &Class, module: &Module) -> Self {
        let call_builder = SyncCallBuilder::new(false).with_params(
            method.inputs.iter().map(|p| (p.name.as_str(), &p.param_type)),
            module,
        );
        Self {
            ffi_name: naming::method_ffi_name(&class.name, &method.name),
            wrappers_open: call_builder.build_wrappers_open(),
            wrappers_close: call_builder.build_wrappers_close(),
            ffi_args: call_builder.build_ffi_args(),
        }
    }
}

#[derive(Template)]
#[template(path = "swift/preamble.txt", escape = "none")]
pub struct PreambleTemplate {
    pub prefix: String,
    pub ffi_module_name: Option<String>,
    pub has_async: bool,
    pub has_streams: bool,
}

impl PreambleTemplate {
    pub fn for_generator(module: &Module) -> Self {
        let has_async = module.functions.iter().any(|function| function.is_async)
            || module
                .classes
                .iter()
                .any(|class_item| class_item.methods.iter().any(|method| method.is_async));
        let has_streams = module.classes.iter().any(|c| !c.streams.is_empty());
        Self {
            prefix: naming::ffi_prefix().to_string(),
            ffi_module_name: None,
            has_async,
            has_streams,
        }
    }

    pub fn for_module(module: &Module) -> Self {
        let ffi_module_name = format!("{}FFI", NamingConvention::class_name(&module.name));
        let has_async = module.functions.iter().any(|function| function.is_async)
            || module
                .classes
                .iter()
                .any(|class_item| class_item.methods.iter().any(|method| method.is_async));
        let has_streams = module.classes.iter().any(|c| !c.streams.is_empty());
        Self {
            prefix: naming::ffi_prefix().to_string(),
            ffi_module_name: Some(ffi_module_name),
            has_async,
            has_streams,
        }
    }
}

#[derive(Template)]
#[template(path = "swift/record.txt", escape = "none")]
pub struct RecordTemplate {
    pub class_name: String,
    pub fields: Vec<FieldView>,
    pub is_blittable: bool,
}

impl RecordTemplate {
    pub fn from_record(record: &Record, module: &Module) -> Self {
        let fields: Vec<FieldView> = record
            .fields
            .iter()
            .enumerate()
            .map(|(idx, field)| Self::make_field(field, idx, module))
            .collect();
        let is_blittable = record.fields.iter().all(|f| Self::is_type_blittable(&f.field_type));
        Self {
            class_name: NamingConvention::class_name(&record.name),
            fields,
            is_blittable,
        }
    }

    fn is_type_blittable(ty: &Type) -> bool {
        matches!(ty, Type::Primitive(_))
    }

    fn make_field(field: &crate::model::RecordField, _idx: usize, module: &Module) -> FieldView {
        let swift_name = NamingConvention::property_name(&field.name);
        let encoder = wire::encode_type(&field.field_type, &swift_name, module);
        
        FieldView {
            swift_name: swift_name.clone(),
            swift_type: TypeMapper::map_type(&field.field_type),
            wire_size_expr: encoder.size_expr,
            wire_decode_inline: Self::make_decode_inline(&field.field_type, module),
            wire_encode: encoder.encode_to_data,
            wire_encode_bytes: encoder.encode_to_bytes,
        }
    }

    fn make_decode_inline(ty: &Type, module: &Module) -> String {
        let codec = wire::decode_type(ty, module);
        let reader = codec.reader_expr.replace("OFFSET", "pos");
        match &codec.size_kind {
            wire::SizeKind::Fixed(size) => {
                format!("{{ let v = {}; pos += {}; return v }}()", reader, size)
            }
            wire::SizeKind::Variable => {
                format!("{{ let (v, s) = {}; pos += s; return v }}()", reader)
            }
        }
    }
}

#[derive(Template)]
#[template(path = "swift/function.txt", escape = "none")]
pub struct FunctionTemplate {
    pub prefix: String,
    pub func_name: String,
    pub ffi_name: String,
    pub params: Vec<ParamInfo>,
    pub return_type: Option<String>,
    pub return_abi: ReturnAbi,
    pub direct_call: String,
    pub is_async: bool,
    pub throws: bool,
    pub has_callbacks: bool,
    pub callbacks: Vec<super::conversion::CallbackInfo>,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_free: String,
    pub ffi_cancel: String,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
    pub callback_args: String,
}

impl FunctionTemplate {
    pub fn from_function(function: &Function, module: &Module) -> Self {
        use super::conversion::{ParamsInfo, ReturnInfo};

        let ret = ReturnInfo::from_return_type(&function.returns);
        let func_name_pascal = NamingConvention::class_name(&function.name);
        let params_info = ParamsInfo::from_inputs(
            function
                .inputs
                .iter()
                .map(|p| (p.name.as_str(), &p.param_type)),
            &func_name_pascal,
        );

        let ffi_name = naming::function_ffi_name(&function.name);
        let call_builder = SyncCallBuilder::new(false).with_params(
            function
                .non_callback_params()
                .map(|p| (p.name.as_str(), &p.param_type)),
            module,
        );

        let callback_args = params_info
            .callbacks
            .iter()
            .map(|cb| format!("{}, {}", cb.trampoline_name, cb.ptr_name))
            .collect::<Vec<_>>()
            .join(", ");

        let return_type = if ret.is_void {
            None
        } else if ret.is_result {
            ret.result_ok_type.clone()
        } else {
            ret.swift_type.clone()
        };

        let return_abi = ReturnAbi::from_return_type(&function.returns, module);
        let direct_call = return_abi.direct_call_expr(&format!("{}({})", ffi_name, call_builder.build_ffi_args()));

        Self {
            prefix: naming::ffi_prefix().to_string(),
            func_name: NamingConvention::method_name(&function.name),
            ffi_name,
            params: params_info.params,
            return_type,
            return_abi,
            direct_call,
            is_async: function.is_async,
            throws: function.throws() || ret.is_result,
            has_callbacks: params_info.has_callbacks,
            callbacks: params_info.callbacks,
            ffi_poll: naming::function_ffi_poll(&function.name),
            ffi_complete: naming::function_ffi_complete(&function.name),
            ffi_free: naming::function_ffi_free(&function.name),
            ffi_cancel: naming::function_ffi_cancel(&function.name),
            wrappers_open: call_builder.build_wrappers_open(),
            wrappers_close: call_builder.build_wrappers_close(),
            ffi_args: call_builder.build_ffi_args(),
            callback_args,
        }
    }

    pub fn has_wrappers(&self) -> bool {
        !self.wrappers_open.is_empty()
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
                    let fields: Vec<EnumFieldView> = variant
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(_, field)| {
                            let swift_name = NamingConvention::param_name(&field.name);
                            let c_name = field.name.clone();
                            let wire_decode = Self::enum_field_wire_decode(&field.field_type, &swift_name, module);
                            let wire_size = Self::enum_field_wire_size(&field.field_type, &swift_name, module);
                            let wire_encode = Self::enum_field_wire_encode(&field.field_type, &swift_name, module);
                            let wire_encode_bytes = Self::enum_field_wire_encode_bytes(&field.field_type, &swift_name, module);
                            EnumFieldView {
                                needs_alias: swift_name != c_name,
                                swift_name,
                                c_name,
                                swift_type: TypeMapper::map_type(&field.field_type),
                                wire_decode,
                                wire_size,
                                wire_encode,
                                wire_encode_bytes,
                            }
                        })
                        .collect();
                    let single_wire_decode = if is_single_tuple && !variant.fields.is_empty() {
                        Self::single_tuple_wire_decode(&variant.fields[0].field_type, module)
                    } else {
                        String::new()
                    };
                    let wire_encode_single = if is_single_tuple && !variant.fields.is_empty() {
                        Self::single_tuple_wire_encode(&variant.fields[0].field_type, module)
                    } else {
                        String::new()
                    };
                    let wire_encode_bytes_single = if is_single_tuple && !variant.fields.is_empty() {
                        Self::single_tuple_wire_encode_bytes(&variant.fields[0].field_type, module)
                    } else {
                        String::new()
                    };
                    DataVariantView {
                        swift_name: NamingConvention::enum_case_name(&variant.name),
                        c_name: variant.name.clone(),
                        tag_constant: format!("{}_TAG_{}", enumeration.name, variant.name),
                        is_single_tuple,
                        wire_decode: single_wire_decode,
                        wire_encode_single,
                        wire_encode_bytes_single,
                        fields,
                    }
                })
                .collect(),
        }
    }

    fn single_tuple_wire_decode(ty: &Type, module: &Module) -> String {
        wire::decode_type(ty, module).decode_as_tuple("pos")
    }

    fn enum_field_wire_decode(ty: &Type, name: &str, module: &Module) -> String {
        wire::decode_type(ty, module).decode_to_binding(name, "pos")
    }

    fn enum_field_wire_size(ty: &Type, name: &str, module: &Module) -> String {
        wire::encode_type(ty, name, module).size_expr
    }

    fn enum_field_wire_encode(ty: &Type, name: &str, module: &Module) -> String {
        wire::encode_type(ty, name, module).encode_to_data
    }

    fn single_tuple_wire_encode(ty: &Type, module: &Module) -> String {
        wire::encode_type(ty, "value", module).encode_to_data
    }

    fn enum_field_wire_encode_bytes(ty: &Type, name: &str, module: &Module) -> String {
        wire::encode_type(ty, name, module).encode_to_bytes
    }

    fn single_tuple_wire_encode_bytes(ty: &Type, module: &Module) -> String {
        wire::encode_type(ty, "value", module).encode_to_bytes
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
                    let is_factory = !ctor.is_default();
                    let first_param = params_info.params.first();
                    let rest_params: Vec<_> = params_info.params.iter().skip(1).cloned().collect();
                    ConstructorView {
                        doc: ctor.doc.clone(),
                        name: NamingConvention::method_name(&ctor.name),
                        ffi_name: if is_factory {
                            naming::method_ffi_name(&class.name, &ctor.name)
                        } else {
                            naming::class_ffi_new(&class.name)
                        },
                        is_failable: false,
                        is_factory,
                        first_param_name: first_param.map(|p| p.swift_name.clone()).unwrap_or_default(),
                        first_param_type: first_param.map(|p| p.swift_type.clone()).unwrap_or_default(),
                        rest_params,
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
                            .returns
                            .ok_type()
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
    pub swift_type: String,
    pub wire_size_expr: String,
    pub wire_decode_inline: String,
    pub wire_encode: String,
    pub wire_encode_bytes: String,
}

pub struct CStyleVariantView {
    pub swift_name: String,
    pub discriminant: i64,
}

pub struct EnumFieldView {
    pub swift_name: String,
    pub c_name: String,
    pub swift_type: String,
    pub needs_alias: bool,
    pub wire_decode: String,
    pub wire_size: String,
    pub wire_encode: String,
    pub wire_encode_bytes: String,
}

pub struct DataVariantView {
    pub swift_name: String,
    pub c_name: String,
    pub tag_constant: String,
    pub is_single_tuple: bool,
    pub wire_decode: String,
    pub wire_encode_single: String,
    pub wire_encode_bytes_single: String,
    pub fields: Vec<EnumFieldView>,
}

pub struct ConstructorView {
    pub doc: Option<String>,
    pub name: String,
    pub ffi_name: String,
    pub is_failable: bool,
    pub is_factory: bool,
    pub params: Vec<ParamInfo>,
    pub first_param_name: String,
    pub first_param_type: String,
    pub rest_params: Vec<ParamInfo>,
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
    pub params: Vec<ParamInfo>,
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
    pub item_decode_expr: String,
    pub subscribe_fn: String,
    pub pop_batch_fn: String,
    pub poll_fn: String,
    pub unsubscribe_fn: String,
    pub free_fn: String,
    pub prefix: String,
    pub atomic_cas_fn: String,
}

impl StreamAsyncBodyTemplate {
    pub fn from_stream(stream: &StreamMethod, class: &Class, module: &Module) -> Self {
        let item_decode_expr = Self::item_decode(&stream.item_type, module);
        Self {
            item_type: TypeMapper::map_type(&stream.item_type),
            item_decode_expr,
            subscribe_fn: naming::stream_ffi_subscribe(&class.name, &stream.name),
            pop_batch_fn: naming::stream_ffi_pop_batch(&class.name, &stream.name),
            poll_fn: naming::stream_ffi_poll(&class.name, &stream.name),
            unsubscribe_fn: naming::stream_ffi_unsubscribe(&class.name, &stream.name),
            free_fn: naming::stream_ffi_free(&class.name, &stream.name),
            prefix: naming::ffi_prefix().to_string(),
            atomic_cas_fn: format!("{}_atomic_u8_cas", naming::ffi_prefix()),
        }
    }

    fn item_decode(ty: &Type, module: &Module) -> String {
        wire::decode_type(ty, module).as_stream_item_closure("offset")
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
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
}

impl SyncMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let ctx = MethodContext::from_method(method, class, module, true);
        Self {
            ffi_name: ctx.ffi_name,
            has_return: method.returns.has_return_value(),
            wrappers_open: ctx.wrappers_open,
            wrappers_close: ctx.wrappers_close,
            ffi_args: ctx.ffi_args,
        }
    }

    pub fn has_wrappers(&self) -> bool {
        !self.wrappers_open.is_empty()
    }
}

#[derive(Template)]
#[template(path = "swift/method_callback.txt", escape = "none")]
pub struct CallbackMethodBodyTemplate {
    pub ffi_name: String,
    pub has_return: bool,
    pub callbacks: Vec<super::conversion::CallbackInfo>,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
    pub callback_args: String,
}

impl CallbackMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let ctx = MethodContext::from_method(method, class, module, true);
        let params_info = super::conversion::ParamsInfo::from_inputs(
            method.inputs.iter().map(|p| (p.name.as_str(), &p.param_type)),
            &NamingConvention::class_name(&method.name),
        );
        let callback_args = params_info
            .callbacks
            .iter()
            .map(|cb| format!("{}, {}", cb.trampoline_name, cb.ptr_name))
            .collect::<Vec<_>>()
            .join(", ");

        Self {
            ffi_name: ctx.ffi_name,
            has_return: method.returns.has_return_value(),
            callbacks: params_info.callbacks,
            wrappers_open: ctx.wrappers_open,
            wrappers_close: ctx.wrappers_close,
            ffi_args: ctx.ffi_args,
            callback_args,
        }
    }

    pub fn has_wrappers(&self) -> bool {
        !self.wrappers_open.is_empty()
    }
}

#[derive(Template)]
#[template(path = "swift/method_throwing.txt", escape = "none")]
pub struct ThrowingMethodBodyTemplate {
    pub ffi_name: String,
    pub prefix: String,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
    pub decode_expr: String,
}

impl ThrowingMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let ctx = MethodContext::from_method_all_params(method, class, module);
        let return_abi = ReturnAbi::from_return_type(&method.returns, module);
        Self {
            ffi_name: ctx.ffi_name,
            prefix: naming::ffi_prefix().to_string(),
            wrappers_open: ctx.wrappers_open,
            wrappers_close: ctx.wrappers_close,
            ffi_args: ctx.ffi_args,
            decode_expr: return_abi.decode_expr().to_string(),
        }
    }

    pub fn has_wrappers(&self) -> bool {
        !self.wrappers_open.is_empty()
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
    pub prefix: String,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
    pub return_type: String,
    pub decode_expr: String,
}

impl AsyncMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let ctx = MethodContext::from_method(method, class, module, true);
        let return_abi = ReturnAbi::from_return_type(&method.returns, module);
        Self {
            ffi_name: ctx.ffi_name,
            ffi_poll: naming::method_ffi_poll(&class.name, &method.name),
            ffi_complete: naming::method_ffi_complete(&class.name, &method.name),
            ffi_cancel: naming::method_ffi_cancel(&class.name, &method.name),
            ffi_free: naming::method_ffi_free(&class.name, &method.name),
            prefix: naming::ffi_prefix().to_string(),
            wrappers_open: ctx.wrappers_open,
            wrappers_close: ctx.wrappers_close,
            ffi_args: ctx.ffi_args,
            return_type: method
                .returns
                .ok_type()
                .map(TypeMapper::map_type)
                .unwrap_or_else(|| "Void".into()),
            decode_expr: return_abi.decode_expr().to_string(),
        }
    }

    pub fn has_wrappers(&self) -> bool {
        !self.wrappers_open.is_empty()
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
    pub prefix: String,
    pub wrappers_open: String,
    pub wrappers_close: String,
    pub ffi_args: String,
    pub return_type: String,
    pub decode_expr: String,
}

impl AsyncThrowingMethodBodyTemplate {
    pub fn from_method(method: &Method, class: &Class, module: &Module) -> Self {
        let ctx = MethodContext::from_method(method, class, module, true);
        let return_abi = ReturnAbi::from_return_type(&method.returns, module);
        Self {
            ffi_name: ctx.ffi_name,
            ffi_poll: naming::method_ffi_poll(&class.name, &method.name),
            ffi_complete: naming::method_ffi_complete(&class.name, &method.name),
            ffi_cancel: naming::method_ffi_cancel(&class.name, &method.name),
            ffi_free: naming::method_ffi_free(&class.name, &method.name),
            prefix: naming::ffi_prefix().to_string(),
            wrappers_open: ctx.wrappers_open,
            wrappers_close: ctx.wrappers_close,
            ffi_args: ctx.ffi_args,
            return_type: method
                .returns
                .ok_type()
                .map(TypeMapper::map_type)
                .unwrap_or_else(|| "Void".into()),
            decode_expr: return_abi.decode_expr().to_string(),
        }
    }

    pub fn has_wrappers(&self) -> bool {
        !self.wrappers_open.is_empty()
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
    pub wire_encoded_return: bool,
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
                        return_type: method.returns.ok_type().map(TypeMapper::map_type),
                        is_async: method.is_async,
                        throws: method.throws(),
                        has_return,
                        has_out_param: has_return && !method.is_async,
                        wire_encoded_return: method
                            .returns
                            .ok_type()
                            .map(|ty| matches!(ty, Type::Record(_) | Type::String | Type::Vec(_)))
                            .unwrap_or(false),
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
