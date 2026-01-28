use std::collections::{HashMap, HashSet};

use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiCallbackMethod, AbiContract, AbiEnum, AbiEnumField,
    AbiEnumPayload, AbiEnumVariant, AbiParam, AbiRecord, AbiStream, AsyncResultTransport, CallId,
    CallMode, ErrorTransport, ParamRole, ReturnTransport, StreamItemTransport,
};
use crate::ir::codec::VecLayout;
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{
    CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef, CustomTypeDef,
    EnumDef, EnumRepr, FieldDef, FunctionDef, MethodDef, ParamDef, RecordDef, ReturnDef, StreamDef,
    StreamMode, VariantPayload,
};
use crate::ir::ids::{
    BuiltinId, CallbackId, ClassId, CustomTypeId, EnumId, FieldName, MethodId, ParamName, RecordId,
};
use crate::ir::ops::{FieldReadOp, OffsetExpr, ReadOp, ReadSeq, SizeExpr, WriteOp, WriteSeq};
use crate::ir::plan::AbiType;
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::kotlin::{
    FactoryStyle, KotlinApiStyle as KotlinInputApiStyle, KotlinOptions, NamingConvention,
};
use crate::render::kotlin::emit;
use crate::render::kotlin::plan::*;
use crate::render::kotlin::templates::{AsyncMethodTemplate, WireMethodTemplate};
use askama::Template;
use riff_ffi_rules::naming;

pub struct KotlinLowerer<'a> {
    contract: &'a FfiContract,
    abi: &'a AbiContract,
    package_name: String,
    module_name: String,
    options: KotlinOptions,
}

impl<'a> KotlinLowerer<'a> {
    pub fn new(
        contract: &'a FfiContract,
        abi: &'a AbiContract,
        package_name: String,
        module_name: String,
        options: KotlinOptions,
    ) -> Self {
        Self {
            contract,
            abi,
            package_name,
            module_name,
            options,
        }
    }

    pub fn lower(&self) -> KotlinModule {
        let has_streams = self
            .contract
            .catalog
            .all_classes()
            .any(|class| !class.streams.is_empty());
        let preamble = self.lower_preamble(has_streams);
        let enums = self
            .contract
            .catalog
            .all_enums()
            .map(|e| self.lower_enum(e))
            .collect::<Vec<_>>();
        let data_enum_codecs = self
            .contract
            .catalog
            .all_enums()
            .filter(|e| self.should_generate_fixed_enum_codec(e))
            .map(|e| self.lower_data_enum_codec(e))
            .collect::<Vec<_>>();
        let records = self
            .contract
            .catalog
            .all_records()
            .map(|r| self.lower_record(r))
            .collect::<Vec<_>>();
        let record_readers = self.lower_record_readers();
        let record_writers = self.lower_record_writers();
        let closures = self.lower_closures();
        let functions = self
            .contract
            .functions
            .iter()
            .map(|function| self.lower_function(function))
            .collect::<Vec<_>>();
        let classes = self
            .contract
            .catalog
            .all_classes()
            .map(|class| self.lower_class(class))
            .collect::<Vec<_>>();
        let callbacks = self
            .contract
            .catalog
            .all_callbacks()
            .map(|c| self.lower_callback_trait(c))
            .collect::<Vec<_>>();
        let native = self.lower_native();

        KotlinModule {
            package_name: self.package_name.clone(),
            prefix: preamble.prefix,
            extra_imports: preamble.extra_imports,
            custom_types: preamble.custom_types,
            enums,
            data_enum_codecs,
            records,
            record_readers,
            record_writers,
            closures,
            functions,
            classes,
            callbacks,
            native,
            api_style: match self.options.api_style {
                KotlinInputApiStyle::TopLevel => KotlinApiStyle::TopLevel,
                KotlinInputApiStyle::ModuleObject => KotlinApiStyle::ModuleObject,
            },
            module_object_name: self.options.module_object_name.clone(),
            has_streams,
        }
    }

    fn lower_preamble(&self, has_streams: bool) -> KotlinPreamble {
        let extra_imports = self.collect_extra_imports(has_streams);
        let custom_types = self
            .contract
            .catalog
            .all_custom_types()
            .map(|custom| self.lower_custom_type(custom))
            .collect::<Vec<_>>();

        KotlinPreamble {
            prefix: naming::ffi_prefix().to_string(),
            extra_imports,
            custom_types,
            has_streams,
        }
    }

    fn collect_extra_imports(&self, has_streams: bool) -> Vec<String> {
        let mut imports = self
            .collect_builtin_ids()
            .into_iter()
            .filter_map(|id| self.builtin_import(&id))
            .collect::<Vec<_>>();
        let has_async_callbacks = self
            .contract
            .catalog
            .all_callbacks()
            .any(|callback| callback.methods.iter().any(|method| method.is_async));
        let coroutine_imports = if has_async_callbacks || has_streams {
            vec![
                "kotlinx.coroutines.DelicateCoroutinesApi".to_string(),
                "kotlinx.coroutines.GlobalScope".to_string(),
                "kotlinx.coroutines.launch".to_string(),
            ]
        } else {
            Vec::new()
        };
        let stream_imports = if has_streams {
            vec![
                "java.util.concurrent.atomic.AtomicInteger".to_string(),
                "kotlinx.coroutines.CoroutineScope".to_string(),
                "kotlinx.coroutines.channels.awaitClose".to_string(),
                "kotlinx.coroutines.flow.Flow".to_string(),
                "kotlinx.coroutines.flow.callbackFlow".to_string(),
            ]
        } else {
            Vec::new()
        };
        coroutine_imports
            .into_iter()
            .chain(stream_imports)
            .for_each(|import| {
                if !imports.iter().any(|item| item == &import) {
                    imports.push(import);
                }
            });
        imports
    }

    fn collect_builtin_ids(&self) -> HashSet<BuiltinId> {
        let mut used = HashSet::new();
        self.contract
            .functions
            .iter()
            .for_each(|function| self.collect_builtins_from_function(function, &mut used));
        self.contract.catalog.all_classes().for_each(|class| {
            class
                .constructors
                .iter()
                .for_each(|ctor| self.collect_builtins_from_constructor(ctor, &mut used));
            class
                .methods
                .iter()
                .for_each(|method| self.collect_builtins_from_method(method, &mut used));
            class
                .streams
                .iter()
                .for_each(|stream| self.collect_builtins_from_type(&stream.item_type, &mut used));
        });
        self.contract.catalog.all_records().for_each(|record| {
            record
                .fields
                .iter()
                .for_each(|field| self.collect_builtins_from_type(&field.type_expr, &mut used))
        });
        self.contract.catalog.all_enums().for_each(|enumeration| {
            if let EnumRepr::Data { variants, .. } = &enumeration.repr {
                variants.iter().for_each(|variant| match &variant.payload {
                    VariantPayload::Struct(fields) => fields.iter().for_each(|field| {
                        self.collect_builtins_from_type(&field.type_expr, &mut used)
                    }),
                    VariantPayload::Tuple(fields) => fields
                        .iter()
                        .for_each(|ty| self.collect_builtins_from_type(ty, &mut used)),
                    VariantPayload::Unit => {}
                })
            }
        });
        self.contract
            .catalog
            .all_custom_types()
            .for_each(|custom| self.collect_builtins_from_type(&custom.repr, &mut used));
        self.contract.catalog.all_callbacks().for_each(|callback| {
            callback.methods.iter().for_each(|method| {
                method
                    .params
                    .iter()
                    .for_each(|param| self.collect_builtins_from_type(&param.type_expr, &mut used));
                self.collect_builtins_from_return(&method.returns, &mut used);
            })
        });
        used
    }

    fn collect_builtins_from_function(&self, func: &FunctionDef, used: &mut HashSet<BuiltinId>) {
        func.params
            .iter()
            .for_each(|param| self.collect_builtins_from_type(&param.type_expr, used));
        self.collect_builtins_from_return(&func.returns, used);
    }

    fn collect_builtins_from_constructor(
        &self,
        ctor: &ConstructorDef,
        used: &mut HashSet<BuiltinId>,
    ) {
        ctor.params()
            .iter()
            .for_each(|param| self.collect_builtins_from_type(&param.type_expr, used));
    }

    fn collect_builtins_from_method(&self, method: &MethodDef, used: &mut HashSet<BuiltinId>) {
        method
            .params
            .iter()
            .for_each(|param| self.collect_builtins_from_type(&param.type_expr, used));
        self.collect_builtins_from_return(&method.returns, used);
    }

    fn collect_builtins_from_return(&self, returns: &ReturnDef, used: &mut HashSet<BuiltinId>) {
        match returns {
            ReturnDef::Void => {}
            ReturnDef::Value(ty) => self.collect_builtins_from_type(ty, used),
            ReturnDef::Result { ok, err } => {
                self.collect_builtins_from_type(ok, used);
                self.collect_builtins_from_type(err, used);
            }
        }
    }

    fn collect_builtins_from_type(&self, ty: &TypeExpr, used: &mut HashSet<BuiltinId>) {
        match ty {
            TypeExpr::Builtin(id) => {
                used.insert(id.clone());
            }
            TypeExpr::Option(inner) | TypeExpr::Vec(inner) => {
                self.collect_builtins_from_type(inner, used)
            }
            TypeExpr::Result { ok, err } => {
                self.collect_builtins_from_type(ok, used);
                self.collect_builtins_from_type(err, used);
            }
            _ => {}
        }
    }

    fn builtin_import(&self, id: &BuiltinId) -> Option<String> {
        match id.as_str() {
            "Duration" => Some("java.time.Duration".to_string()),
            "SystemTime" => Some("java.time.Instant".to_string()),
            "Uuid" => Some("java.util.UUID".to_string()),
            "Url" => Some("java.net.URI".to_string()),
            _ => None,
        }
    }

    fn lower_custom_type(&self, custom: &CustomTypeDef) -> KotlinCustomType {
        let class_name = NamingConvention::class_name(custom.id.as_str());
        let repr_kotlin_type = self.kotlin_type(&custom.repr);
        let repr_decode_pair_expr =
            emit::emit_read_pair(&self.custom_read_seq(custom), "offset", "offset");
        let repr_encode_expr = emit::emit_write_expr(&self.custom_write_seq(custom), "repr");
        let repr_size_expr = emit::emit_size_expr(&self.custom_size_expr(custom));

        KotlinCustomType {
            class_name,
            repr_kotlin_type,
            repr_size_expr,
            repr_encode_expr,
            repr_decode_pair_expr,
        }
    }

    fn custom_read_seq(&self, custom: &CustomTypeDef) -> ReadSeq {
        self.find_custom_read_seq(&custom.id)
            .unwrap_or_else(|| self.read_seq_from_repr(&custom.repr))
    }

    fn custom_write_seq(&self, custom: &CustomTypeDef) -> WriteSeq {
        self.find_custom_write_seq(&custom.id)
            .unwrap_or_else(|| self.write_seq_from_repr(&custom.repr))
    }

    fn custom_size_expr(&self, custom: &CustomTypeDef) -> SizeExpr {
        self.custom_write_seq(custom).size
    }

    fn lower_enum(&self, enumeration: &EnumDef) -> KotlinEnum {
        let abi_enum = self.abi_enum_for(enumeration);
        let class_name = NamingConvention::class_name(enumeration.id.as_str());
        let variants = abi_enum
            .variants
            .iter()
            .map(|variant| self.lower_enum_variant(variant))
            .collect::<Vec<_>>();
        let kind = if enumeration.is_error {
            KotlinEnumKind::Error
        } else if abi_enum.is_c_style {
            KotlinEnumKind::CStyle
        } else {
            KotlinEnumKind::Sealed
        };
        KotlinEnum {
            class_name,
            variants,
            kind,
        }
    }

    fn lower_enum_variant(&self, variant: &AbiEnumVariant) -> KotlinEnumVariant {
        let fields = match &variant.payload {
            AbiEnumPayload::Unit => Vec::new(),
            AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => fields
                .iter()
                .map(|field| self.lower_enum_field(field))
                .collect(),
        };
        KotlinEnumVariant {
            name: NamingConvention::class_name(variant.name.as_str()),
            tag: variant.discriminant,
            fields,
        }
    }

    fn lower_enum_field(&self, field: &AbiEnumField) -> KotlinEnumField {
        let local_name = format!("_{}_", field.name.as_str().to_lowercase());
        KotlinEnumField {
            name: NamingConvention::property_name(field.name.as_str()),
            kotlin_type: self.kotlin_type(&field.type_expr),
            local_name: local_name.clone(),
            wire_decode_inline: emit::emit_inline_decode(&field.decode, &local_name, "pos"),
            wire_size_expr: emit::emit_size_expr(&field.decode.size),
            wire_encode: emit::emit_write_expr(&field.encode, field.name.as_str()),
        }
    }

    fn lower_data_enum_codec(&self, enumeration: &EnumDef) -> KotlinDataEnumCodec {
        let layout = self.data_enum_layout(enumeration);
        let class_name = NamingConvention::class_name(enumeration.id.as_str());
        let codec_name = format!("{}Codec", class_name);
        let variants = match (&enumeration.repr, &layout) {
            (EnumRepr::Data { variants, .. }, Some(layout)) => variants
                .iter()
                .enumerate()
                .map(|(index, variant)| KotlinDataEnumVariant {
                    name: NamingConvention::class_name(variant.name.as_str()),
                    const_name: variant.name.as_str().to_uppercase(),
                    tag_value: variant.discriminant,
                    fields: self.lower_data_enum_codec_fields(
                        &variant.payload,
                        layout.variant_offsets.get(index),
                    ),
                })
                .collect(),
            _ => Vec::new(),
        };
        KotlinDataEnumCodec {
            class_name,
            codec_name,
            struct_size: layout.as_ref().map(|l| l.struct_size).unwrap_or(0),
            payload_offset: layout.as_ref().map(|l| l.payload_offset).unwrap_or(0),
            variants,
        }
    }

    fn lower_data_enum_codec_fields(
        &self,
        payload: &VariantPayload,
        offsets: Option<&Vec<usize>>,
    ) -> Vec<KotlinDataEnumField> {
        match (payload, offsets) {
            (VariantPayload::Struct(fields), Some(offsets)) => fields
                .iter()
                .zip(offsets.iter().copied())
                .filter_map(|(field, offset)| match field.type_expr {
                    TypeExpr::Primitive(primitive) => {
                        let (getter, putter, conversion) =
                            self.primitive_field_accessors(primitive);
                        let value_expr = self.primitive_write_value_expr(
                            primitive,
                            &format!(
                                "value.{}",
                                NamingConvention::property_name(field.name.as_str())
                            ),
                        );
                        Some(KotlinDataEnumField {
                            param_name: field.name.as_str().to_string(),
                            value_expr,
                            offset,
                            getter,
                            putter,
                            conversion,
                        })
                    }
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn lower_record(&self, record: &RecordDef) -> KotlinRecord {
        let class_name = NamingConvention::class_name(record.id.as_str());
        let fields = record
            .fields
            .iter()
            .map(|field| self.lower_record_field(record, field))
            .collect::<Vec<_>>();
        KotlinRecord {
            class_name,
            fields,
            is_blittable: record.is_blittable(),
            struct_size: self.record_struct_size(record.id.as_str()),
        }
    }

    fn lower_record_field(&self, record: &RecordDef, field: &FieldDef) -> KotlinRecordField {
        let kotlin_name = NamingConvention::property_name(field.name.as_str());
        let local_name = format!("_{}_", field.name.as_str().to_lowercase());
        let decode_seq = self
            .record_field_read_seq(&record.id, &field.name)
            .expect("record field decode ops");
        KotlinRecordField {
            name: kotlin_name.clone(),
            kotlin_type: self.kotlin_type(&field.type_expr),
            default_value: self.default_expr(&field.type_expr),
            read_expr: emit::emit_read_value(&decode_seq, "offset", "offset"),
            local_name: local_name.clone(),
            wire_decode_inline: emit::emit_inline_decode(&decode_seq, &local_name, "pos"),
            wire_size_expr: emit::emit_size_expr(&decode_seq.size),
            wire_encode: emit::emit_write_expr(
                &self
                    .record_field_write_seq(&record.id, &field.name)
                    .expect("record field encode ops"),
                &kotlin_name,
            ),
            padding_after: self.field_padding_after(&record.id, &field.name),
        }
    }

    fn lower_record_readers(&self) -> Vec<KotlinRecordReader> {
        let record_ids = self.blittable_vec_return_records();
        self.contract
            .catalog
            .all_records()
            .filter(|record| record_ids.contains(record.id.as_str()))
            .filter_map(|record| {
                let fields = self.record_blittable_fields(&record.id)?;
                let reader_name =
                    format!("{}Reader", NamingConvention::class_name(record.id.as_str()));
                Some(KotlinRecordReader {
                    reader_name,
                    class_name: NamingConvention::class_name(record.id.as_str()),
                    struct_size: self.record_struct_size(record.id.as_str()),
                    fields: fields
                        .iter()
                        .map(|field| {
                            let (getter, _, conversion) =
                                self.primitive_field_accessors(field.primitive);
                            KotlinRecordReaderField {
                                name: NamingConvention::property_name(field.name.as_str()),
                                const_name: field.name.as_str().to_uppercase(),
                                offset: field.offset,
                                getter,
                                conversion,
                            }
                        })
                        .collect(),
                })
            })
            .collect()
    }

    fn lower_record_writers(&self) -> Vec<KotlinRecordWriter> {
        let record_ids = self.blittable_vec_param_records();
        self.contract
            .catalog
            .all_records()
            .filter(|record| record_ids.contains(record.id.as_str()))
            .filter_map(|record| {
                let fields = self.record_blittable_fields(&record.id)?;
                let writer_name =
                    format!("{}Writer", NamingConvention::class_name(record.id.as_str()));
                Some(KotlinRecordWriter {
                    writer_name,
                    class_name: NamingConvention::class_name(record.id.as_str()),
                    struct_size: self.record_struct_size(record.id.as_str()),
                    fields: fields
                        .iter()
                        .map(|field| {
                            let (_, putter, _) = self.primitive_field_accessors(field.primitive);
                            let value_expr = self.primitive_write_value_expr(
                                field.primitive,
                                &format!(
                                    "item.{}",
                                    NamingConvention::property_name(field.name.as_str())
                                ),
                            );
                            KotlinRecordWriterField {
                                const_name: field.name.as_str().to_uppercase(),
                                offset: field.offset,
                                putter,
                                value_expr,
                            }
                        })
                        .collect(),
                })
            })
            .collect()
    }

    fn lower_closures(&self) -> Vec<KotlinClosureInterface> {
        self.contract
            .catalog
            .all_callbacks()
            .filter(|callback| matches!(callback.kind, CallbackKind::Closure))
            .filter_map(|callback| callback.methods.first().map(|method| (callback, method)))
            .map(|(callback, method)| KotlinClosureInterface {
                interface_name: self.closure_interface_name(callback.id.as_str()),
                params: method
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, param)| KotlinSignatureParam {
                        name: format!("p{}", index),
                        kotlin_type: self.closure_param_type(&param.type_expr),
                    })
                    .collect(),
                return_type: match &method.returns {
                    ReturnDef::Void => None,
                    _ => Some(self.kotlin_type_from_return_def(&method.returns)),
                },
            })
            .collect()
    }

    fn lower_function(&self, func: &FunctionDef) -> KotlinFunction {
        let call = self.abi_call_for_function(func);
        let signature_params = func
            .params
            .iter()
            .map(|param| KotlinSignatureParam {
                name: NamingConvention::param_name(param.name.as_str()),
                kotlin_type: self.kotlin_type(&param.type_expr),
            })
            .collect::<Vec<_>>();
        let wire_writers = self.wire_writers_for_params(call);
        let wire_writer_closes: Vec<String> = wire_writers
            .iter()
            .map(|w| w.binding_name.clone())
            .collect();
        let native_args = self.native_args_for_params(call, &wire_writers);
        let return_type = self.kotlin_return_type_from_def(&func.returns, &call.return_);
        let return_abi = self.kotlin_return_abi(&call.return_);
        let decode_expr = self.decode_expr_for_call_return(&call.return_, &func.returns);
        let is_blittable_return = self.is_blittable_return(&call.return_);
        let async_call = match &call.mode {
            CallMode::Async(_) => Some(self.async_call_for_function(func, call)),
            CallMode::Sync => None,
        };
        KotlinFunction {
            func_name: NamingConvention::method_name(func.id.as_str()),
            signature_params,
            return_type,
            wire_writers,
            wire_writer_closes,
            native_args,
            throws: self.is_throwing_return(&func.returns),
            err_type: self.error_type_name(&func.returns),
            ffi_name: call.symbol.as_str().to_string(),
            return_abi,
            async_call,
            decode_expr,
            is_blittable_return,
        }
    }

    fn lower_class(&self, class: &ClassDef) -> KotlinClass {
        let class_name = NamingConvention::class_name(class.id.as_str());
        let constructors = class
            .constructors
            .iter()
            .enumerate()
            .map(|(index, ctor)| self.lower_constructor(class, ctor, index))
            .collect::<Vec<_>>();
        let methods = class
            .methods
            .iter()
            .map(|method| self.lower_method(class, method))
            .collect::<Vec<_>>();
        let streams = class
            .streams
            .iter()
            .map(|stream| {
                let abi_stream = self.abi_stream(class, stream);
                self.lower_stream(stream, abi_stream, &class_name)
            })
            .collect::<Vec<_>>();
        KotlinClass {
            class_name,
            doc: class.doc.clone(),
            prefix: naming::ffi_prefix().to_string(),
            ffi_free: naming::class_ffi_free(class.id.as_str()).into_string(),
            constructors,
            methods,
            streams,
            use_companion_methods: matches!(
                self.options.factory_style,
                FactoryStyle::CompanionMethods
            ),
        }
    }

    fn lower_constructor(
        &self,
        class: &ClassDef,
        ctor: &ConstructorDef,
        index: usize,
    ) -> KotlinConstructor {
        let call = self.abi_call_for_constructor(class, index);
        let (name, is_factory) = match ctor {
            ConstructorDef::Default { .. } => ("new".to_string(), false),
            ConstructorDef::NamedFactory { name, .. } => (name.as_str().to_string(), true),
            ConstructorDef::NamedInit { name, .. } => (name.as_str().to_string(), true),
        };
        let signature_params = ctor
            .params()
            .iter()
            .map(|param| KotlinSignatureParam {
                name: NamingConvention::param_name(param.name.as_str()),
                kotlin_type: self.kotlin_type(&param.type_expr),
            })
            .collect::<Vec<_>>();
        let wire_writers = self.wire_writers_for_params(call);
        let wire_writer_closes: Vec<String> = wire_writers
            .iter()
            .map(|w| w.binding_name.clone())
            .collect();
        let native_args = self.native_args_for_params(call, &wire_writers);
        KotlinConstructor {
            name: NamingConvention::method_name(&name),
            is_factory,
            is_fallible: ctor.is_fallible(),
            signature_params,
            wire_writers,
            wire_writer_closes,
            native_args,
            ffi_name: call.symbol.as_str().to_string(),
        }
    }

    fn lower_method(&self, class: &ClassDef, method: &MethodDef) -> KotlinMethod {
        let call = self.abi_call_for_method(class, method);
        let wire_writers = self.wire_writers_for_params(call);
        let wire_writer_closes: Vec<String> = wire_writers
            .iter()
            .map(|w| w.binding_name.clone())
            .collect();
        let native_args = self.native_args_for_params(call, &wire_writers);
        let signature_params = method
            .params
            .iter()
            .map(|param| KotlinSignatureParam {
                name: NamingConvention::param_name(param.name.as_str()),
                kotlin_type: self.kotlin_type(&param.type_expr),
            })
            .collect::<Vec<_>>();
        let return_type = self.kotlin_return_type_from_def(&method.returns, &call.return_);
        let return_abi = self.kotlin_return_abi(&call.return_);
        let decode_expr = self.decode_expr_for_call_return(&call.return_, &method.returns);
        let is_blittable_return = self.is_blittable_return(&call.return_);
        let ffi_name = call.symbol.as_str().to_string();
        let include_handle = true;
        let err_type = self.error_type_name(&method.returns);
        let rendered = if method.is_async {
            let async_call = self.async_call_for_method(class, method, call);
            let async_decode_expr = self.async_call_decode_expr(method, call);
            let is_blittable_async = self.is_blittable_async_return(call);
            AsyncMethodTemplate {
                method_name: &NamingConvention::method_name(method.id.as_str()),
                signature_params: &signature_params,
                return_type: return_type.as_deref(),
                wire_writers: &wire_writers,
                wire_writer_closes: &wire_writer_closes,
                native_args: &native_args,
                throws: self.is_throwing_return(&method.returns),
                err_type: &err_type,
                ffi_name: &ffi_name,
                include_handle,
                ffi_poll: &async_call.poll,
                ffi_complete: &async_call.complete,
                ffi_cancel: &async_call.cancel,
                ffi_free: &async_call.free,
                return_abi: &async_call.return_abi,
                decode_expr: &async_decode_expr,
                is_blittable_return: is_blittable_async,
            }
            .render()
            .unwrap()
        } else {
            WireMethodTemplate {
                method_name: &NamingConvention::method_name(method.id.as_str()),
                signature_params: &signature_params,
                return_type: return_type.as_deref(),
                wire_writers: &wire_writers,
                wire_writer_closes: &wire_writer_closes,
                native_args: &native_args,
                throws: self.is_throwing_return(&method.returns),
                err_type: &err_type,
                ffi_name: &ffi_name,
                return_abi: &return_abi,
                decode_expr: &decode_expr,
                is_blittable_return,
                include_handle,
            }
            .render()
            .unwrap()
        };
        KotlinMethod {
            impl_: if method.is_async {
                KotlinMethodImpl::AsyncMethod(rendered)
            } else {
                KotlinMethodImpl::SyncMethod(rendered)
            },
        }
    }

    fn lower_stream(
        &self,
        stream_def: &StreamDef,
        stream: &AbiStream,
        class_name: &str,
    ) -> KotlinStream {
        let StreamItemTransport::WireEncoded { decode_ops } = &stream.item;
        let method_name_pascal = NamingConvention::class_name(stream.stream_id.as_str());
        let mode = match stream.mode {
            StreamMode::Async => KotlinStreamMode::Async,
            StreamMode::Batch => KotlinStreamMode::Batch {
                class_name: class_name.to_string(),
                method_name_pascal: method_name_pascal.clone(),
            },
            StreamMode::Callback => KotlinStreamMode::Callback {
                class_name: class_name.to_string(),
                method_name_pascal: method_name_pascal.clone(),
            },
        };
        KotlinStream {
            name: NamingConvention::method_name(stream.stream_id.as_str()),
            mode,
            item_type: self.kotlin_type(&stream_def.item_type),
            item_decode: self.rebase_read_seq(decode_ops, "pos", "0"),
            subscribe: stream.subscribe.to_string(),
            poll: stream.poll.to_string(),
            pop_batch: stream.pop_batch.to_string(),
            wait: stream.wait.to_string(),
            unsubscribe: stream.unsubscribe.to_string(),
            free: stream.free.to_string(),
            free_buf: format!("{}_free_native_buffer", naming::ffi_prefix()),
        }
    }

    fn abi_stream<'b>(&'b self, class: &ClassDef, stream: &StreamDef) -> &'b AbiStream {
        self.abi
            .streams
            .iter()
            .find(|item| item.class_id == class.id && item.stream_id == stream.id)
            .expect("abi stream")
    }

    fn rebase_read_seq(&self, seq: &ReadSeq, old_base: &str, new_base: &str) -> ReadSeq {
        ReadSeq {
            size: seq.size.clone(),
            ops: seq
                .ops
                .iter()
                .map(|op| self.rebase_read_op(op, old_base, new_base))
                .collect(),
            shape: seq.shape,
        }
    }

    fn rebase_read_op(&self, op: &ReadOp, old_base: &str, new_base: &str) -> ReadOp {
        match op {
            ReadOp::Primitive { primitive, offset } => ReadOp::Primitive {
                primitive: *primitive,
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::String { offset } => ReadOp::String {
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::Bytes { offset } => ReadOp::Bytes {
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::Option { tag_offset, some } => ReadOp::Option {
                tag_offset: self.rebase_offset_expr(tag_offset, old_base, new_base),
                some: Box::new(self.rebase_read_seq(some, old_base, new_base)),
            },
            ReadOp::Vec {
                len_offset,
                element_type,
                element,
                layout,
            } => ReadOp::Vec {
                len_offset: self.rebase_offset_expr(len_offset, old_base, new_base),
                element_type: element_type.clone(),
                element: Box::new(self.rebase_read_seq(element, old_base, new_base)),
                layout: layout.clone(),
            },
            ReadOp::Record { id, offset, fields } => ReadOp::Record {
                id: id.clone(),
                offset: self.rebase_offset_expr(offset, old_base, new_base),
                fields: fields
                    .iter()
                    .map(|field| {
                        let seq = self.rebase_read_seq(&field.seq, old_base, new_base);
                        FieldReadOp {
                            name: field.name.clone(),
                            seq,
                        }
                    })
                    .collect(),
            },
            ReadOp::Enum { id, offset, layout } => ReadOp::Enum {
                id: id.clone(),
                offset: self.rebase_offset_expr(offset, old_base, new_base),
                layout: layout.clone(),
            },
            ReadOp::Result {
                tag_offset,
                ok,
                err,
            } => ReadOp::Result {
                tag_offset: self.rebase_offset_expr(tag_offset, old_base, new_base),
                ok: Box::new(self.rebase_read_seq(ok, old_base, new_base)),
                err: Box::new(self.rebase_read_seq(err, old_base, new_base)),
            },
            ReadOp::Builtin { id, offset } => ReadOp::Builtin {
                id: id.clone(),
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::Custom { id, underlying } => ReadOp::Custom {
                id: id.clone(),
                underlying: Box::new(self.rebase_read_seq(underlying, old_base, new_base)),
            },
        }
    }

    fn rebase_offset_expr(
        &self,
        offset: &OffsetExpr,
        old_base: &str,
        new_base: &str,
    ) -> OffsetExpr {
        match offset {
            OffsetExpr::Fixed(value) => OffsetExpr::Fixed(*value),
            OffsetExpr::Base => OffsetExpr::Base,
            OffsetExpr::BasePlus(add) => OffsetExpr::BasePlus(*add),
            OffsetExpr::Var(name) => {
                if name == old_base {
                    OffsetExpr::Var(new_base.to_string())
                } else {
                    OffsetExpr::Var(name.clone())
                }
            }
            OffsetExpr::VarPlus(name, add) => {
                if name == old_base {
                    OffsetExpr::VarPlus(new_base.to_string(), *add)
                } else {
                    OffsetExpr::VarPlus(name.clone(), *add)
                }
            }
        }
    }

    fn lower_callback_trait(&self, callback: &CallbackTraitDef) -> KotlinCallbackTrait {
        let interface_name = NamingConvention::class_name(callback.id.as_str());
        let handle_map_name = format!("{}HandleMap", interface_name);
        let callbacks_object = format!("{}Callbacks", interface_name);
        let bridge_name = format!("{}Bridge", interface_name);
        let sync_methods = callback
            .methods
            .iter()
            .filter(|method| !method.is_async)
            .map(|method| self.lower_callback_method(callback, method))
            .collect();
        let async_methods = callback
            .methods
            .iter()
            .filter(|method| method.is_async)
            .map(|method| self.lower_async_callback_method(callback, method))
            .collect();
        KotlinCallbackTrait {
            interface_name,
            handle_map_name,
            callbacks_object,
            bridge_name,
            doc: callback.doc.clone(),
            sync_methods,
            async_methods,
        }
    }

    fn lower_callback_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
    ) -> KotlinCallbackMethod {
        let abi_method = self.abi_callback_method(&callback.id, &method.id);
        let param_map = method
            .params
            .iter()
            .map(|param| (param.name.clone(), param))
            .collect::<HashMap<_, _>>();
        let params = abi_method
            .params
            .iter()
            .filter(|param| {
                matches!(
                    param.role,
                    ParamRole::InDirect | ParamRole::InEncoded { .. }
                )
            })
            .map(|param| {
                let def = param_map.get(&param.name).expect("param def");
                self.lower_callback_param(def, param)
            })
            .collect();
        let return_info = self.callback_return_info(&method.returns, &abi_method.return_);
        KotlinCallbackMethod {
            name: NamingConvention::method_name(method.id.as_str()),
            ffi_name: abi_method.vtable_field.as_str().to_string(),
            params,
            return_info,
        }
    }

    fn lower_async_callback_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
    ) -> KotlinAsyncCallbackMethod {
        let abi_method = self.abi_callback_method(&callback.id, &method.id);
        let param_map = method
            .params
            .iter()
            .map(|param| (param.name.clone(), param))
            .collect::<HashMap<_, _>>();
        let params = abi_method
            .params
            .iter()
            .filter(|param| {
                matches!(
                    param.role,
                    ParamRole::InDirect | ParamRole::InEncoded { .. }
                )
            })
            .map(|param| {
                let def = param_map.get(&param.name).expect("param def");
                self.lower_callback_param(def, param)
            })
            .collect();
        let return_info = self.callback_return_info(&method.returns, &abi_method.return_);
        let invoker = self.async_callback_invoker(&method.returns);
        KotlinAsyncCallbackMethod {
            name: NamingConvention::method_name(method.id.as_str()),
            ffi_name: abi_method.vtable_field.as_str().to_string(),
            invoker_name: invoker.name,
            params,
            return_info,
        }
    }

    fn lower_callback_param(&self, def: &ParamDef, param: &AbiParam) -> KotlinCallbackParam {
        let name = NamingConvention::param_name(param.name.as_str());
        let kotlin_type = self.kotlin_type(&def.type_expr);
        match &param.role {
            ParamRole::InDirect => KotlinCallbackParam {
                name: name.clone(),
                kotlin_type,
                jni_type: self.jni_type_for_abi(&param.ffi_type),
                conversion: self.callback_direct_conversion(&def.type_expr, &name),
            },
            ParamRole::InEncoded { decode_ops, .. } => KotlinCallbackParam {
                name: name.clone(),
                kotlin_type,
                jni_type: "ByteBuffer".to_string(),
                conversion: self.callback_encoded_conversion(decode_ops, &name),
            },
            _ => panic!("unsupported callback param role"),
        }
    }

    fn async_callback_invoker(&self, returns: &ReturnDef) -> KotlinAsyncCallbackInvoker {
        let result_jni_type = match returns {
            ReturnDef::Value(TypeExpr::Primitive(p)) => Some(self.primitive_jni_type(*p)),
            ReturnDef::Value(TypeExpr::String) => Some("String".to_string()),
            ReturnDef::Value(TypeExpr::Enum(_)) => Some("Int".to_string()),
            ReturnDef::Value(TypeExpr::Handle(_)) | ReturnDef::Value(TypeExpr::Callback(_)) => {
                Some("Long".to_string())
            }
            ReturnDef::Value(TypeExpr::Record(_))
            | ReturnDef::Value(TypeExpr::Vec(_))
            | ReturnDef::Value(TypeExpr::Option(_))
            | ReturnDef::Value(TypeExpr::Bytes) => Some("ByteBuffer".to_string()),
            _ => None,
        };
        KotlinAsyncCallbackInvoker {
            name: format!("invokeAsyncCallback{}", self.invoker_suffix(returns)),
            result_jni_type,
        }
    }

    fn invoker_suffix(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Value(TypeExpr::Primitive(p)) => match p {
                PrimitiveType::Bool => "Bool".to_string(),
                PrimitiveType::I8 => "I8".to_string(),
                PrimitiveType::U8 => "I8".to_string(),
                PrimitiveType::I16 => "I16".to_string(),
                PrimitiveType::U16 => "I16".to_string(),
                PrimitiveType::I32 => "I32".to_string(),
                PrimitiveType::U32 => "I32".to_string(),
                PrimitiveType::I64 | PrimitiveType::ISize => "I64".to_string(),
                PrimitiveType::U64 | PrimitiveType::USize => "I64".to_string(),
                PrimitiveType::F32 => "F32".to_string(),
                PrimitiveType::F64 => "F64".to_string(),
            },
            ReturnDef::Value(TypeExpr::Handle(_)) | ReturnDef::Value(TypeExpr::Callback(_)) => {
                "I64".to_string()
            }
            _ => "Void".to_string(),
        }
    }

    fn callback_return_info(
        &self,
        returns: &ReturnDef,
        transport: &ReturnTransport,
    ) -> Option<KotlinCallbackReturn> {
        let kotlin_type = self.kotlin_return_type_from_def(returns, transport)?;
        let wrap_ok = matches!(returns, ReturnDef::Result { .. });
        let (jni_type, default_value, to_jni) = match transport {
            ReturnTransport::Void => return None,
            ReturnTransport::Direct(abi) => (
                self.jni_type_for_abi(abi),
                self.callback_default_value_for_abi(abi),
                self.callback_return_cast_for_abi(abi),
            ),
            ReturnTransport::Encoded { encode_ops, .. } => (
                "ByteBuffer".to_string(),
                "ByteBuffer.allocateDirect(0)".to_string(),
                self.callback_return_wire_encode(encode_ops, wrap_ok),
            ),
            ReturnTransport::Handle { class_id, nullable } => (
                "Long".to_string(),
                "0L".to_string(),
                self.callback_return_handle_cast(class_id, *nullable),
            ),
            ReturnTransport::Callback {
                callback_id,
                nullable,
            } => (
                "Long".to_string(),
                "0L".to_string(),
                self.callback_return_callback_cast(callback_id, *nullable),
            ),
        };
        Some(KotlinCallbackReturn {
            kotlin_type,
            jni_type,
            default_value,
            to_jni,
        })
    }

    fn callback_direct_conversion(&self, ty: &TypeExpr, name: &str) -> String {
        match ty {
            TypeExpr::Primitive(p) => match p {
                PrimitiveType::U8 => format!("{}.toUByte()", name),
                PrimitiveType::U16 => format!("{}.toUShort()", name),
                PrimitiveType::U32 => format!("{}.toUInt()", name),
                PrimitiveType::U64 | PrimitiveType::USize => format!("{}.toULong()", name),
                _ => name.to_string(),
            },
            _ => name.to_string(),
        }
    }

    fn callback_encoded_conversion(&self, decode_ops: &ReadSeq, name: &str) -> String {
        let decode_expr = emit::emit_read_value(decode_ops, "offset", "offset");
        format!(
            "run {{ val wire = WireBuffer.fromByteBuffer({}); val offset = 0; {} }}",
            name, decode_expr
        )
    }

    fn callback_return_cast_for_abi(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::U8 => ".toByte()".to_string(),
            AbiType::U16 => ".toShort()".to_string(),
            AbiType::U32 => ".toInt()".to_string(),
            AbiType::U64 | AbiType::USize => ".toLong()".to_string(),
            _ => String::new(),
        }
    }

    fn callback_default_value_for_abi(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::Bool => "false".to_string(),
            AbiType::I8 | AbiType::U8 => "0".to_string(),
            AbiType::I16 | AbiType::U16 => "0".to_string(),
            AbiType::I32 | AbiType::U32 => "0".to_string(),
            AbiType::I64 | AbiType::U64 | AbiType::ISize | AbiType::USize => "0L".to_string(),
            AbiType::F32 => "0f".to_string(),
            AbiType::F64 => "0.0".to_string(),
            _ => "0".to_string(),
        }
    }

    fn callback_return_handle_cast(&self, _class_id: &ClassId, nullable: bool) -> String {
        if nullable {
            "?.handle ?: 0L".to_string()
        } else {
            ".handle".to_string()
        }
    }

    fn callback_return_callback_cast(&self, callback_id: &CallbackId, nullable: bool) -> String {
        let bridge = format!(
            "{}Bridge",
            NamingConvention::class_name(callback_id.as_str())
        );
        if nullable {
            format!("?.let {{ {}.create(it) }} ?: 0L", bridge)
        } else {
            format!(".let {{ {}.create(it) }}", bridge)
        }
    }

    fn callback_return_wire_encode(&self, encode_ops: &WriteSeq, wrap_ok: bool) -> String {
        let size_expr = emit::emit_size_expr(&encode_ops.size);
        let encode_expr = emit::emit_write_expr(encode_ops, "value");
        let value_binding = if wrap_ok {
            "val value = RiffResult.Ok(value); "
        } else {
            ""
        };
        format!(
            ".let {{ value -> run {{ val writer = WireWriterPool.acquire({}); try {{ val wire = writer.writer; {}{}; writer.buffer }} finally {{ writer.close() }} }} }}",
            size_expr, value_binding, encode_expr
        )
    }

    fn lower_native(&self) -> KotlinNative {
        let functions = self
            .contract
            .functions
            .iter()
            .map(|func| self.lower_native_function(func))
            .collect::<Vec<_>>();
        let wire_functions = self
            .abi
            .calls
            .iter()
            .filter(|call| matches!(call.return_, ReturnTransport::Encoded { .. }))
            .map(|call| KotlinNativeWireFunction {
                ffi_name: call.symbol.as_str().to_string(),
                params: {
                    let len_params = self.len_param_names(call);
                    call.params
                        .iter()
                        .filter(|param| self.include_native_param(param, &len_params))
                        .map(|param| KotlinNativeParam {
                            name: param.name.as_str().to_string(),
                            jni_type: self.jni_type_for_param(param),
                        })
                        .collect()
                },
                return_jni_type: "ByteBuffer?".to_string(),
            })
            .collect::<Vec<_>>();
        let classes = self
            .contract
            .catalog
            .all_classes()
            .map(|class| self.lower_native_class(class))
            .collect::<Vec<_>>();
        let async_callback_invokers = self
            .contract
            .catalog
            .all_callbacks()
            .flat_map(|callback| {
                callback
                    .methods
                    .iter()
                    .filter(|method| method.is_async)
                    .map(|method| self.async_callback_invoker(&method.returns))
            })
            .collect();
        KotlinNative {
            lib_name: self
                .options
                .library_name
                .clone()
                .unwrap_or_else(|| self.contract.package.name.clone()),
            prefix: naming::ffi_prefix().to_string(),
            functions,
            wire_functions,
            classes,
            async_callback_invokers,
        }
    }

    fn lower_native_function(&self, func: &FunctionDef) -> KotlinNativeFunction {
        let call = self.abi_call_for_function(func);
        let return_jni_type = match &call.return_ {
            ReturnTransport::Void => "Unit".to_string(),
            ReturnTransport::Direct(abi) => self.jni_type_for_abi(abi),
            ReturnTransport::Encoded { .. } => "ByteBuffer?".to_string(),
            ReturnTransport::Handle { .. } | ReturnTransport::Callback { .. } => "Long".to_string(),
        };
        let complete_return_jni_type = match &call.mode {
            CallMode::Async(async_call) => match &async_call.result {
                AsyncResultTransport::Void => "Unit".to_string(),
                AsyncResultTransport::Direct(abi) => self.jni_type_for_abi(abi),
                AsyncResultTransport::Encoded { .. } => "ByteBuffer?".to_string(),
                AsyncResultTransport::Handle { .. } | AsyncResultTransport::Callback { .. } => {
                    "Long".to_string()
                }
            },
            CallMode::Sync => String::new(),
        };
        let async_ffi = match &call.mode {
            CallMode::Async(async_call) => Some(KotlinNativeAsyncFfi {
                ffi_poll: async_call.poll.as_str().to_string(),
                ffi_complete: async_call.complete.as_str().to_string(),
                ffi_cancel: async_call.cancel.as_str().to_string(),
                ffi_free: async_call.free.as_str().to_string(),
                complete_return_jni_type,
            }),
            CallMode::Sync => None,
        };
        KotlinNativeFunction {
            ffi_name: call.symbol.as_str().to_string(),
            params: {
                let len_params = self.len_param_names(call);
                call.params
                    .iter()
                    .filter(|param| self.include_native_param(param, &len_params))
                    .map(|param| KotlinNativeParam {
                        name: param.name.as_str().to_string(),
                        jni_type: self.jni_type_for_param(param),
                    })
                    .collect()
            },
            return_jni_type,
            async_ffi,
        }
    }

    fn lower_native_class(&self, class: &ClassDef) -> KotlinNativeClass {
        let ctors = class
            .constructors
            .iter()
            .enumerate()
            .map(|(index, _ctor)| {
                let call = self.abi_call_for_constructor(class, index);
                KotlinNativeCtor {
                    ffi_name: call.symbol.as_str().to_string(),
                    params: {
                        let len_params = self.len_param_names(call);
                        call.params
                            .iter()
                            .filter(|param| self.include_native_param(param, &len_params))
                            .map(|param| KotlinNativeParam {
                                name: param.name.as_str().to_string(),
                                jni_type: self.jni_type_for_param(param),
                            })
                            .collect()
                    },
                }
            })
            .collect();
        let async_methods = class
            .methods
            .iter()
            .filter(|m| m.is_async)
            .map(|method| {
                let call = self.abi_call_for_method(class, method);
                let async_call = match &call.mode {
                    CallMode::Async(async_call) => async_call,
                    CallMode::Sync => panic!("async method missing async call"),
                };
                KotlinNativeAsyncMethod {
                    ffi_name: call.symbol.as_str().to_string(),
                    ffi_poll: async_call.poll.as_str().to_string(),
                    ffi_complete: async_call.complete.as_str().to_string(),
                    ffi_cancel: async_call.cancel.as_str().to_string(),
                    ffi_free: async_call.free.as_str().to_string(),
                    include_handle: true,
                    params: {
                        let len_params = self.len_param_names(call);
                        call.params
                            .iter()
                            .filter(|param| self.include_native_param(param, &len_params))
                            .map(|param| KotlinNativeParam {
                                name: param.name.as_str().to_string(),
                                jni_type: self.jni_type_for_param(param),
                            })
                            .collect()
                    },
                    return_jni_type: match &async_call.result {
                        AsyncResultTransport::Void => "Unit".to_string(),
                        AsyncResultTransport::Direct(abi) => self.jni_type_for_abi(abi),
                        AsyncResultTransport::Encoded { .. } => "ByteBuffer?".to_string(),
                        AsyncResultTransport::Handle { .. }
                        | AsyncResultTransport::Callback { .. } => "Long".to_string(),
                    },
                }
            })
            .collect();
        let sync_methods = class
            .methods
            .iter()
            .filter(|m| !m.is_async)
            .map(|method| {
                let call = self.abi_call_for_method(class, method);
                KotlinNativeSyncMethod {
                    ffi_name: call.symbol.as_str().to_string(),
                    include_handle: true,
                    params: {
                        let len_params = self.len_param_names(call);
                        call.params
                            .iter()
                            .filter(|param| self.include_native_param(param, &len_params))
                            .map(|param| KotlinNativeParam {
                                name: param.name.as_str().to_string(),
                                jni_type: self.jni_type_for_param(param),
                            })
                            .collect()
                    },
                    return_jni_type: self.jni_type_for_return(&call.return_),
                }
            })
            .collect();
        let streams = class
            .streams
            .iter()
            .map(|stream| {
                let abi_stream = self.abi_stream(class, stream);
                KotlinNativeStream {
                    subscribe: abi_stream.subscribe.as_str().to_string(),
                    poll: abi_stream.poll.as_str().to_string(),
                    pop_batch: abi_stream.pop_batch.as_str().to_string(),
                    wait: abi_stream.wait.as_str().to_string(),
                    unsubscribe: abi_stream.unsubscribe.as_str().to_string(),
                    free: abi_stream.free.as_str().to_string(),
                }
            })
            .collect();
        KotlinNativeClass {
            ffi_free: naming::class_ffi_free(class.id.as_str()).into_string(),
            ctors,
            async_methods,
            sync_methods,
            streams,
        }
    }

    fn jni_type_for_return(&self, returns: &ReturnTransport) -> String {
        match returns {
            ReturnTransport::Void => "Unit".to_string(),
            ReturnTransport::Direct(abi) => self.jni_type_for_abi(abi),
            ReturnTransport::Encoded { .. } => "ByteBuffer?".to_string(),
            ReturnTransport::Handle { .. } | ReturnTransport::Callback { .. } => "Long".to_string(),
        }
    }

    fn jni_type_for_abi(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::Bool => "Boolean".to_string(),
            AbiType::I8 => "Byte".to_string(),
            AbiType::U8 => "Byte".to_string(),
            AbiType::I16 => "Short".to_string(),
            AbiType::U16 => "Short".to_string(),
            AbiType::I32 => "Int".to_string(),
            AbiType::U32 => "Int".to_string(),
            AbiType::I64 => "Long".to_string(),
            AbiType::U64 => "Long".to_string(),
            AbiType::ISize => "Long".to_string(),
            AbiType::USize => "Long".to_string(),
            AbiType::F32 => "Float".to_string(),
            AbiType::F64 => "Double".to_string(),
            AbiType::Pointer => "Long".to_string(),
            AbiType::Void => "Unit".to_string(),
        }
    }

    fn jni_type_for_param(&self, param: &AbiParam) -> String {
        match &param.role {
            ParamRole::InDirect => self.jni_type_for_abi(&param.ffi_type),
            ParamRole::InString { .. } => "String".to_string(),
            ParamRole::InBuffer { element_abi, .. } => self.jni_buffer_type(element_abi),
            ParamRole::InEncoded { .. } => "ByteBuffer".to_string(),
            ParamRole::InHandle { .. } => "Long".to_string(),
            ParamRole::InCallback { .. } => "Long".to_string(),
            _ => self.jni_type_for_abi(&param.ffi_type),
        }
    }

    fn jni_buffer_type(&self, element_abi: &AbiType) -> String {
        match element_abi {
            AbiType::I32 | AbiType::U32 => "IntArray".to_string(),
            AbiType::I16 | AbiType::U16 => "ShortArray".to_string(),
            AbiType::I64 | AbiType::U64 => "LongArray".to_string(),
            AbiType::F32 => "FloatArray".to_string(),
            AbiType::F64 => "DoubleArray".to_string(),
            AbiType::U8 | AbiType::I8 => "ByteArray".to_string(),
            AbiType::Bool => "BooleanArray".to_string(),
            _ => "ByteBuffer".to_string(),
        }
    }

    fn kotlin_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => self.primitive_kotlin_type(*p),
            TypeExpr::String => "String".to_string(),
            TypeExpr::Bytes => "ByteArray".to_string(),
            TypeExpr::Builtin(id) => self.builtin_kotlin_type(id),
            TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Custom(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Vec(inner) => self.kotlin_vec_type(inner),
            TypeExpr::Option(inner) => format!("{}?", self.kotlin_type(inner)),
            TypeExpr::Result { ok, .. } => self.kotlin_type(ok),
            TypeExpr::Handle(class_id) => NamingConvention::class_name(class_id.as_str()),
            TypeExpr::Callback(callback_id) => NamingConvention::class_name(callback_id.as_str()),
            TypeExpr::Void => "Unit".to_string(),
        }
    }

    fn kotlin_vec_type(&self, inner: &TypeExpr) -> String {
        match inner {
            TypeExpr::Primitive(p) => match p {
                PrimitiveType::I32 | PrimitiveType::U32 => "IntArray".to_string(),
                PrimitiveType::I16 | PrimitiveType::U16 => "ShortArray".to_string(),
                PrimitiveType::I64
                | PrimitiveType::U64
                | PrimitiveType::ISize
                | PrimitiveType::USize => "LongArray".to_string(),
                PrimitiveType::F32 => "FloatArray".to_string(),
                PrimitiveType::F64 => "DoubleArray".to_string(),
                PrimitiveType::U8 | PrimitiveType::I8 => "ByteArray".to_string(),
                PrimitiveType::Bool => "BooleanArray".to_string(),
            },
            _ => format!("List<{}>", self.kotlin_type(inner)),
        }
    }

    fn primitive_kotlin_type(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "Boolean".to_string(),
            PrimitiveType::I8 => "Byte".to_string(),
            PrimitiveType::U8 => "UByte".to_string(),
            PrimitiveType::I16 => "Short".to_string(),
            PrimitiveType::U16 => "UShort".to_string(),
            PrimitiveType::I32 => "Int".to_string(),
            PrimitiveType::U32 => "UInt".to_string(),
            PrimitiveType::I64 | PrimitiveType::ISize => "Long".to_string(),
            PrimitiveType::U64 | PrimitiveType::USize => "ULong".to_string(),
            PrimitiveType::F32 => "Float".to_string(),
            PrimitiveType::F64 => "Double".to_string(),
        }
    }

    fn primitive_jni_type(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "Boolean".to_string(),
            PrimitiveType::I8 | PrimitiveType::U8 => "Byte".to_string(),
            PrimitiveType::I16 | PrimitiveType::U16 => "Short".to_string(),
            PrimitiveType::I32 | PrimitiveType::U32 => "Int".to_string(),
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "Long".to_string(),
            PrimitiveType::F32 => "Float".to_string(),
            PrimitiveType::F64 => "Double".to_string(),
        }
    }

    fn builtin_kotlin_type(&self, id: &BuiltinId) -> String {
        match id.as_str() {
            "Duration" => "Duration".to_string(),
            "SystemTime" => "Instant".to_string(),
            "Uuid" => "UUID".to_string(),
            "Url" => "URI".to_string(),
            _ => "String".to_string(),
        }
    }

    fn kotlin_return_type_from_def(
        &self,
        returns: &ReturnDef,
        transport: &ReturnTransport,
    ) -> Option<String> {
        let base = match returns {
            ReturnDef::Void => None,
            ReturnDef::Value(ty) => Some(self.kotlin_type(ty)),
            ReturnDef::Result { ok, .. } => match ok {
                TypeExpr::Void => Some("Unit".to_string()),
                _ => Some(self.kotlin_type(ok)),
            },
        };
        match transport {
            ReturnTransport::Handle { nullable, .. }
            | ReturnTransport::Callback { nullable, .. }
                if *nullable =>
            {
                base.map(|ty| {
                    if ty.ends_with('?') {
                        ty
                    } else {
                        format!("{}?", ty)
                    }
                })
            }
            _ => base,
        }
    }

    fn kotlin_type_from_return_def(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Void => "Unit".to_string(),
            ReturnDef::Value(ty) => self.kotlin_type(ty),
            ReturnDef::Result { ok, .. } => match ok {
                TypeExpr::Void => "Unit".to_string(),
                _ => self.kotlin_type(ok),
            },
        }
    }

    fn closure_interface_name(&self, callback_id: &str) -> String {
        let signature = callback_id
            .strip_prefix("__Closure_")
            .unwrap_or(callback_id);
        format!("{}Callback", signature)
    }

    fn closure_param_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Record(_) => "java.nio.ByteBuffer".to_string(),
            _ => self.kotlin_type(ty),
        }
    }

    fn kotlin_type_from_abi(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::Bool => "Boolean".to_string(),
            AbiType::I8 => "Byte".to_string(),
            AbiType::U8 => "UByte".to_string(),
            AbiType::I16 => "Short".to_string(),
            AbiType::U16 => "UShort".to_string(),
            AbiType::I32 => "Int".to_string(),
            AbiType::U32 => "UInt".to_string(),
            AbiType::I64 => "Long".to_string(),
            AbiType::U64 => "ULong".to_string(),
            AbiType::ISize => "Long".to_string(),
            AbiType::USize => "ULong".to_string(),
            AbiType::F32 => "Float".to_string(),
            AbiType::F64 => "Double".to_string(),
            AbiType::Pointer => "Long".to_string(),
            AbiType::Void => "Unit".to_string(),
        }
    }

    fn return_type_from_decode_ops(&self, seq: &ReadSeq) -> String {
        let op = seq.ops.first().expect("decode op");
        match op {
            ReadOp::Primitive { primitive, .. } => self.primitive_kotlin_type(*primitive),
            ReadOp::String { .. } => "String".to_string(),
            ReadOp::Bytes { .. } => "ByteArray".to_string(),
            ReadOp::Builtin { id, .. } => self.builtin_kotlin_type(id),
            ReadOp::Record { id, .. } => NamingConvention::class_name(id.as_str()),
            ReadOp::Enum { id, .. } => NamingConvention::class_name(id.as_str()),
            ReadOp::Vec { element_type, .. } => self.kotlin_vec_type(element_type),
            ReadOp::Option { some, .. } => format!("{}?", self.return_type_from_decode_ops(some)),
            ReadOp::Result { ok, .. } => self.return_type_from_decode_ops(ok),
            ReadOp::Custom { id, .. } => NamingConvention::class_name(id.as_str()),
        }
    }

    fn kotlin_return_abi(&self, returns: &ReturnTransport) -> KotlinReturnAbi {
        match returns {
            ReturnTransport::Void => KotlinReturnAbi::Unit,
            ReturnTransport::Direct(abi) => KotlinReturnAbi::Direct {
                kotlin_cast: self.kotlin_return_cast(abi),
            },
            ReturnTransport::Handle { class_id, nullable } => KotlinReturnAbi::Direct {
                kotlin_cast: self.kotlin_handle_return_cast(class_id, *nullable),
            },
            ReturnTransport::Callback {
                callback_id,
                nullable,
            } => KotlinReturnAbi::Direct {
                kotlin_cast: self.kotlin_callback_return_cast(callback_id, *nullable),
            },
            ReturnTransport::Encoded { .. } => KotlinReturnAbi::WireEncoded,
        }
    }

    fn kotlin_return_abi_for_async(&self, result: &AsyncResultTransport) -> KotlinReturnAbi {
        match result {
            AsyncResultTransport::Void => KotlinReturnAbi::Unit,
            AsyncResultTransport::Direct(abi) => KotlinReturnAbi::Direct {
                kotlin_cast: self.kotlin_return_cast(abi),
            },
            AsyncResultTransport::Handle { class_id, nullable } => KotlinReturnAbi::Direct {
                kotlin_cast: self.kotlin_handle_return_cast(class_id, *nullable),
            },
            AsyncResultTransport::Callback {
                callback_id,
                nullable,
            } => KotlinReturnAbi::Direct {
                kotlin_cast: self.kotlin_callback_return_cast(callback_id, *nullable),
            },
            AsyncResultTransport::Encoded { .. } => KotlinReturnAbi::WireEncoded,
        }
    }

    fn kotlin_handle_return_cast(&self, class_id: &ClassId, nullable: bool) -> String {
        let class_name = NamingConvention::class_name(class_id.as_str());
        if nullable {
            format!(".takeIf {{ it != 0L }}?.let {{ {}(it) }}", class_name)
        } else {
            format!(".let {{ {}(it) }}", class_name)
        }
    }

    fn kotlin_callback_return_cast(&self, callback_id: &CallbackId, nullable: bool) -> String {
        let bridge = format!(
            "{}Bridge",
            NamingConvention::class_name(callback_id.as_str())
        );
        if nullable {
            format!(".takeIf {{ it != 0L }}?.let {{ {}.create(it) }}", bridge)
        } else {
            format!(".let {{ {}.create(it) }}", bridge)
        }
    }

    fn kotlin_return_cast(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::U8 => ".toUByte()".to_string(),
            AbiType::U16 => ".toUShort()".to_string(),
            AbiType::U32 => ".toUInt()".to_string(),
            AbiType::U64 | AbiType::USize => ".toULong()".to_string(),
            _ => String::new(),
        }
    }

    fn is_throwing_return(&self, returns: &ReturnDef) -> bool {
        matches!(returns, ReturnDef::Result { .. })
    }

    fn error_type_name(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Result { err, .. } => match err {
                TypeExpr::Enum(id) if self.is_error_enum(id) => {
                    NamingConvention::class_name(id.as_str())
                }
                _ => "FfiException".to_string(),
            },
            _ => "FfiException".to_string(),
        }
    }

    fn err_to_throwable(&self, err: &TypeExpr) -> String {
        match err {
            TypeExpr::String => "FfiException(-1, err)".to_string(),
            TypeExpr::Enum(id) if self.is_error_enum(id) => "err".to_string(),
            _ => "FfiException(-1, \"Error: $err\")".to_string(),
        }
    }

    fn is_error_enum(&self, id: &EnumId) -> bool {
        self.contract
            .catalog
            .resolve_enum(id)
            .map(|enumeration| enumeration.is_error)
            .unwrap_or(false)
    }

    fn wire_writers_for_params(&self, call: &AbiCall) -> Vec<KotlinWireWriter> {
        call.params
            .iter()
            .filter_map(|param| match &param.role {
                ParamRole::InEncoded { encode_ops, .. } => Some(KotlinWireWriter {
                    binding_name: format!("wire_writer_{}", param.name.as_str()),
                    size_expr: emit::emit_size_expr(&encode_ops.size),
                    encode_expr: emit::emit_write_expr(encode_ops, param.name.as_str()),
                }),
                _ => None,
            })
            .collect()
    }

    fn native_args_for_params(&self, call: &AbiCall, writers: &[KotlinWireWriter]) -> Vec<String> {
        let len_params = self.len_param_names(call);
        call.params
            .iter()
            .filter(|param| self.include_native_param(param, &len_params))
            .map(|param| match &param.role {
                ParamRole::InEncoded { .. } => writers
                    .iter()
                    .find(|w| w.binding_name == format!("wire_writer_{}", param.name.as_str()))
                    .map(|w| format!("{}.buffer", w.binding_name))
                    .unwrap_or_else(|| "wire.buffer".to_string()),
                ParamRole::InHandle { nullable, .. } => {
                    let name = param.name.as_str();
                    if *nullable {
                        format!("{}?.handle ?: 0L", name)
                    } else {
                        format!("{}.handle", name)
                    }
                }
                ParamRole::InCallback {
                    callback_id,
                    nullable,
                    ..
                } => {
                    let bridge = format!(
                        "{}Bridge",
                        NamingConvention::class_name(callback_id.as_str())
                    );
                    let name = param.name.as_str();
                    if *nullable {
                        format!("{}?.let {{ {}.create(it) }} ?: 0L", name, bridge)
                    } else {
                        format!("{}.create({})", bridge, name)
                    }
                }
                _ => param.name.as_str().to_string(),
            })
            .collect()
    }

    fn len_param_names(&self, call: &AbiCall) -> HashSet<ParamName> {
        call.params
            .iter()
            .filter_map(|param| match &param.role {
                ParamRole::InBuffer { len_param, .. }
                | ParamRole::InString { len_param }
                | ParamRole::InEncoded { len_param, .. }
                | ParamRole::OutBuffer { len_param, .. } => Some(len_param.clone()),
                _ => None,
            })
            .collect()
    }

    fn include_native_param(&self, param: &AbiParam, len_params: &HashSet<ParamName>) -> bool {
        !len_params.contains(&param.name)
            && !matches!(
                param.role,
                ParamRole::OutLen { .. } | ParamRole::OutDirect | ParamRole::StatusOut
            )
    }

    fn decode_expr_for_call_return(
        &self,
        returns: &ReturnTransport,
        returns_def: &ReturnDef,
    ) -> String {
        match returns {
            ReturnTransport::Void | ReturnTransport::Direct(_) => String::new(),
            ReturnTransport::Encoded { decode_ops, .. } => {
                if self.is_throwing_return(returns_def) {
                    self.decode_result_expr(returns_def, decode_ops)
                } else if self.is_blittable_return(returns) {
                    self.decode_blittable_return(decode_ops)
                } else {
                    emit::emit_read_value(decode_ops, "0", "0")
                }
            }
            ReturnTransport::Handle { class_id, nullable } => {
                self.decode_handle_return(class_id, *nullable, "result")
            }
            ReturnTransport::Callback {
                callback_id,
                nullable,
            } => self.decode_callback_return(callback_id, *nullable, "result"),
        }
    }

    fn decode_result_expr(&self, returns: &ReturnDef, decode_ops: &ReadSeq) -> String {
        let (ok_seq, err_seq) = match decode_ops.ops.first() {
            Some(ReadOp::Result { ok, err, .. }) => (ok.as_ref(), err.as_ref()),
            _ => return emit::emit_read_value(decode_ops, "0", "0"),
        };
        let ok_expr = match returns {
            ReturnDef::Result {
                ok: TypeExpr::Void, ..
            } => "Unit to 0".to_string(),
            _ => emit::emit_read_pair(ok_seq, "pos", "pos"),
        };
        let err_expr = emit::emit_read_pair(err_seq, "pos", "pos");
        let err_to_throwable = match returns {
            ReturnDef::Result { err, .. } => self.err_to_throwable(err),
            _ => "FfiException(-1, \"Error: $err\")".to_string(),
        };
        format!(
            "wire.readResult(0, {{ pos -> {} }}, {{ pos -> {} }}).first.unwrapOrThrow {{ err -> {} }}",
            ok_expr, err_expr, err_to_throwable
        )
    }

    fn decode_handle_return(&self, class_id: &ClassId, nullable: bool, value_expr: &str) -> String {
        let class_name = NamingConvention::class_name(class_id.as_str());
        if nullable {
            format!(
                "{}.takeIf {{ it != 0L }}?.let {{ {}(it) }}",
                value_expr, class_name
            )
        } else {
            format!("{}({})", class_name, value_expr)
        }
    }

    fn decode_callback_return(
        &self,
        callback_id: &CallbackId,
        nullable: bool,
        value_expr: &str,
    ) -> String {
        let bridge = format!(
            "{}Bridge",
            NamingConvention::class_name(callback_id.as_str())
        );
        if nullable {
            format!(
                "{}.takeIf {{ it != 0L }}?.let {{ {}.create(it) }}",
                value_expr, bridge
            )
        } else {
            format!("{}.create({})", bridge, value_expr)
        }
    }

    fn is_blittable_return(&self, returns: &ReturnTransport) -> bool {
        match returns {
            ReturnTransport::Encoded { decode_ops, .. } => self.is_blittable_decode_seq(decode_ops),
            _ => false,
        }
    }

    fn decode_blittable_return(&self, decode_ops: &ReadSeq) -> String {
        match decode_ops.ops.first() {
            Some(ReadOp::Record { id, .. }) => {
                format!(
                    "{}Reader.read(buffer, 0)",
                    NamingConvention::class_name(id.as_str())
                )
            }
            Some(ReadOp::Vec {
                element_type: TypeExpr::Record(id),
                ..
            }) => format!(
                "{}Reader.readAll(buffer, 4, buffer.getInt(0))",
                NamingConvention::class_name(id.as_str())
            ),
            _ => emit::emit_read_value(decode_ops, "0", "0"),
        }
    }

    fn async_call_for_method(
        &self,
        _class: &ClassDef,
        _method: &MethodDef,
        call: &AbiCall,
    ) -> KotlinAsyncCall {
        let async_call = match &call.mode {
            CallMode::Async(async_call) => async_call,
            CallMode::Sync => panic!("async method missing async call"),
        };
        KotlinAsyncCall {
            poll: async_call.poll.as_str().to_string(),
            complete: async_call.complete.as_str().to_string(),
            cancel: async_call.cancel.as_str().to_string(),
            free: async_call.free.as_str().to_string(),
            return_abi: self.kotlin_return_abi_for_async(&async_call.result),
        }
    }

    fn async_call_decode_expr(&self, method: &MethodDef, call: &AbiCall) -> String {
        let async_call = match &call.mode {
            CallMode::Async(async_call) => async_call,
            CallMode::Sync => panic!("async method missing async call"),
        };
        self.decode_expr_for_async_result(&async_call.result, &method.returns)
    }

    fn decode_expr_for_async_result(
        &self,
        result: &AsyncResultTransport,
        returns: &ReturnDef,
    ) -> String {
        match result {
            AsyncResultTransport::Void | AsyncResultTransport::Direct(_) => String::new(),
            AsyncResultTransport::Encoded { decode_ops, .. } => {
                if self.is_throwing_return(returns) {
                    self.decode_result_expr(returns, decode_ops)
                } else if self.is_blittable_async_result(result) {
                    self.decode_blittable_return(decode_ops)
                } else {
                    emit::emit_read_value(decode_ops, "0", "0")
                }
            }
            AsyncResultTransport::Handle { class_id, nullable } => {
                self.decode_handle_return(class_id, *nullable, "result")
            }
            AsyncResultTransport::Callback {
                callback_id,
                nullable,
            } => self.decode_callback_return(callback_id, *nullable, "result"),
        }
    }

    fn is_blittable_async_return(&self, call: &AbiCall) -> bool {
        match &call.mode {
            CallMode::Async(async_call) => self.is_blittable_async_result(&async_call.result),
            CallMode::Sync => false,
        }
    }

    fn is_blittable_async_result(&self, result: &AsyncResultTransport) -> bool {
        match result {
            AsyncResultTransport::Encoded { decode_ops, .. } => {
                self.is_blittable_decode_seq(decode_ops)
            }
            _ => false,
        }
    }

    fn is_blittable_decode_seq(&self, decode_ops: &ReadSeq) -> bool {
        match decode_ops.ops.first() {
            Some(ReadOp::Record { id, .. }) => self
                .contract
                .catalog
                .resolve_record(id)
                .map(|record| record.is_blittable())
                .unwrap_or(false),
            Some(ReadOp::Vec {
                element_type,
                layout,
                ..
            }) => {
                matches!(layout, VecLayout::Blittable { .. })
                    && matches!(element_type, TypeExpr::Record(_))
            }
            _ => false,
        }
    }

    fn async_call_for_function(&self, _func: &FunctionDef, call: &AbiCall) -> KotlinAsyncCall {
        let async_call = match &call.mode {
            CallMode::Async(async_call) => async_call,
            CallMode::Sync => panic!("async function missing async call"),
        };
        let return_abi = self.kotlin_return_abi_for_async(&async_call.result);
        KotlinAsyncCall {
            poll: async_call.poll.as_str().to_string(),
            complete: async_call.complete.as_str().to_string(),
            cancel: async_call.cancel.as_str().to_string(),
            free: async_call.free.as_str().to_string(),
            return_abi,
        }
    }

    fn record_struct_size(&self, record_id: &str) -> usize {
        self.abi
            .records
            .iter()
            .find(|record| record.id.as_str() == record_id)
            .and_then(|record| record.size)
            .unwrap_or(0)
    }

    fn field_padding_after(&self, record_id: &RecordId, field_name: &FieldName) -> usize {
        let record = match self.abi_record_for(record_id) {
            Some(record) if record.is_blittable => record,
            _ => return 0,
        };

        let fields = match self.record_field_offsets(record) {
            Some(fields) => fields,
            None => return 0,
        };
        let current = match fields.iter().find(|field| field.name == *field_name) {
            Some(field) => field,
            None => return 0,
        };
        let next_offset = fields
            .iter()
            .filter(|field| field.offset > current.offset)
            .map(|field| field.offset)
            .min()
            .unwrap_or(record.size.unwrap_or(0));

        next_offset.saturating_sub(current.offset + current.size)
    }

    fn default_expr(&self, ty: &TypeExpr) -> Option<String> {
        match ty {
            TypeExpr::Primitive(p) => Some(match p {
                PrimitiveType::Bool => "false".to_string(),
                PrimitiveType::U8 => "0u".to_string(),
                PrimitiveType::U16 => "0u".to_string(),
                PrimitiveType::U32 => "0u".to_string(),
                PrimitiveType::U64 | PrimitiveType::USize => "0u".to_string(),
                PrimitiveType::I8 => "0".to_string(),
                PrimitiveType::I16 => "0".to_string(),
                PrimitiveType::I32 => "0".to_string(),
                PrimitiveType::I64 | PrimitiveType::ISize => "0".to_string(),
                PrimitiveType::F32 => "0f".to_string(),
                PrimitiveType::F64 => "0.0".to_string(),
            }),
            TypeExpr::String => Some("\"\"".to_string()),
            TypeExpr::Option(_) => Some("null".to_string()),
            _ => None,
        }
    }

    fn should_generate_fixed_enum_codec(&self, enumeration: &EnumDef) -> bool {
        match &enumeration.repr {
            EnumRepr::Data { variants, .. } => {
                variants.iter().all(|variant| match &variant.payload {
                    VariantPayload::Unit => true,
                    VariantPayload::Struct(fields) => fields
                        .iter()
                        .all(|field| matches!(field.type_expr, TypeExpr::Primitive(_))),
                    VariantPayload::Tuple(fields) => {
                        fields.iter().all(|ty| matches!(ty, TypeExpr::Primitive(_)))
                    }
                })
            }
            _ => false,
        }
    }

    fn data_enum_layout(&self, enumeration: &EnumDef) -> Option<DataEnumLayout> {
        let EnumRepr::Data { variants, .. } = &enumeration.repr else {
            return None;
        };

        let tag_size = 4usize;
        let tag_alignment = 4usize;

        let variant_layouts = variants
            .iter()
            .map(|variant| self.data_enum_variant_layout(&variant.payload))
            .collect::<Vec<_>>();

        let union_alignment = variant_layouts
            .iter()
            .map(|layout| layout.alignment)
            .max()
            .unwrap_or(1);

        let union_size = variant_layouts
            .iter()
            .map(|layout| layout.size)
            .max()
            .unwrap_or(0);

        let payload_offset = align_up(tag_size, union_alignment);
        let struct_alignment = tag_alignment.max(union_alignment);
        let struct_size = align_up(
            payload_offset + align_up(union_size, union_alignment),
            struct_alignment,
        );

        Some(DataEnumLayout {
            struct_size,
            payload_offset,
            variant_offsets: variant_layouts
                .into_iter()
                .map(|layout| layout.offsets)
                .collect(),
        })
    }

    fn data_enum_variant_layout(&self, payload: &VariantPayload) -> DataEnumVariantLayout {
        match payload {
            VariantPayload::Unit => DataEnumVariantLayout {
                offsets: Vec::new(),
                size: 0,
                alignment: 1,
            },
            VariantPayload::Struct(fields) => {
                let primitives = fields
                    .iter()
                    .map(|field| match field.type_expr {
                        TypeExpr::Primitive(primitive) => primitive,
                        _ => panic!("data enum payload must be primitive"),
                    })
                    .collect::<Vec<_>>();
                self.primitive_fields_layout(&primitives)
            }
            VariantPayload::Tuple(fields) => {
                let primitives = fields
                    .iter()
                    .map(|ty| match ty {
                        TypeExpr::Primitive(primitive) => *primitive,
                        _ => panic!("data enum payload must be primitive"),
                    })
                    .collect::<Vec<_>>();
                self.primitive_fields_layout(&primitives)
            }
        }
    }

    fn primitive_fields_layout(&self, primitives: &[PrimitiveType]) -> DataEnumVariantLayout {
        let (offsets, size, alignment) = primitives.iter().fold(
            (Vec::new(), 0usize, 1usize),
            |(mut offsets, mut current, mut alignment), primitive| {
                let (size, align) = primitive_layout(*primitive);
                let aligned = align_up(current, align);
                offsets.push(aligned);
                current = aligned + size;
                alignment = alignment.max(align);
                (offsets, current, alignment)
            },
        );
        DataEnumVariantLayout {
            offsets,
            size: align_up(size, alignment),
            alignment,
        }
    }

    fn record_field_read_seq(
        &self,
        record_id: &RecordId,
        field_name: &FieldName,
    ) -> Option<ReadSeq> {
        self.abi_record_for(record_id)
            .and_then(|record| match record.decode_ops.ops.first() {
                Some(ReadOp::Record { fields, .. }) => fields
                    .iter()
                    .find(|field| field.name == *field_name)
                    .map(|field| field.seq.clone()),
                _ => None,
            })
    }

    fn record_field_write_seq(
        &self,
        record_id: &RecordId,
        field_name: &FieldName,
    ) -> Option<WriteSeq> {
        self.abi_record_for(record_id)
            .and_then(|record| match record.encode_ops.ops.first() {
                Some(WriteOp::Record { fields, .. }) => fields
                    .iter()
                    .find(|field| field.name == *field_name)
                    .map(|field| field.seq.clone()),
                _ => None,
            })
    }

    fn record_field_offsets(&self, record: &AbiRecord) -> Option<Vec<RecordFieldOffset>> {
        match record.decode_ops.ops.first() {
            Some(ReadOp::Record { fields, .. }) => fields
                .iter()
                .map(|field| {
                    let offset = read_seq_offset(&field.seq)?;
                    let size = match &field.seq.size {
                        SizeExpr::Fixed(value) => *value,
                        _ => return None,
                    };
                    Some(RecordFieldOffset {
                        name: field.name.clone(),
                        offset,
                        size,
                    })
                })
                .collect::<Option<Vec<_>>>(),
            _ => None,
        }
    }

    fn record_blittable_fields(&self, record_id: &RecordId) -> Option<Vec<RecordBlittableField>> {
        let record = self.abi_record_for(record_id)?;
        if !record.is_blittable {
            return None;
        }
        match record.decode_ops.ops.first() {
            Some(ReadOp::Record { fields, .. }) => fields
                .iter()
                .map(|field| match field.seq.ops.first() {
                    Some(ReadOp::Primitive { primitive, .. }) => {
                        read_seq_offset(&field.seq).map(|offset_value| RecordBlittableField {
                            name: field.name.clone(),
                            offset: offset_value,
                            primitive: *primitive,
                        })
                    }
                    _ => None,
                })
                .collect::<Option<Vec<_>>>(),
            _ => None,
        }
    }

    fn abi_record_for(&self, record_id: &RecordId) -> Option<&AbiRecord> {
        self.abi
            .records
            .iter()
            .find(|record| record.id == *record_id)
    }

    fn abi_enum_for(&self, enumeration: &EnumDef) -> &AbiEnum {
        self.abi
            .enums
            .iter()
            .find(|abi_enum| abi_enum.id == enumeration.id)
            .expect("abi enum missing")
    }

    fn abi_call_for_function(&self, function: &FunctionDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|call| call.id == CallId::Function(function.id.clone()))
            .expect("abi call missing for function")
    }

    fn abi_call_for_method(&self, class: &ClassDef, method: &MethodDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|call| {
                call.id
                    == CallId::Method {
                        class_id: class.id.clone(),
                        method_id: method.id.clone(),
                    }
            })
            .expect("abi call missing for method")
    }

    fn abi_call_for_constructor(&self, class: &ClassDef, index: usize) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|call| {
                call.id
                    == CallId::Constructor {
                        class_id: class.id.clone(),
                        index,
                    }
            })
            .expect("abi call missing for constructor")
    }

    fn abi_callback_for(&self, callback_id: &CallbackId) -> &AbiCallbackInvocation {
        self.abi
            .callbacks
            .iter()
            .find(|callback| callback.callback_id == *callback_id)
            .expect("abi callback missing")
    }

    fn abi_callback_method(
        &self,
        callback_id: &CallbackId,
        method_id: &MethodId,
    ) -> &AbiCallbackMethod {
        self.abi_callback_for(callback_id)
            .methods
            .iter()
            .find(|method| method.id == *method_id)
            .expect("abi callback method missing")
    }

    fn primitive_field_accessors(&self, primitive: PrimitiveType) -> (String, String, String) {
        let getter = match primitive {
            PrimitiveType::Bool | PrimitiveType::I8 | PrimitiveType::U8 => "get",
            PrimitiveType::I16 | PrimitiveType::U16 => "getShort",
            PrimitiveType::I32 | PrimitiveType::U32 => "getInt",
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "getLong",
            PrimitiveType::F32 => "getFloat",
            PrimitiveType::F64 => "getDouble",
        }
        .to_string();

        let putter = match primitive {
            PrimitiveType::Bool | PrimitiveType::I8 | PrimitiveType::U8 => "put",
            PrimitiveType::I16 | PrimitiveType::U16 => "putShort",
            PrimitiveType::I32 | PrimitiveType::U32 => "putInt",
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "putLong",
            PrimitiveType::F32 => "putFloat",
            PrimitiveType::F64 => "putDouble",
        }
        .to_string();

        let conversion = match primitive {
            PrimitiveType::Bool => " != 0.toByte()",
            PrimitiveType::U8 => ".toUByte()",
            PrimitiveType::U16 => ".toUShort()",
            PrimitiveType::U32 => ".toUInt()",
            PrimitiveType::U64 | PrimitiveType::USize => ".toULong()",
            _ => "",
        }
        .to_string();

        (getter, putter, conversion)
    }

    fn primitive_write_value_expr(&self, primitive: PrimitiveType, value: &str) -> String {
        match primitive {
            PrimitiveType::Bool => format!("(if ({}) 1 else 0).toByte()", value),
            PrimitiveType::U8 => format!("({}).toByte()", value),
            PrimitiveType::U16 => format!("({}).toShort()", value),
            PrimitiveType::U32 => format!("({}).toInt()", value),
            PrimitiveType::U64 | PrimitiveType::USize => format!("({}).toLong()", value),
            _ => value.to_string(),
        }
    }

    fn find_custom_read_seq(&self, custom: &CustomTypeId) -> Option<ReadSeq> {
        self.read_seqs()
            .into_iter()
            .find_map(|seq| self.read_seq_custom(seq, custom))
    }

    fn read_seqs(&self) -> Vec<&ReadSeq> {
        let record_seqs = self.abi.records.iter().map(|record| &record.decode_ops);
        let enum_seqs = self
            .abi
            .enums
            .iter()
            .map(|enumeration| &enumeration.decode_ops);
        let enum_field_seqs = self.abi.enums.iter().flat_map(|enumeration| {
            enumeration
                .variants
                .iter()
                .flat_map(|variant| match &variant.payload {
                    AbiEnumPayload::Unit => Vec::new(),
                    AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => {
                        fields.iter().map(|field| &field.decode).collect()
                    }
                })
        });
        let call_seqs = self.abi.calls.iter().flat_map(|call| {
            let return_seq = match &call.return_ {
                ReturnTransport::Encoded { decode_ops, .. } => Some(decode_ops),
                _ => None,
            };
            let param_seqs = call.params.iter().filter_map(|param| match &param.role {
                ParamRole::InEncoded { decode_ops, .. } => Some(decode_ops),
                ParamRole::OutBuffer { decode_ops, .. } => Some(decode_ops),
                _ => None,
            });
            let error_seq = match &call.error {
                ErrorTransport::Encoded { decode_ops } => Some(decode_ops),
                _ => None,
            };
            let async_seq = match &call.mode {
                CallMode::Async(async_call) => match &async_call.result {
                    AsyncResultTransport::Encoded { decode_ops, .. } => Some(decode_ops),
                    _ => None,
                },
                CallMode::Sync => None,
            };
            return_seq
                .into_iter()
                .chain(param_seqs)
                .chain(error_seq)
                .chain(async_seq)
        });
        let callback_seqs = self.abi.callbacks.iter().flat_map(|callback| {
            callback.methods.iter().flat_map(|method| {
                let return_seq = match &method.return_ {
                    ReturnTransport::Encoded { decode_ops, .. } => Some(decode_ops),
                    _ => None,
                };
                let param_seqs = method.params.iter().filter_map(|param| match &param.role {
                    ParamRole::InEncoded { decode_ops, .. } => Some(decode_ops),
                    ParamRole::OutBuffer { decode_ops, .. } => Some(decode_ops),
                    _ => None,
                });
                return_seq.into_iter().chain(param_seqs)
            })
        });

        record_seqs
            .chain(enum_seqs)
            .chain(enum_field_seqs)
            .chain(call_seqs)
            .chain(callback_seqs)
            .collect()
    }

    fn read_seq_custom(&self, seq: &ReadSeq, custom: &CustomTypeId) -> Option<ReadSeq> {
        seq.ops.iter().find_map(|op| match op {
            ReadOp::Custom { id, underlying } if id == custom => Some(*underlying.clone()),
            ReadOp::Option { some, .. } => self.read_seq_custom(some, custom),
            ReadOp::Vec { element, .. } => self.read_seq_custom(element, custom),
            ReadOp::Record { fields, .. } => fields
                .iter()
                .find_map(|field| self.read_seq_custom(&field.seq, custom)),
            ReadOp::Result { ok, err, .. } => self
                .read_seq_custom(ok, custom)
                .or_else(|| self.read_seq_custom(err, custom)),
            _ => None,
        })
    }

    fn find_custom_write_seq(&self, custom: &CustomTypeId) -> Option<WriteSeq> {
        self.write_seqs()
            .into_iter()
            .find_map(|seq| self.write_seq_custom(seq, custom))
    }

    fn write_seqs(&self) -> Vec<&WriteSeq> {
        let record_seqs = self.abi.records.iter().map(|record| &record.encode_ops);
        let enum_seqs = self
            .abi
            .enums
            .iter()
            .map(|enumeration| &enumeration.encode_ops);
        let enum_field_seqs = self.abi.enums.iter().flat_map(|enumeration| {
            enumeration
                .variants
                .iter()
                .flat_map(|variant| match &variant.payload {
                    AbiEnumPayload::Unit => Vec::new(),
                    AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => {
                        fields.iter().map(|field| &field.encode).collect()
                    }
                })
        });
        let call_seqs = self.abi.calls.iter().flat_map(|call| {
            let return_seq = match &call.return_ {
                ReturnTransport::Encoded { encode_ops, .. } => Some(encode_ops),
                _ => None,
            };
            let param_seqs = call.params.iter().filter_map(|param| match &param.role {
                ParamRole::InEncoded { encode_ops, .. } => Some(encode_ops),
                _ => None,
            });
            let async_seq = match &call.mode {
                CallMode::Async(async_call) => match &async_call.result {
                    AsyncResultTransport::Encoded { encode_ops, .. } => Some(encode_ops),
                    _ => None,
                },
                CallMode::Sync => None,
            };
            return_seq.into_iter().chain(param_seqs).chain(async_seq)
        });
        let callback_seqs = self.abi.callbacks.iter().flat_map(|callback| {
            callback.methods.iter().flat_map(|method| {
                let return_seq = match &method.return_ {
                    ReturnTransport::Encoded { encode_ops, .. } => Some(encode_ops),
                    _ => None,
                };
                let param_seqs = method.params.iter().filter_map(|param| match &param.role {
                    ParamRole::InEncoded { encode_ops, .. } => Some(encode_ops),
                    _ => None,
                });
                return_seq.into_iter().chain(param_seqs)
            })
        });

        record_seqs
            .chain(enum_seqs)
            .chain(enum_field_seqs)
            .chain(call_seqs)
            .chain(callback_seqs)
            .collect()
    }

    fn write_seq_custom(&self, seq: &WriteSeq, custom: &CustomTypeId) -> Option<WriteSeq> {
        seq.ops.iter().find_map(|op| match op {
            WriteOp::Custom { id, underlying, .. } if id == custom => Some(*underlying.clone()),
            WriteOp::Option { some, .. } => self.write_seq_custom(some, custom),
            WriteOp::Vec { element, .. } => self.write_seq_custom(element, custom),
            WriteOp::Record { fields, .. } => fields
                .iter()
                .find_map(|field| self.write_seq_custom(&field.seq, custom)),
            WriteOp::Result { ok, err, .. } => self
                .write_seq_custom(ok, custom)
                .or_else(|| self.write_seq_custom(err, custom)),
            _ => None,
        })
    }

    fn read_seq_from_repr(&self, repr: &TypeExpr) -> ReadSeq {
        self.find_read_seq_for_type(repr)
            .unwrap_or_else(|| panic!("missing read ops for custom repr: {:?}", repr))
    }

    fn write_seq_from_repr(&self, repr: &TypeExpr) -> WriteSeq {
        self.find_write_seq_for_type(repr)
            .unwrap_or_else(|| panic!("missing write ops for custom repr: {:?}", repr))
    }

    fn find_read_seq_for_type(&self, ty: &TypeExpr) -> Option<ReadSeq> {
        self.read_seqs()
            .into_iter()
            .find(|seq| self.read_seq_matches_type(seq, ty))
            .cloned()
    }

    fn find_write_seq_for_type(&self, ty: &TypeExpr) -> Option<WriteSeq> {
        self.write_seqs()
            .into_iter()
            .find(|seq| self.write_seq_matches_type(seq, ty))
            .cloned()
    }

    fn read_seq_matches_type(&self, seq: &ReadSeq, ty: &TypeExpr) -> bool {
        match (seq.ops.first(), ty) {
            (Some(ReadOp::Primitive { primitive, .. }), TypeExpr::Primitive(expected)) => {
                primitive == expected
            }
            (Some(ReadOp::String { .. }), TypeExpr::String) => true,
            (Some(ReadOp::Bytes { .. }), TypeExpr::Bytes) => true,
            (Some(ReadOp::Builtin { id, .. }), TypeExpr::Builtin(expected)) => id == expected,
            (Some(ReadOp::Record { id, .. }), TypeExpr::Record(expected)) => id == expected,
            (Some(ReadOp::Enum { id, .. }), TypeExpr::Enum(expected)) => id == expected,
            (Some(ReadOp::Custom { id, .. }), TypeExpr::Custom(expected)) => id == expected,
            (Some(ReadOp::Vec { element_type, .. }), TypeExpr::Vec(inner)) => {
                element_type == inner.as_ref()
            }
            (Some(ReadOp::Option { some, .. }), TypeExpr::Option(inner)) => {
                self.read_seq_matches_type(some, inner)
            }
            (
                Some(ReadOp::Result { ok, err, .. }),
                TypeExpr::Result {
                    ok: ok_ty,
                    err: err_ty,
                },
            ) => self.read_seq_matches_type(ok, ok_ty) && self.read_seq_matches_type(err, err_ty),
            _ => false,
        }
    }

    fn write_seq_matches_type(&self, seq: &WriteSeq, ty: &TypeExpr) -> bool {
        match (seq.ops.first(), ty) {
            (Some(WriteOp::Primitive { primitive, .. }), TypeExpr::Primitive(expected)) => {
                primitive == expected
            }
            (Some(WriteOp::String { .. }), TypeExpr::String) => true,
            (Some(WriteOp::Bytes { .. }), TypeExpr::Bytes) => true,
            (Some(WriteOp::Builtin { id, .. }), TypeExpr::Builtin(expected)) => id == expected,
            (Some(WriteOp::Record { id, .. }), TypeExpr::Record(expected)) => id == expected,
            (Some(WriteOp::Enum { id, .. }), TypeExpr::Enum(expected)) => id == expected,
            (Some(WriteOp::Custom { id, .. }), TypeExpr::Custom(expected)) => id == expected,
            (Some(WriteOp::Vec { element_type, .. }), TypeExpr::Vec(inner)) => {
                element_type == inner.as_ref()
            }
            (Some(WriteOp::Option { some, .. }), TypeExpr::Option(inner)) => {
                self.write_seq_matches_type(some, inner)
            }
            (
                Some(WriteOp::Result { ok, err, .. }),
                TypeExpr::Result {
                    ok: ok_ty,
                    err: err_ty,
                },
            ) => self.write_seq_matches_type(ok, ok_ty) && self.write_seq_matches_type(err, err_ty),
            _ => false,
        }
    }

    fn blittable_vec_return_records(&self) -> HashSet<&str> {
        self.contract
            .functions
            .iter()
            .filter_map(|func| {
                let call = self.abi_call_for_function(func);
                match &call.return_ {
                    ReturnTransport::Encoded { decode_ops, .. } => match decode_ops.ops.first() {
                        Some(ReadOp::Vec {
                            element_type,
                            layout,
                            ..
                        }) => match element_type {
                            TypeExpr::Record(id)
                                if matches!(layout, VecLayout::Blittable { .. })
                                    && self
                                        .contract
                                        .catalog
                                        .resolve_record(id)
                                        .map(|record| record.is_blittable())
                                        .unwrap_or(false) =>
                            {
                                Some(id.as_str())
                            }
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                }
            })
            .collect()
    }

    fn blittable_vec_param_records(&self) -> HashSet<&str> {
        let types_from_functions = self
            .contract
            .functions
            .iter()
            .flat_map(|function| function.params.iter())
            .map(|param| &param.type_expr);
        let types_from_methods = self
            .contract
            .catalog
            .all_classes()
            .flat_map(|class| class.methods.iter())
            .flat_map(|method| method.params.iter())
            .map(|param| &param.type_expr);
        let types_from_ctors = self
            .contract
            .catalog
            .all_classes()
            .flat_map(|class| class.constructors.iter())
            .flat_map(|ctor| ctor.params().into_iter())
            .map(|param| &param.type_expr);
        let types_from_traits = self
            .contract
            .catalog
            .all_callbacks()
            .flat_map(|callback| callback.methods.iter())
            .flat_map(|method| method.params.iter())
            .map(|param| &param.type_expr);
        let types_from_records = self
            .contract
            .catalog
            .all_records()
            .flat_map(|record| record.fields.iter())
            .map(|field| &field.type_expr);
        let types_from_enums =
            self.contract
                .catalog
                .all_enums()
                .flat_map(|enumeration| match &enumeration.repr {
                    EnumRepr::Data { variants, .. } => variants
                        .iter()
                        .flat_map(|variant| match &variant.payload {
                            VariantPayload::Struct(fields) => fields
                                .iter()
                                .map(|field| &field.type_expr)
                                .collect::<Vec<_>>(),
                            VariantPayload::Tuple(fields) => fields.iter().collect::<Vec<_>>(),
                            VariantPayload::Unit => Vec::new(),
                        })
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                });

        types_from_functions
            .chain(types_from_methods)
            .chain(types_from_ctors)
            .chain(types_from_traits)
            .chain(types_from_records)
            .chain(types_from_enums)
            .filter_map(|ty| match ty {
                TypeExpr::Vec(inner) => match inner.as_ref() {
                    TypeExpr::Record(id) => Some(id.as_str()),
                    _ => None,
                },
                _ => None,
            })
            .filter(|record_name| {
                self.contract
                    .catalog
                    .all_records()
                    .any(|record| record.id.as_str() == *record_name && record.is_blittable())
            })
            .collect()
    }
}

struct RecordFieldOffset {
    name: FieldName,
    offset: usize,
    size: usize,
}

struct RecordBlittableField {
    name: FieldName,
    offset: usize,
    primitive: PrimitiveType,
}

struct DataEnumLayout {
    struct_size: usize,
    payload_offset: usize,
    variant_offsets: Vec<Vec<usize>>,
}

struct DataEnumVariantLayout {
    offsets: Vec<usize>,
    size: usize,
    alignment: usize,
}

fn align_up(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        value
    } else {
        value.div_ceil(alignment) * alignment
    }
}

fn primitive_layout(primitive: PrimitiveType) -> (usize, usize) {
    let size = primitive.wire_size_bytes();
    let alignment = match primitive {
        PrimitiveType::Bool | PrimitiveType::I8 | PrimitiveType::U8 => 1,
        PrimitiveType::I16 | PrimitiveType::U16 => 2,
        PrimitiveType::I32 | PrimitiveType::U32 | PrimitiveType::F32 => 4,
        PrimitiveType::I64
        | PrimitiveType::U64
        | PrimitiveType::ISize
        | PrimitiveType::USize
        | PrimitiveType::F64 => 8,
    };
    (size, alignment)
}

fn read_seq_offset(seq: &ReadSeq) -> Option<usize> {
    let op = seq.ops.first()?;
    let offset = match op {
        ReadOp::Primitive { offset, .. }
        | ReadOp::String { offset }
        | ReadOp::Bytes { offset }
        | ReadOp::Builtin { offset, .. }
        | ReadOp::Record { offset, .. }
        | ReadOp::Enum { offset, .. } => offset,
        _ => return None,
    };
    match offset {
        OffsetExpr::Fixed(value) => Some(*value),
        OffsetExpr::Base => Some(0),
        OffsetExpr::BasePlus(value) => Some(*value),
        _ => None,
    }
}

struct KotlinPreamble {
    prefix: String,
    extra_imports: Vec<String>,
    custom_types: Vec<KotlinCustomType>,
    has_streams: bool,
}
