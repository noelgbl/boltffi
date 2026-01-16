use askama::Template;
use riff_ffi_rules::naming;

use crate::model::{Class, Module, StreamMethod, Type};

use super::super::names::NamingConvention;
use super::super::types::TypeMapper;
use super::super::wire;

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
