use std::fs;
use std::path::Path;
use syn::{Attribute, Fields, ImplItem, Item, ItemImpl, ItemStruct, Type};
use walkdir::WalkDir;

use crate::model::{
    Class, Constructor, Method, Module, Parameter, Primitive, Receiver, Record, RecordField,
    StreamMethod, StreamMode, Type as MType,
};

pub struct SourceScanner {
    module_name: String,
    classes: Vec<ScannedClass>,
    records: Vec<ScannedRecord>,
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

impl SourceScanner {
    pub fn new(module_name: impl Into<String>) -> Self {
        Self {
            module_name: module_name.into(),
            classes: Vec::new(),
            records: Vec::new(),
        }
    }

    pub fn scan_directory(&mut self, dir: &Path) -> Result<(), String> {
        for entry in WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
        {
            self.scan_file(entry.path())?;
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
                    || has_repr_c(&item_struct.attrs)
                    || (has_attribute(&item_struct.attrs, "derive")
                        && has_ffi_type_derive(&item_struct.attrs))
                {
                    self.process_record(item_struct);
                }
            }
            Item::Impl(item_impl) => {
                if has_attribute(&item_impl.attrs, "ffi_class") {
                    self.process_class(item_impl);
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
                    let field_type = rust_type_to_ffi_type(&f.ty)?;
                    Some((field_name, field_type))
                })
                .collect(),
            _ => Vec::new(),
        };

        self.records.push(ScannedRecord { name, fields });
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

                if has_attribute(&method.attrs, "ffi_stream") {
                    if let Some(stream) = self.process_stream_method(method) {
                        class.streams.push(stream);
                    }
                } else {
                    if let Some(m) = self.process_method(method) {
                        class.methods.push(m);
                    }
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

        let params: Vec<(String, MType)> = method
            .sig
            .inputs
            .iter()
            .filter_map(|arg| {
                if let syn::FnArg::Typed(pat_type) = arg {
                    let param_name = match &*pat_type.pat {
                        syn::Pat::Ident(ident) => ident.ident.to_string(),
                        _ => return None,
                    };
                    let param_type = rust_type_to_ffi_type(&pat_type.ty)?;
                    Some((param_name, param_type))
                } else {
                    None
                }
            })
            .collect();

        let output = match &method.sig.output {
            syn::ReturnType::Default => None,
            syn::ReturnType::Type(_, ty) => rust_type_to_ffi_type(ty),
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

        let (item_type, mode) = extract_stream_attr(&method.attrs)?;

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

fn extract_stream_attr(attrs: &[Attribute]) -> Option<(MType, StreamMode)> {
    for attr in attrs {
        if !attr.path().is_ident("ffi_stream") {
            continue;
        }

        let Ok(meta) = attr.meta.require_list() else {
            continue;
        };

        let tokens = meta.tokens.to_string();
        let item_type = extract_item_type(&tokens)?;
        let mode = extract_stream_mode(&tokens);

        return Some((item_type, mode));
    }
    None
}

fn extract_item_type(tokens: &str) -> Option<MType> {
    let item_start = tokens.find("item")? + 4;
    let rest = &tokens[item_start..];
    let eq_pos = rest.find('=')?;
    let after_eq = rest[eq_pos + 1..].trim();

    let type_end = after_eq
        .find(',')
        .unwrap_or(after_eq.find(')').unwrap_or(after_eq.len()));
    let type_str = after_eq[..type_end].trim();

    string_to_ffi_type(type_str)
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

fn rust_type_to_ffi_type(ty: &Type) -> Option<MType> {
    match ty {
        Type::Path(type_path) => {
            let path_str = type_path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");

            string_to_ffi_type(&path_str)
        }
        Type::Reference(type_ref) => {
            if let Type::Path(inner) = &*type_ref.elem {
                let ident = inner.path.segments.last()?.ident.to_string();
                if ident == "str" {
                    return Some(MType::String);
                }
            }
            rust_type_to_ffi_type(&type_ref.elem)
        }
        Type::ImplTrait(impl_trait) => {
            for bound in &impl_trait.bounds {
                if let syn::TypeParamBound::Trait(trait_bound) = bound {
                    let trait_name = trait_bound
                        .path
                        .segments
                        .last()
                        .map(|s| s.ident.to_string())?;

                    if trait_name == "FnMut" || trait_name == "Fn" || trait_name == "FnOnce" {
                        if let syn::PathArguments::Parenthesized(args) =
                            &trait_bound.path.segments.last()?.arguments
                        {
                            let param_type = args.inputs.first().and_then(rust_type_to_ffi_type);
                            return Some(MType::Callback(Box::new(
                                param_type.unwrap_or(MType::Void),
                            )));
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn string_to_ffi_type(s: &str) -> Option<MType> {
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
            Some(MType::Vec(Box::new(string_to_ffi_type(inner)?)))
        }
        s if s.starts_with("Option<") => {
            let inner = &s[7..s.len() - 1];
            Some(MType::Option(Box::new(string_to_ffi_type(inner)?)))
        }
        s => Some(MType::Record(s.to_string())),
    }
}

pub fn scan_crate(crate_path: &Path, module_name: &str) -> Result<Module, String> {
    let src_path = crate_path.join("src");
    let mut scanner = SourceScanner::new(module_name);
    scanner.scan_directory(&src_path)?;
    Ok(scanner.into_module())
}
