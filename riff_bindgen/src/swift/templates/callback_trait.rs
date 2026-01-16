use askama::Template;
use riff_ffi_rules::naming;

use crate::model::{CallbackTrait, Module, Type};

use super::super::names::NamingConvention;
use super::super::types::TypeMapper;

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
