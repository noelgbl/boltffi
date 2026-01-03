use std::collections::HashSet;
use std::fs;
use std::path::Path;
use syn::{
    Attribute, Fields, FnArg, ImplItem, Item, ItemEnum, ItemImpl, ItemStruct, ItemTrait, Type,
};
use walkdir::WalkDir;

use crate::model::{
    CallbackTrait, Class, Constructor, Enumeration, Function, Method, Module, Parameter, Primitive,
    Receiver, Record, RecordField, StreamMethod, StreamMode, TraitMethod, TraitMethodParam,
    Type as MType, Variant,
};

#[derive(Default)]
pub struct TypeRegistry {
    enums: HashSet<String>,
    records: HashSet<String>,
}

impl TypeRegistry {
    pub fn is_enum(&self, name: &str) -> bool {
        self.enums.contains(name)
    }

    pub fn register_enum(&mut self, name: String) {
        self.enums.insert(name);
    }

    pub fn register_record(&mut self, name: String) {
        self.records.insert(name);
    }

    pub fn classify_named_type(&self, name: &str) -> MType {
        if self.enums.contains(name) {
            MType::Enum(name.to_string())
        } else {
            MType::Record(name.to_string())
        }
    }
}

pub struct SourceScanner {
    module_name: String,
    type_registry: TypeRegistry,
    classes: Vec<ScannedClass>,
    records: Vec<ScannedRecord>,
    enums: Vec<ScannedEnum>,
    functions: Vec<ScannedFunction>,
    callback_traits: Vec<ScannedCallbackTrait>,
}

struct ScannedClass {
    name: String,
    methods: Vec<ScannedMethod>,
    streams: Vec<ScannedStream>,
    has_constructor: bool,
}

struct ScannedMethod {
    name: String,
    receiver: Receiver,
    params: Vec<(String, MType)>,
    output: Option<MType>,
    is_async: bool,
}

struct ScannedStream {
    name: String,
    item_type: MType,
    mode: StreamMode,
}

struct ScannedRecord {
    name: String,
    fields: Vec<(String, MType)>,
}

struct ScannedEnum {
    name: String,
    variants: Vec<ScannedVariant>,
    is_error: bool,
}

struct ScannedVariant {
    name: String,
    discriminant: Option<i64>,
    fields: Vec<(String, MType)>,
}

struct ScannedFunction {
    name: String,
    params: Vec<(String, MType)>,
    output: Option<MType>,
    is_async: bool,
}

struct ScannedCallbackTrait {
    name: String,
    methods: Vec<ScannedTraitMethod>,
}

struct ScannedTraitMethod {
    name: String,
    params: Vec<(String, MType)>,
    output: Option<MType>,
    is_async: bool,
}

impl SourceScanner {
    pub fn new(module_name: impl Into<String>) -> Self {
        Self {
            module_name: module_name.into(),
            type_registry: TypeRegistry::default(),
            classes: Vec::new(),
            records: Vec::new(),
            enums: Vec::new(),
            functions: Vec::new(),
            callback_traits: Vec::new(),
        }
    }

    pub fn scan_directory(&mut self, dir: &Path) -> Result<(), String> {
        let files: Vec<_> = WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
            .map(|e| e.path().to_path_buf())
            .collect();

        for path in &files {
            self.collect_type_names(path)?;
        }

        for path in &files {
            self.scan_file(path)?;
        }
        Ok(())
    }

    fn collect_type_names(&mut self, path: &Path) -> Result<(), String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        let syntax = syn::parse_file(&content)
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

        for item in &syntax.items {
            match item {
                Item::Struct(item_struct) => {
                    if has_attribute(&item_struct.attrs, "ffi_record")
                        || has_attribute(&item_struct.attrs, "data")
                        || has_repr_c(&item_struct.attrs)
                        || (has_attribute(&item_struct.attrs, "derive")
                            && has_ffi_type_derive(&item_struct.attrs))
                    {
                        self.type_registry
                            .register_record(item_struct.ident.to_string());
                    }
                }
                Item::Enum(item_enum) => {
                    if has_repr_int(&item_enum.attrs)
                        || has_attribute(&item_enum.attrs, "data")
                        || has_attribute(&item_enum.attrs, "error")
                    {
                        self.type_registry
                            .register_enum(item_enum.ident.to_string());
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub fn scan_file(&mut self, path: &Path) -> Result<(), String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        let syntax = syn::parse_file(&content)
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

        for item in syntax.items {
            self.process_item(&item);
        }

        Ok(())
    }

    fn process_item(&mut self, item: &Item) {
        match item {
            Item::Struct(item_struct) => {
                if has_attribute(&item_struct.attrs, "ffi_record")
                    || has_attribute(&item_struct.attrs, "data")
                    || has_repr_c(&item_struct.attrs)
                    || (has_attribute(&item_struct.attrs, "derive")
                        && has_ffi_type_derive(&item_struct.attrs))
                {
                    self.process_record(item_struct);
                }
            }
            Item::Impl(item_impl) => {
                if has_attribute(&item_impl.attrs, "ffi_class")
                    || has_attribute(&item_impl.attrs, "export")
                {
                    self.process_class(item_impl);
                }
            }
            Item::Trait(item_trait) => {
                if has_attribute(&item_trait.attrs, "ffi_trait")
                    || has_attribute(&item_trait.attrs, "export")
                {
                    self.process_callback_trait(item_trait);
                }
            }
            Item::Fn(item_fn) => {
                if has_attribute(&item_fn.attrs, "ffi_export")
                    || has_attribute(&item_fn.attrs, "export")
                {
                    self.process_function(item_fn);
                }
            }
            Item::Enum(item_enum) => {
                let is_error = has_attribute(&item_enum.attrs, "error");
                if has_repr_int(&item_enum.attrs)
                    || has_attribute(&item_enum.attrs, "data")
                    || is_error
                {
                    self.process_enum(item_enum, is_error);
                }
            }
            _ => {}
        }
    }

    fn process_record(&mut self, item_struct: &ItemStruct) {
        let name = item_struct.ident.to_string();
        let fields = match &item_struct.fields {
            Fields::Named(named) => named
                .named
                .iter()
                .filter_map(|f| {
                    let field_name = f.ident.as_ref()?.to_string();
                    let field_type = rust_type_to_ffi_type(&f.ty, &self.type_registry)?;
                    Some((field_name, field_type))
                })
                .collect(),
            _ => Vec::new(),
        };

        self.records.push(ScannedRecord { name, fields });
    }

    fn process_enum(&mut self, item_enum: &ItemEnum, is_error: bool) {
        let name = item_enum.ident.to_string();
        let mut next_discriminant: i64 = 0;

        let variants: Vec<ScannedVariant> = item_enum
            .variants
            .iter()
            .map(|v| {
                let variant_name = v.ident.to_string();
                let discriminant = v
                    .discriminant
                    .as_ref()
                    .and_then(|(_, expr)| parse_discriminant_expr(expr))
                    .unwrap_or(next_discriminant);
                next_discriminant = discriminant + 1;

                let fields: Vec<(String, MType)> = match &v.fields {
                    Fields::Named(named) => named
                        .named
                        .iter()
                        .filter_map(|f| {
                            let field_name = f.ident.as_ref()?.to_string();
                            let field_type = rust_type_to_ffi_type(&f.ty, &self.type_registry)?;
                            Some((field_name, field_type))
                        })
                        .collect(),
                    Fields::Unnamed(unnamed) => unnamed
                        .unnamed
                        .iter()
                        .enumerate()
                        .filter_map(|(i, f)| {
                            let field_type = rust_type_to_ffi_type(&f.ty, &self.type_registry)?;
                            Some((format!("_{}", i), field_type))
                        })
                        .collect(),
                    Fields::Unit => Vec::new(),
                };

                ScannedVariant {
                    name: variant_name,
                    discriminant: Some(discriminant),
                    fields,
                }
            })
            .collect();

        self.enums.push(ScannedEnum { name, variants, is_error });
    }

    fn process_function(&mut self, item_fn: &syn::ItemFn) {
        let name = item_fn.sig.ident.to_string();
        let is_async = item_fn.sig.asyncness.is_some();

        let typed_params: Vec<_> = item_fn
            .sig
            .inputs
            .iter()
            .filter_map(|arg| {
                if let FnArg::Typed(pat_type) = arg {
                    Some(pat_type)
                } else {
                    None
                }
            })
            .collect();

        let params: Vec<(String, MType)> = typed_params
            .iter()
            .filter_map(|pat_type| {
                let param_name = match &*pat_type.pat {
                    syn::Pat::Ident(pat_ident) => pat_ident.ident.to_string(),
                    _ => return None,
                };
                let param_type = rust_type_to_ffi_type(&pat_type.ty, &self.type_registry)?;
                Some((param_name, param_type))
            })
            .collect();

        if params.len() != typed_params.len() {
            return;
        }

        let output = match &item_fn.sig.output {
            syn::ReturnType::Default => None,
            syn::ReturnType::Type(_, ty) => {
                let converted = rust_type_to_ffi_type(ty, &self.type_registry);
                if converted.is_none() {
                    return;
                }
                converted
            }
        };

        self.functions.push(ScannedFunction {
            name,
            params,
            output,
            is_async,
        });
    }

    fn process_callback_trait(&mut self, item_trait: &ItemTrait) {
        let name = item_trait.ident.to_string();
        let mut methods = Vec::new();

        for item in &item_trait.items {
            if let syn::TraitItem::Fn(method) = item
                && let Some(scanned_method) = self.process_trait_method(method)
            {
                methods.push(scanned_method);
            }
        }

        self.callback_traits
            .push(ScannedCallbackTrait { name, methods });
    }

    fn process_trait_method(&self, method: &syn::TraitItemFn) -> Option<ScannedTraitMethod> {
        let name = method.sig.ident.to_string();
        let is_async = method.sig.asyncness.is_some();

        let params: Vec<(String, MType)> = method
            .sig
            .inputs
            .iter()
            .filter_map(|arg| {
                if let FnArg::Typed(pat_type) = arg {
                    let param_name = match &*pat_type.pat {
                        syn::Pat::Ident(pat_ident) => pat_ident.ident.to_string(),
                        _ => return None,
                    };
                    let param_type = rust_type_to_ffi_type(&pat_type.ty, &self.type_registry)?;
                    Some((param_name, param_type))
                } else {
                    None
                }
            })
            .collect();

        let output = match &method.sig.output {
            syn::ReturnType::Default => None,
            syn::ReturnType::Type(_, ty) => rust_type_to_ffi_type(ty, &self.type_registry),
        };

        Some(ScannedTraitMethod {
            name,
            params,
            output,
            is_async,
        })
    }

    fn process_class(&mut self, item_impl: &ItemImpl) {
        let name = match &*item_impl.self_ty {
            Type::Path(type_path) => type_path
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default(),
            _ => return,
        };

        let mut class = ScannedClass {
            name,
            methods: Vec::new(),
            streams: Vec::new(),
            has_constructor: false,
        };

        for item in &item_impl.items {
            if let ImplItem::Fn(method) = item {
                if method.sig.ident == "new" {
                    class.has_constructor = true;
                    continue;
                }

                if has_attribute(&method.attrs, "skip") {
                    continue;
                }

                if has_attribute(&method.attrs, "ffi_stream") {
                    if let Some(stream) = self.process_stream_method(method) {
                        class.streams.push(stream);
                    }
                } else if let Some(m) = self.process_method(method) {
                    class.methods.push(m);
                }
            }
        }

        self.classes.push(class);
    }

    fn process_method(&self, method: &syn::ImplItemFn) -> Option<ScannedMethod> {
        let name = method.sig.ident.to_string();
        let is_async = method.sig.asyncness.is_some();

        let receiver = if method.sig.inputs.is_empty() {
            Receiver::None
        } else {
            match method.sig.inputs.first()? {
                syn::FnArg::Receiver(r) => {
                    if r.mutability.is_some() {
                        Receiver::RefMut
                    } else {
                        Receiver::Ref
                    }
                }
                _ => Receiver::None,
            }
        };

        let typed_params: Vec<_> = method
            .sig
            .inputs
            .iter()
            .filter_map(|arg| {
                if let syn::FnArg::Typed(pat_type) = arg {
                    Some(pat_type)
                } else {
                    None
                }
            })
            .collect();

        let params: Vec<(String, MType)> = typed_params
            .iter()
            .filter_map(|pat_type| {
                let param_name = match &*pat_type.pat {
                    syn::Pat::Ident(ident) => ident.ident.to_string(),
                    _ => return None,
                };
                let param_type = rust_type_to_ffi_type(&pat_type.ty, &self.type_registry)?;
                Some((param_name, param_type))
            })
            .collect();

        if params.len() != typed_params.len() {
            return None;
        }

        let output = match &method.sig.output {
            syn::ReturnType::Default => None,
            syn::ReturnType::Type(_, ty) => rust_type_to_ffi_type(ty, &self.type_registry),
        };

        Some(ScannedMethod {
            name,
            receiver,
            params,
            output,
            is_async,
        })
    }

    fn process_stream_method(&self, method: &syn::ImplItemFn) -> Option<ScannedStream> {
        let name = method.sig.ident.to_string();

        let (item_type, mode) = extract_stream_attr(&method.attrs, &self.type_registry)?;

        Some(ScannedStream {
            name,
            item_type,
            mode,
        })
    }

    pub fn into_module(self) -> Module {
        let mut module = Module::new(&self.module_name);

        for record in self.records {
            let mut r = Record::new(&record.name);
            for (name, ty) in record.fields {
                r = r.with_field(RecordField::new(&name, ty));
            }
            module = module.with_record(r);
        }

        for scanned_enum in self.enums {
            let mut e = Enumeration::new(&scanned_enum.name);
            if scanned_enum.is_error {
                e = e.as_error();
            }
            for variant in scanned_enum.variants {
                let mut v = Variant::new(&variant.name);
                if let Some(d) = variant.discriminant {
                    v = v.with_discriminant(d);
                }
                for (name, ty) in variant.fields {
                    v = v.with_field(RecordField::new(&name, ty));
                }
                e = e.with_variant(v);
            }
            module = module.with_enum(e);
        }

        for function in self.functions {
            let mut f = Function::new(&function.name);
            for (name, ty) in function.params {
                f = f.with_param(Parameter::new(&name, ty));
            }
            if let Some(output) = function.output {
                f = f.with_output(output);
            }
            if function.is_async {
                f = f.make_async();
            }
            module = module.with_function(f);
        }

        for class in self.classes {
            let mut c = Class::new(&class.name);

            if class.has_constructor {
                c = c.with_constructor(Constructor::new());
            }

            for method in class.methods {
                let mut m = Method::new(&method.name, method.receiver);
                for (name, ty) in method.params {
                    m = m.with_param(Parameter::new(&name, ty));
                }
                if let Some(output) = method.output {
                    m = m.with_output(output);
                }
                if method.is_async {
                    m = m.make_async();
                }
                c = c.with_method(m);
            }

            for stream in class.streams {
                let s = StreamMethod::new(&stream.name, stream.item_type).with_mode(stream.mode);
                c = c.with_stream(s);
            }

            module = module.with_class(c);
        }

        for callback_trait in self.callback_traits {
            let mut ct = CallbackTrait::new(&callback_trait.name);

            for method in callback_trait.methods {
                let mut tm = TraitMethod::new(&method.name);
                for (name, ty) in method.params {
                    tm = tm.with_param(TraitMethodParam::new(&name, ty));
                }
                if let Some(output) = method.output {
                    tm = tm.with_output(output);
                }
                if method.is_async {
                    tm = tm.make_async();
                }
                ct = ct.with_method(tm);
            }

            module = module.with_callback_trait(ct);
        }

        module
    }
}

fn has_attribute(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

fn has_repr_c(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("repr") {
            return false;
        }
        let Ok(meta) = attr.meta.require_list() else {
            return false;
        };
        meta.tokens.to_string().contains('C')
    })
}

fn has_repr_int(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("repr") {
            return false;
        }
        let Ok(meta) = attr.meta.require_list() else {
            return false;
        };
        let tokens = meta.tokens.to_string();
        ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"]
            .iter()
            .any(|ty| tokens.contains(ty))
    })
}

fn parse_discriminant_expr(expr: &syn::Expr) -> Option<i64> {
    match expr {
        syn::Expr::Lit(lit) => {
            if let syn::Lit::Int(int_lit) = &lit.lit {
                int_lit.base10_parse().ok()
            } else {
                None
            }
        }
        syn::Expr::Unary(unary) => {
            if matches!(unary.op, syn::UnOp::Neg(_)) {
                parse_discriminant_expr(&unary.expr).map(|v| -v)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn has_ffi_type_derive(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }
        let Ok(meta) = attr.meta.require_list() else {
            return false;
        };
        meta.tokens.to_string().contains("FfiType")
    })
}

fn extract_stream_attr(
    attrs: &[Attribute],
    registry: &TypeRegistry,
) -> Option<(MType, StreamMode)> {
    for attr in attrs {
        if !attr.path().is_ident("ffi_stream") {
            continue;
        }

        let Ok(meta) = attr.meta.require_list() else {
            continue;
        };

        let tokens = meta.tokens.to_string();
        let item_type = extract_item_type(&tokens, registry)?;
        let mode = extract_stream_mode(&tokens);

        return Some((item_type, mode));
    }
    None
}

fn extract_item_type(tokens: &str, registry: &TypeRegistry) -> Option<MType> {
    let item_start = tokens.find("item")? + 4;
    let rest = &tokens[item_start..];
    let eq_pos = rest.find('=')?;
    let after_eq = rest[eq_pos + 1..].trim();

    let type_end = after_eq
        .find(',')
        .unwrap_or(after_eq.find(')').unwrap_or(after_eq.len()));
    let type_str = after_eq[..type_end].trim();

    string_to_ffi_type(type_str, registry)
}

fn extract_stream_mode(tokens: &str) -> StreamMode {
    if tokens.contains("mode") {
        if tokens.contains("\"batch\"") {
            StreamMode::Batch
        } else if tokens.contains("\"callback\"") {
            StreamMode::Callback
        } else {
            StreamMode::Async
        }
    } else {
        StreamMode::Async
    }
}

fn rust_type_to_ffi_type(ty: &Type, registry: &TypeRegistry) -> Option<MType> {
    match ty {
        Type::Path(type_path) => {
            let last_segment = type_path.path.segments.last()?;
            let ident = last_segment.ident.to_string();

            if ident == "Box"
                && let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
                && let Some(syn::GenericArgument::Type(Type::TraitObject(trait_obj))) =
                    args.args.first()
                && let Some(syn::TypeParamBound::Trait(trait_bound)) = trait_obj.bounds.first()
                && let Some(seg) = trait_bound.path.segments.last()
            {
                return Some(MType::BoxedTrait(seg.ident.to_string()));
            }

            if ident == "Vec" {
                if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
                    && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
                {
                    let inner = rust_type_to_ffi_type(inner_ty, registry)?;
                    return Some(MType::Vec(Box::new(inner)));
                }
                return None;
            }

            if ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
                    && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
                {
                    let inner = rust_type_to_ffi_type(inner_ty, registry)?;
                    return Some(MType::Option(Box::new(inner)));
                }
                return None;
            }

            if ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                    let mut args_iter = args.args.iter();
                    if let Some(syn::GenericArgument::Type(ok_ty)) = args_iter.next() {
                        let ok = rust_type_to_ffi_type(ok_ty, registry)?;
                        let err = args_iter
                            .next()
                            .and_then(|arg| {
                                if let syn::GenericArgument::Type(err_ty) = arg {
                                    rust_type_to_ffi_type(err_ty, registry)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(MType::String);
                        return Some(MType::Result {
                            ok: Box::new(ok),
                            err: Box::new(err),
                        });
                    }
                }
                return None;
            }

            let path_str = type_path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");

            string_to_ffi_type(&path_str, registry)
        }
        Type::Reference(type_ref) => {
            if let Type::Path(inner) = &*type_ref.elem {
                let ident = inner.path.segments.last()?.ident.to_string();
                if ident == "str" {
                    return Some(MType::String);
                }
            }
            if let Type::Slice(slice) = &*type_ref.elem {
                let inner = rust_type_to_ffi_type(&slice.elem, registry)?;
                return if type_ref.mutability.is_some() {
                    Some(MType::MutSlice(Box::new(inner)))
                } else {
                    Some(MType::Slice(Box::new(inner)))
                };
            }
            rust_type_to_ffi_type(&type_ref.elem, registry)
        }
        Type::Slice(slice) => {
            let inner = rust_type_to_ffi_type(&slice.elem, registry)?;
            Some(MType::Slice(Box::new(inner)))
        }
        Type::ImplTrait(impl_trait) => {
            for bound in &impl_trait.bounds {
                if let syn::TypeParamBound::Trait(trait_bound) = bound {
                    let trait_name = trait_bound
                        .path
                        .segments
                        .last()
                        .map(|s| s.ident.to_string())?;

                    if (trait_name == "FnMut" || trait_name == "Fn" || trait_name == "FnOnce")
                        && let syn::PathArguments::Parenthesized(args) =
                            &trait_bound.path.segments.last()?.arguments
                    {
                        let param_type = args
                            .inputs
                            .first()
                            .and_then(|t| rust_type_to_ffi_type(t, registry));
                        return Some(MType::Callback(Box::new(param_type.unwrap_or(MType::Void))));
                    }
                }
            }
            None
        }
        Type::TraitObject(trait_obj) => {
            if let Some(syn::TypeParamBound::Trait(trait_bound)) = trait_obj.bounds.first()
                && let Some(seg) = trait_bound.path.segments.last()
            {
                return Some(MType::BoxedTrait(seg.ident.to_string()));
            }
            None
        }
        _ => None,
    }
}

fn string_to_ffi_type(s: &str, registry: &TypeRegistry) -> Option<MType> {
    match s.trim() {
        "i8" => Some(MType::Primitive(Primitive::I8)),
        "i16" => Some(MType::Primitive(Primitive::I16)),
        "i32" => Some(MType::Primitive(Primitive::I32)),
        "i64" => Some(MType::Primitive(Primitive::I64)),
        "u8" => Some(MType::Primitive(Primitive::U8)),
        "u16" => Some(MType::Primitive(Primitive::U16)),
        "u32" => Some(MType::Primitive(Primitive::U32)),
        "u64" => Some(MType::Primitive(Primitive::U64)),
        "f32" => Some(MType::Primitive(Primitive::F32)),
        "f64" => Some(MType::Primitive(Primitive::F64)),
        "bool" => Some(MType::Primitive(Primitive::Bool)),
        "usize" => Some(MType::Primitive(Primitive::Usize)),
        "isize" => Some(MType::Primitive(Primitive::Isize)),
        "String" | "str" => Some(MType::String),
        s if s.starts_with("Vec<") => {
            let inner = &s[4..s.len() - 1];
            Some(MType::Vec(Box::new(string_to_ffi_type(inner, registry)?)))
        }
        s if s.starts_with("Option<") => {
            let inner = &s[7..s.len() - 1];
            Some(MType::Option(Box::new(string_to_ffi_type(
                inner, registry,
            )?)))
        }
        s if s.starts_with("Result<") => {
            let inner = &s[7..s.len() - 1];
            let parts: Vec<&str> = inner.splitn(2, ',').map(|p| p.trim()).collect();
            let ok = string_to_ffi_type(parts.first()?, registry)?;
            let err = parts
                .get(1)
                .and_then(|e| string_to_ffi_type(e, registry))
                .unwrap_or(MType::String);
            Some(MType::Result {
                ok: Box::new(ok),
                err: Box::new(err),
            })
        }
        s => Some(registry.classify_named_type(s)),
    }
}

pub fn scan_crate(crate_path: &Path, module_name: &str) -> Result<Module, String> {
    let src_path = crate_path.join("src");
    let mut scanner = SourceScanner::new(module_name);
    scanner.scan_directory(&src_path)?;
    Ok(scanner.into_module())
}
