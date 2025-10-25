mod body;
mod names;
mod templates;
mod types;

use askama::Template;

use crate::model::{CallbackTrait, Class, Enumeration, Module, Record, StreamMode};

pub use body::BodyRenderer;
pub use names::NamingConvention;
pub use templates::{
    CallbackTraitTemplate, ClassTemplate, CStyleEnumTemplate, DataEnumTemplate, RecordTemplate,
    StreamCancellableTemplate, StreamSubscriptionTemplate,
};
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

    pub fn render_stream_wrappers(class: &Class, module: &Module) -> String {
        class
            .streams
            .iter()
            .filter_map(|stream| match stream.mode {
                StreamMode::Batch => Some(
                    StreamSubscriptionTemplate::from_stream(stream, class, module)
                        .render()
                        .expect("subscription template failed"),
                ),
                StreamMode::Callback => Some(
                    StreamCancellableTemplate::from_stream(stream, class, module)
                        .render()
                        .expect("cancellable template failed"),
                ),
                StreamMode::Async => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub fn render_callback_trait(callback_trait: &CallbackTrait, module: &Module) -> String {
        CallbackTraitTemplate::from_trait(callback_trait, module)
            .render()
            .expect("callback trait template failed")
    }
}
