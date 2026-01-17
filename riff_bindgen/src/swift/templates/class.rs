use askama::Template;
use riff_ffi_rules::naming;

use crate::model::{Class, Module, StreamMode};

use super::super::body::BodyRenderer;
use super::super::conversion::{ParamInfo, ParamsInfo};
use super::super::names::NamingConvention;
use super::super::types::TypeMapper;

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
                    let params_info = ParamsInfo::from_inputs(
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
                        first_param_name: first_param
                            .map(|p| p.swift_name.clone())
                            .unwrap_or_default(),
                        first_param_type: first_param
                            .map(|p| p.swift_type.clone())
                            .unwrap_or_default(),
                        rest_params,
                        params: params_info.params,
                    }
                })
                .collect(),
            methods: class
                .methods
                .iter()
                .map(|method| {
                    let params_info = ParamsInfo::from_inputs(
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
