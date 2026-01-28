use askama::Template;

use super::plan::KotlinMethodImpl::{AsyncMethod, SyncMethod};
use super::plan::{KotlinModule, KotlinStreamMode};

#[derive(Template)]
#[template(path = "render_kotlin/preamble.txt", escape = "none")]
pub struct PreambleTemplate<'a> {
    pub package_name: &'a str,
    pub prefix: &'a str,
    pub extra_imports: &'a [String],
    pub custom_types: &'a [super::plan::KotlinCustomType],
    pub has_streams: bool,
}

#[derive(Template)]
#[template(path = "render_kotlin/native.txt", escape = "none")]
pub struct NativeTemplate<'a> {
    pub lib_name: &'a str,
    pub prefix: &'a str,
    pub functions: &'a [super::plan::KotlinNativeFunction],
    pub wire_functions: &'a [super::plan::KotlinNativeWireFunction],
    pub classes: &'a [super::plan::KotlinNativeClass],
    pub async_callback_invokers: &'a [super::plan::KotlinAsyncCallbackInvoker],
}

#[derive(Template)]
#[template(path = "render_kotlin/record.txt", escape = "none")]
pub struct RecordTemplate<'a> {
    pub class_name: &'a str,
    pub fields: &'a [super::plan::KotlinRecordField],
    pub is_blittable: bool,
    pub struct_size: usize,
}

#[derive(Template)]
#[template(path = "render_kotlin/record_reader.txt", escape = "none")]
pub struct RecordReaderTemplate<'a> {
    pub reader_name: &'a str,
    pub class_name: &'a str,
    pub struct_size: usize,
    pub fields: &'a [super::plan::KotlinRecordReaderField],
}

#[derive(Template)]
#[template(path = "render_kotlin/record_writer.txt", escape = "none")]
pub struct RecordWriterTemplate<'a> {
    pub writer_name: &'a str,
    pub class_name: &'a str,
    pub struct_size: usize,
    pub fields: &'a [super::plan::KotlinRecordWriterField],
}

#[derive(Template)]
#[template(path = "render_kotlin/enum_c_style.txt", escape = "none")]
pub struct CStyleEnumTemplate<'a> {
    pub class_name: &'a str,
    pub variants: &'a [super::plan::KotlinEnumVariant],
}

#[derive(Template)]
#[template(path = "render_kotlin/enum_sealed.txt", escape = "none")]
pub struct SealedEnumTemplate<'a> {
    pub class_name: &'a str,
    pub variants: &'a [super::plan::KotlinEnumVariant],
    pub is_error: bool,
}

#[derive(Template)]
#[template(path = "render_kotlin/enum_data_codec.txt", escape = "none")]
pub struct DataEnumCodecTemplate<'a> {
    pub class_name: &'a str,
    pub codec_name: &'a str,
    pub struct_size: usize,
    pub payload_offset: usize,
    pub variants: &'a [super::plan::KotlinDataEnumVariant],
}

#[derive(Template)]
#[template(path = "render_kotlin/function_wire.txt", escape = "none")]
pub struct WireFunctionTemplate<'a> {
    pub func_name: &'a str,
    pub signature_params: &'a [super::plan::KotlinSignatureParam],
    pub return_type: Option<&'a str>,
    pub wire_writers: &'a [super::plan::KotlinWireWriter],
    pub wire_writer_closes: &'a [String],
    pub native_args: &'a [String],
    pub throws: bool,
    pub err_type: &'a str,
    pub ffi_name: &'a str,
    pub return_abi: &'a super::plan::KotlinReturnAbi,
    pub decode_expr: &'a str,
    pub is_blittable_return: bool,
}

#[derive(Template)]
#[template(path = "render_kotlin/function_async.txt", escape = "none")]
pub struct AsyncFunctionTemplate<'a> {
    pub func_name: &'a str,
    pub signature_params: &'a [super::plan::KotlinSignatureParam],
    pub return_type: Option<&'a str>,
    pub wire_writers: &'a [super::plan::KotlinWireWriter],
    pub wire_writer_closes: &'a [String],
    pub native_args: &'a [String],
    pub throws: bool,
    pub err_type: &'a str,
    pub ffi_name: &'a str,
    pub include_handle: bool,
    pub ffi_poll: &'a str,
    pub ffi_complete: &'a str,
    pub ffi_cancel: &'a str,
    pub ffi_free: &'a str,
    pub return_abi: &'a super::plan::KotlinReturnAbi,
    pub decode_expr: &'a str,
    pub is_blittable_return: bool,
}

#[derive(Template)]
#[template(path = "render_kotlin/class.txt", escape = "none")]
pub struct ClassTemplate<'a> {
    pub class_name: &'a str,
    pub doc: &'a Option<String>,
    pub constructors: &'a [super::plan::KotlinConstructor],
    pub methods: &'a [super::plan::KotlinMethod],
    pub streams: &'a [super::plan::KotlinStream],
    pub use_companion_methods: bool,
    pub has_factory_ctors: bool,
    pub prefix: &'a str,
    pub ffi_free: &'a str,
}

#[derive(Template)]
#[template(path = "render_kotlin/method_wire.txt", escape = "none")]
pub struct WireMethodTemplate<'a> {
    pub method_name: &'a str,
    pub signature_params: &'a [super::plan::KotlinSignatureParam],
    pub return_type: Option<&'a str>,
    pub wire_writers: &'a [super::plan::KotlinWireWriter],
    pub wire_writer_closes: &'a [String],
    pub native_args: &'a [String],
    pub throws: bool,
    pub err_type: &'a str,
    pub ffi_name: &'a str,
    pub return_abi: &'a super::plan::KotlinReturnAbi,
    pub decode_expr: &'a str,
    pub is_blittable_return: bool,
    pub include_handle: bool,
}

#[derive(Template)]
#[template(path = "render_kotlin/method_async.txt", escape = "none")]
pub struct AsyncMethodTemplate<'a> {
    pub method_name: &'a str,
    pub signature_params: &'a [super::plan::KotlinSignatureParam],
    pub return_type: Option<&'a str>,
    pub wire_writers: &'a [super::plan::KotlinWireWriter],
    pub wire_writer_closes: &'a [String],
    pub native_args: &'a [String],
    pub throws: bool,
    pub err_type: &'a str,
    pub ffi_name: &'a str,
    pub include_handle: bool,
    pub ffi_poll: &'a str,
    pub ffi_complete: &'a str,
    pub ffi_cancel: &'a str,
    pub ffi_free: &'a str,
    pub return_abi: &'a super::plan::KotlinReturnAbi,
    pub decode_expr: &'a str,
    pub is_blittable_return: bool,
}

#[derive(Template)]
#[template(path = "render_kotlin/callback_trait.txt", escape = "none")]
pub struct CallbackTraitTemplate<'a> {
    pub interface_name: &'a str,
    pub handle_map_name: &'a str,
    pub callbacks_object: &'a str,
    pub bridge_name: &'a str,
    pub doc: &'a Option<String>,
    pub sync_methods: &'a [super::plan::KotlinCallbackMethod],
    pub async_methods: &'a [super::plan::KotlinAsyncCallbackMethod],
}

#[derive(Template)]
#[template(path = "render_kotlin/closure_interface.txt", escape = "none")]
pub struct ClosureInterfaceTemplate<'a> {
    pub interface_name: &'a str,
    pub params: &'a [super::plan::KotlinSignatureParam],
    pub return_type: &'a str,
    pub is_void_return: bool,
}

pub struct KotlinEmitter;

impl KotlinEmitter {
    pub fn emit(module: &KotlinModule) -> String {
        let preamble = PreambleTemplate {
            package_name: &module.package_name,
            prefix: &module.prefix,
            extra_imports: &module.extra_imports,
            custom_types: &module.custom_types,
            has_streams: module.has_streams,
        }
        .render()
        .unwrap();

        let mut declarations = Vec::new();

        module.enums.iter().for_each(|enumeration| {
            let rendered = if enumeration.is_c_style() && !enumeration.is_error() {
                CStyleEnumTemplate {
                    class_name: &enumeration.class_name,
                    variants: &enumeration.variants,
                }
                .render()
                .unwrap()
            } else {
                SealedEnumTemplate {
                    class_name: &enumeration.class_name,
                    variants: &enumeration.variants,
                    is_error: enumeration.is_error(),
                }
                .render()
                .unwrap()
            };
            declarations.push(rendered);
        });

        module.data_enum_codecs.iter().for_each(|codec| {
            let rendered = DataEnumCodecTemplate {
                class_name: &codec.class_name,
                codec_name: &codec.codec_name,
                struct_size: codec.struct_size,
                payload_offset: codec.payload_offset,
                variants: &codec.variants,
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        module.records.iter().for_each(|record| {
            let rendered = RecordTemplate {
                class_name: &record.class_name,
                fields: &record.fields,
                is_blittable: record.is_blittable,
                struct_size: record.struct_size,
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        module.record_readers.iter().for_each(|reader| {
            let rendered = RecordReaderTemplate {
                reader_name: &reader.reader_name,
                class_name: &reader.class_name,
                struct_size: reader.struct_size,
                fields: &reader.fields,
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        module.record_writers.iter().for_each(|writer| {
            let rendered = RecordWriterTemplate {
                writer_name: &writer.writer_name,
                class_name: &writer.class_name,
                struct_size: writer.struct_size,
                fields: &writer.fields,
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        module.closures.iter().for_each(|closure| {
            let rendered = ClosureInterfaceTemplate {
                interface_name: &closure.interface_name,
                params: &closure.params,
                return_type: closure.return_type(),
                is_void_return: closure.is_void_return(),
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        module.functions.iter().for_each(|function| {
            let rendered = if function.is_async() {
                let async_call = function.async_call.as_ref().unwrap();
                AsyncFunctionTemplate {
                    func_name: &function.func_name,
                    signature_params: &function.signature_params,
                    return_type: function.return_type.as_deref(),
                    wire_writers: &function.wire_writers,
                    wire_writer_closes: &function.wire_writer_closes,
                    native_args: &function.native_args,
                    throws: function.throws,
                    err_type: &function.err_type,
                    ffi_name: &function.ffi_name,
                    include_handle: false,
                    ffi_poll: &async_call.poll,
                    ffi_complete: &async_call.complete,
                    ffi_cancel: &async_call.cancel,
                    ffi_free: &async_call.free,
                    return_abi: &async_call.return_abi,
                    decode_expr: &function.decode_expr,
                    is_blittable_return: function.is_blittable_return,
                }
                .render()
                .unwrap()
            } else {
                WireFunctionTemplate {
                    func_name: &function.func_name,
                    signature_params: &function.signature_params,
                    return_type: function.return_type.as_deref(),
                    wire_writers: &function.wire_writers,
                    wire_writer_closes: &function.wire_writer_closes,
                    native_args: &function.native_args,
                    throws: function.throws,
                    err_type: &function.err_type,
                    ffi_name: &function.ffi_name,
                    return_abi: &function.return_abi,
                    decode_expr: &function.decode_expr,
                    is_blittable_return: function.is_blittable_return,
                }
                .render()
                .unwrap()
            };
            declarations.push(rendered);
        });

        module.classes.iter().for_each(|class| {
            let rendered = ClassTemplate {
                class_name: &class.class_name,
                doc: &class.doc,
                constructors: &class.constructors,
                methods: &class.methods,
                streams: &class.streams,
                use_companion_methods: class.use_companion_methods,
                has_factory_ctors: class.has_factory_ctors(),
                prefix: &class.prefix,
                ffi_free: &class.ffi_free,
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        module.callbacks.iter().for_each(|callback| {
            let rendered = CallbackTraitTemplate {
                interface_name: &callback.interface_name,
                handle_map_name: &callback.handle_map_name,
                callbacks_object: &callback.callbacks_object,
                bridge_name: &callback.bridge_name,
                doc: &callback.doc,
                sync_methods: &callback.sync_methods,
                async_methods: &callback.async_methods,
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        let native = NativeTemplate {
            lib_name: &module.native.lib_name,
            prefix: &module.native.prefix,
            functions: &module.native.functions,
            wire_functions: &module.native.wire_functions,
            classes: &module.native.classes,
            async_callback_invokers: &module.native.async_callback_invokers,
        }
        .render()
        .unwrap();

        let rendered_declarations = match module.api_style {
            super::plan::KotlinApiStyle::TopLevel => declarations
                .iter()
                .map(|section| section.trim().to_string())
                .filter(|section| !section.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n"),
            super::plan::KotlinApiStyle::ModuleObject => {
                let object_name = module
                    .module_object_name
                    .clone()
                    .unwrap_or_else(|| "RiffModule".to_string());
                format!(
                    "object {} {{\n{}\n}}",
                    object_name,
                    declarations
                        .iter()
                        .map(|section| section.trim().to_string())
                        .filter(|section| !section.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n\n")
                )
            }
        };

        let mut output = [preamble, rendered_declarations, native]
            .into_iter()
            .map(|section| section.trim().to_string())
            .filter(|section| !section.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        output.push('\n');
        output
    }
}
