use crate::ir::contract::{FfiContract, PackageInfo, TypeCatalog};
use crate::ir::definitions::{
    CStyleVariant, CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef,
    CustomTypeDef, DataVariant, DeprecationInfo, EnumDef, EnumRepr, FieldDef, FunctionDef,
    MethodDef, ParamDef, ParamPassing, Receiver, RecordDef, ReturnDef, StreamDef, StreamMode,
    VariantPayload,
};
use crate::ir::ids::{
    BuiltinId, CallbackId, ClassId, ConverterPath, CustomTypeId, EnumId, FieldName, FunctionId,
    MethodId, ParamName, QualifiedName, RecordId, StreamId, VariantName,
};
use crate::ir::types::{BuiltinDef, BuiltinKind, PrimitiveType, TypeExpr};
use crate::model::{self, Module};

pub struct ContractBuilder<'m> {
    module: &'m Module,
}

impl<'m> ContractBuilder<'m> {
    pub fn new(module: &'m Module) -> Self {
        Self { module }
    }

    pub fn build(&self) -> FfiContract {
        let mut catalog = TypeCatalog::new();

        self.module
            .records
            .iter()
            .map(|r| self.convert_record(r))
            .for_each(|r| catalog.insert_record(r));

        self.module
            .enums
            .iter()
            .map(|e| self.convert_enum(e))
            .for_each(|e| catalog.insert_enum(e));

        self.module
            .classes
            .iter()
            .map(|c| self.convert_class(c))
            .for_each(|c| catalog.insert_class(c));

        self.module
            .callback_traits
            .iter()
            .map(|cb| self.convert_callback_trait(cb))
            .for_each(|cb| catalog.insert_callback(cb));

        self.module
            .custom_types
            .iter()
            .map(|ct| self.convert_custom_type(ct))
            .for_each(|ct| catalog.insert_custom(ct));

        let mut builtin_ids: Vec<_> = self.module.used_builtins.iter().collect();
        builtin_ids.sort_by_key(|id| id.type_id());
        builtin_ids
            .into_iter()
            .map(|id| convert_builtin_id(*id))
            .for_each(|b| catalog.insert_builtin(b));

        let mut closure_entries: Vec<_> = self.module.closures.iter().collect();
        closure_entries.sort_by_key(|(id, _)| *id);
        closure_entries
            .into_iter()
            .map(|(sig_id, sig)| self.convert_closure_to_callback(sig_id, sig))
            .for_each(|cb| catalog.insert_callback(cb));

        let functions = self
            .module
            .functions
            .iter()
            .map(|f| self.convert_function(f))
            .collect();

        FfiContract {
            package: PackageInfo {
                name: self.module.name.clone(),
                version: None,
            },
            catalog,
            functions,
        }
    }

    fn convert_record(&self, record: &model::Record) -> RecordDef {
        RecordDef {
            id: RecordId::new(&record.name),
            fields: record
                .fields
                .iter()
                .map(|f| FieldDef {
                    name: FieldName::new(&f.name),
                    type_expr: self.convert_type(&f.field_type),
                    doc: f.doc.clone(),
                })
                .collect(),
            doc: record.doc.clone(),
            deprecated: record.deprecated.as_ref().map(convert_deprecation),
        }
    }

    fn convert_enum(&self, enumeration: &model::Enumeration) -> EnumDef {
        let repr = if enumeration.is_c_style() {
            EnumRepr::CStyle {
                tag_type: PrimitiveType::I32,
                variants: enumeration
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(idx, v)| CStyleVariant {
                        name: VariantName::new(&v.name),
                        discriminant: v.discriminant.unwrap_or(idx as i64),
                        doc: v.doc.clone(),
                    })
                    .collect(),
            }
        } else {
            EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: enumeration
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(idx, v)| DataVariant {
                        name: VariantName::new(&v.name),
                        discriminant: v.discriminant.unwrap_or(idx as i64),
                        payload: self.convert_variant_payload(&v.fields),
                        doc: v.doc.clone(),
                    })
                    .collect(),
            }
        };

        EnumDef {
            id: EnumId::new(&enumeration.name),
            repr,
            is_error: enumeration.is_error,
            doc: enumeration.doc.clone(),
            deprecated: enumeration.deprecated.as_ref().map(convert_deprecation),
        }
    }

    fn convert_variant_payload(&self, fields: &[model::RecordField]) -> VariantPayload {
        if fields.is_empty() {
            VariantPayload::Unit
        } else {
            VariantPayload::Struct(
                fields
                    .iter()
                    .map(|f| FieldDef {
                        name: FieldName::new(&f.name),
                        type_expr: self.convert_type(&f.field_type),
                        doc: f.doc.clone(),
                    })
                    .collect(),
            )
        }
    }

    fn convert_function(&self, func: &model::Function) -> FunctionDef {
        FunctionDef {
            id: FunctionId::new(&func.name),
            params: func
                .inputs
                .iter()
                .map(|p| self.convert_param(&p.name, &p.param_type))
                .collect(),
            returns: self.convert_return_type(&func.returns),
            is_async: func.is_async,
            doc: func.doc.clone(),
            deprecated: func.deprecated.as_ref().map(convert_deprecation),
        }
    }

    fn convert_class(&self, class: &model::Class) -> ClassDef {
        ClassDef {
            id: ClassId::new(&class.name),
            constructors: class
                .constructors
                .iter()
                .map(|ctor| self.convert_constructor(ctor))
                .collect(),
            methods: class
                .methods
                .iter()
                .map(|m| self.convert_method(m))
                .collect(),
            streams: class
                .streams
                .iter()
                .map(|s| self.convert_stream(s))
                .collect(),
            doc: class.doc.clone(),
            deprecated: class.deprecated.as_ref().map(convert_deprecation),
        }
    }

    fn convert_stream(&self, stream: &model::StreamMethod) -> StreamDef {
        StreamDef {
            id: StreamId::new(&stream.name),
            item_type: self.convert_type(&stream.item_type),
            mode: match stream.mode {
                model::StreamMode::Async => StreamMode::Async,
                model::StreamMode::Batch => StreamMode::Batch,
                model::StreamMode::Callback => StreamMode::Callback,
            },
            doc: stream.doc.clone(),
            deprecated: stream.deprecated.as_ref().map(convert_deprecation),
        }
    }

    fn convert_constructor(&self, ctor: &model::Constructor) -> ConstructorDef {
        let params: Vec<_> = ctor
            .inputs
            .iter()
            .map(|p| self.convert_param(&p.name, &p.param_type))
            .collect();

        if ctor.name == "new" {
            ConstructorDef::Default {
                params,
                is_fallible: ctor.is_fallible,
                doc: ctor.doc.clone(),
                deprecated: None,
            }
        } else if params.is_empty() {
            ConstructorDef::NamedFactory {
                name: MethodId::new(&ctor.name),
                is_fallible: ctor.is_fallible,
                doc: ctor.doc.clone(),
                deprecated: None,
            }
        } else {
            let mut params_iter = params.into_iter();
            let first_param = params_iter.next().expect("params is non-empty");
            ConstructorDef::NamedInit {
                name: MethodId::new(&ctor.name),
                first_param,
                rest_params: params_iter.collect(),
                is_fallible: ctor.is_fallible,
                doc: ctor.doc.clone(),
                deprecated: None,
            }
        }
    }

    fn convert_method(&self, method: &model::Method) -> MethodDef {
        MethodDef {
            id: MethodId::new(&method.name),
            receiver: convert_receiver(method.receiver),
            params: method
                .inputs
                .iter()
                .map(|p| self.convert_param(&p.name, &p.param_type))
                .collect(),
            returns: self.convert_return_type(&method.returns),
            is_async: method.is_async,
            doc: method.doc.clone(),
            deprecated: method.deprecated.as_ref().map(convert_deprecation),
        }
    }

    fn convert_callback_trait(&self, cb: &model::CallbackTrait) -> CallbackTraitDef {
        CallbackTraitDef {
            id: CallbackId::new(&cb.name),
            methods: cb
                .methods
                .iter()
                .map(|m| CallbackMethodDef {
                    id: MethodId::new(&m.name),
                    params: m
                        .inputs
                        .iter()
                        .map(|p| self.convert_param(&p.name, &p.param_type))
                        .collect(),
                    returns: self.convert_return_type(&m.returns),
                    is_async: m.is_async,
                    doc: m.doc.clone(),
                })
                .collect(),
            kind: CallbackKind::Trait,
            doc: cb.doc.clone(),
        }
    }

    fn convert_custom_type(&self, ct: &model::CustomType) -> CustomTypeDef {
        CustomTypeDef {
            id: CustomTypeId::new(&ct.name),
            rust_type: QualifiedName::new(&ct.name),
            repr: self.convert_type(&ct.repr),
            converters: ConverterPath {
                into_ffi: QualifiedName::new(format!("{}::into_ffi", ct.name)),
                try_from_ffi: QualifiedName::new(format!("{}::try_from_ffi", ct.name)),
            },
            doc: None,
        }
    }

    fn convert_closure_to_callback(
        &self,
        sig_id: &str,
        sig: &model::ClosureSignature,
    ) -> CallbackTraitDef {
        let params = sig
            .params
            .iter()
            .enumerate()
            .map(|(idx, ty)| {
                let (type_expr, passing) = self.convert_type_with_passing(ty);
                ParamDef {
                    name: ParamName::new(format!("arg{}", idx)),
                    type_expr,
                    passing,
                    doc: None,
                }
            })
            .collect();

        let returns = if sig.is_void_return() {
            ReturnDef::Void
        } else {
            ReturnDef::Value(self.convert_type(&sig.returns))
        };

        CallbackTraitDef {
            id: CallbackId::new(sig_id),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("call"),
                params,
                returns,
                is_async: false,
                doc: None,
            }],
            kind: CallbackKind::Closure,
            doc: None,
        }
    }

    fn convert_param(&self, name: &str, ty: &model::Type) -> ParamDef {
        let (type_expr, passing) = self.convert_type_with_passing(ty);
        ParamDef {
            name: ParamName::new(name),
            type_expr,
            passing,
            doc: None,
        }
    }

    fn convert_type_with_passing(&self, ty: &model::Type) -> (TypeExpr, ParamPassing) {
        match ty {
            model::Type::Slice(inner) => (
                TypeExpr::Vec(Box::new(self.convert_type(inner))),
                ParamPassing::Ref,
            ),
            model::Type::MutSlice(inner) => (
                TypeExpr::Vec(Box::new(self.convert_type(inner))),
                ParamPassing::RefMut,
            ),
            model::Type::BoxedTrait(name) => (
                TypeExpr::Callback(CallbackId::new(name)),
                ParamPassing::BoxedDyn,
            ),
            model::Type::Closure(sig) => {
                let sig_id = format!("__Closure_{}", sig.signature_id());
                (
                    TypeExpr::Callback(CallbackId::new(&sig_id)),
                    ParamPassing::ImplTrait,
                )
            }
            _ => (self.convert_type(ty), ParamPassing::Value),
        }
    }

    fn convert_type(&self, ty: &model::Type) -> TypeExpr {
        match ty {
            model::Type::Primitive(p) => TypeExpr::Primitive(convert_primitive(*p)),
            model::Type::String => TypeExpr::String,
            model::Type::Bytes => TypeExpr::Bytes,
            model::Type::Builtin(id) => TypeExpr::Builtin(BuiltinId::new(id.type_id())),
            model::Type::Vec(inner) => TypeExpr::Vec(Box::new(self.convert_type(inner))),
            model::Type::Option(inner) => TypeExpr::Option(Box::new(self.convert_type(inner))),
            model::Type::Result { ok, err } => TypeExpr::Result {
                ok: Box::new(self.convert_type(ok)),
                err: Box::new(self.convert_type(err)),
            },
            model::Type::Record(name) => TypeExpr::Record(RecordId::new(name)),
            model::Type::Enum(name) => TypeExpr::Enum(EnumId::new(name)),
            model::Type::Object(name) => TypeExpr::Handle(ClassId::new(name)),
            model::Type::Custom { name, .. } => TypeExpr::Custom(CustomTypeId::new(name)),
            model::Type::BoxedTrait(name) => TypeExpr::Callback(CallbackId::new(name)),
            model::Type::Closure(sig) => {
                let sig_id = format!("__Closure_{}", sig.signature_id());
                TypeExpr::Callback(CallbackId::new(&sig_id))
            }
            model::Type::Slice(inner) | model::Type::MutSlice(inner) => {
                TypeExpr::Vec(Box::new(self.convert_type(inner)))
            }
            model::Type::Void => TypeExpr::Void,
        }
    }

    fn convert_return_type(&self, ret: &model::ReturnType) -> ReturnDef {
        match ret {
            model::ReturnType::Void => ReturnDef::Void,
            model::ReturnType::Value(ty) => {
                if ty.is_void() {
                    ReturnDef::Void
                } else {
                    ReturnDef::Value(self.convert_type(ty))
                }
            }
            model::ReturnType::Fallible { ok, err } => ReturnDef::Result {
                ok: self.convert_type(ok),
                err: self.convert_type(err),
            },
        }
    }
}

fn convert_primitive(p: model::Primitive) -> PrimitiveType {
    match p {
        model::Primitive::Bool => PrimitiveType::Bool,
        model::Primitive::I8 => PrimitiveType::I8,
        model::Primitive::U8 => PrimitiveType::U8,
        model::Primitive::I16 => PrimitiveType::I16,
        model::Primitive::U16 => PrimitiveType::U16,
        model::Primitive::I32 => PrimitiveType::I32,
        model::Primitive::U32 => PrimitiveType::U32,
        model::Primitive::I64 => PrimitiveType::I64,
        model::Primitive::U64 => PrimitiveType::U64,
        model::Primitive::F32 => PrimitiveType::F32,
        model::Primitive::F64 => PrimitiveType::F64,
        model::Primitive::Isize => PrimitiveType::ISize,
        model::Primitive::Usize => PrimitiveType::USize,
    }
}

fn convert_receiver(r: model::Receiver) -> Receiver {
    match r {
        model::Receiver::None => Receiver::Static,
        model::Receiver::Ref => Receiver::RefSelf,
        model::Receiver::RefMut => Receiver::RefMutSelf,
    }
}

fn convert_deprecation(d: &model::Deprecation) -> DeprecationInfo {
    DeprecationInfo {
        message: d.message.clone(),
        since: d.since.clone(),
    }
}

fn convert_builtin_id(id: model::BuiltinId) -> BuiltinDef {
    let (kind, rust_type) = match id {
        model::BuiltinId::Duration => (BuiltinKind::Duration, "std::time::Duration"),
        model::BuiltinId::SystemTime => (BuiltinKind::SystemTime, "std::time::SystemTime"),
        model::BuiltinId::Uuid => (BuiltinKind::Uuid, "uuid::Uuid"),
        model::BuiltinId::Url => (BuiltinKind::Url, "url::Url"),
    };
    BuiltinDef {
        id: BuiltinId::new(id.type_id()),
        kind,
        rust_type: QualifiedName::new(rust_type),
    }
}

pub fn build_contract(module: &mut Module) -> FfiContract {
    module.collect_derived_types();
    ContractBuilder::new(module).build()
}
