mod names;
mod templates;
mod types;

use askama::Template;

use crate::model::{Class, Enumeration, Module, Record};

pub use names::NamingConvention;
pub use templates::{ClassTemplate, CStyleEnumTemplate, DataEnumTemplate, RecordTemplate};
pub use types::TypeMapper;

pub struct Swift;

impl Swift {
    pub fn render_record(record: &Record) -> String {
        RecordTemplate::from_record(record)
            .render()
            .expect("record template failed")
    }

    pub fn render_enum(enumeration: &Enumeration) -> String {
        if enumeration.is_c_style() {
            CStyleEnumTemplate::from_enum(enumeration)
                .render()
                .expect("c-style enum template failed")
        } else {
            DataEnumTemplate::from_enum(enumeration)
                .render()
                .expect("data enum template failed")
        }
    }

    pub fn render_class(class: &Class, module: &Module) -> String {
        ClassTemplate::from_class(class, module)
            .render()
            .expect("class template failed")
    }
}
