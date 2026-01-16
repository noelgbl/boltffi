use askama::Template;
use riff_ffi_rules::naming;

use crate::model::Module;

use super::super::names::NamingConvention;

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
