use std::fs;
use std::path::PathBuf;
use syn::{FnArg, ItemFn, Pat, ReturnType, Type};
use walkdir::WalkDir;

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = crate_dir.parent().unwrap();
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let header_path = out_dir.join("mobiFFI_core.h");

    let config_path = workspace_root.join("cbindgen.toml");
    let config = if config_path.exists() {
        cbindgen::Config::from_file(&config_path).unwrap()
    } else {
        cbindgen::Config::default()
    };

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(&header_path);

    let (macro_exports, stream_exports) = collect_ffi_exports(&crate_dir.join("src"));
    let repr_c_structs = collect_repr_c_structs(&crate_dir.join("src"));
    let ffi_enums = collect_ffi_enums(&crate_dir.join("src"));
    let ffi_traits = collect_ffi_traits(&crate_dir.join("src"));
    if !macro_exports.is_empty()
        || !repr_c_structs.is_empty()
        || !ffi_enums.is_empty()
        || !stream_exports.is_empty()
        || !ffi_traits.is_empty()
    {
        append_macro_exports(
            &header_path,
            &macro_exports,
            &repr_c_structs,
            &ffi_enums,
            &stream_exports,
            &ffi_traits,
        );
    }

    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=../cbindgen.toml");
}

enum FfiReturnKind {
    Unit,
    Primitive(String),
    String,
    ResultPrimitive(String),
    ResultString,
    Vec(String),
    OptionPrimitive(String),
    AsyncPoll(String),
}

struct FfiExport {
    name: String,
    params: Vec<(String, String)>,
    return_kind: FfiReturnKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum StreamMode {
    Async,
    Batch,
    Callback,
}

impl Default for StreamMode {
    fn default() -> Self {
        StreamMode::Async
    }
}

struct FfiStreamExport {
    class_name: String,
    method_name: String,
    item_type: String,
    mode: StreamMode,
}

struct FfiStruct {
    name: String,
    fields: Vec<(String, String)>,
}

struct FfiTrait {
    name: String,
    methods: Vec<FfiTraitMethod>,
}

struct FfiTraitMethod {
    name: String,
    params: Vec<(String, String)>,
    return_type: Option<String>,
    is_async: bool,
}

enum EnumVariantData {
    Unit,
    Tuple(Vec<String>),
    Struct(Vec<(String, String)>),
}

struct EnumVariant {
    name: String,
    discriminant: i64,
    data: EnumVariantData,
}

struct FfiEnum {
    name: String,
    repr: String,
    is_data_enum: bool,
    variants: Vec<EnumVariant>,
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
                parse_discriminant_expr(&unary.expr).map(|value| -value)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn collect_repr_c_structs(src_dir: &PathBuf) -> Vec<FfiStruct> {
    let mut structs = Vec::new();

    for entry in WalkDir::new(src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let content = match fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let syntax = match syn::parse_file(&content) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for item in &syntax.items {
            if let syn::Item::Struct(s) = item {
                let has_repr_c = s.attrs.iter().any(|attr| {
                    if attr.path().is_ident("repr") {
                        if let Ok(arg) = attr.parse_args::<syn::Ident>() {
                            return arg == "C";
                        }
                    }
                    false
                });

                let has_generics = !s.generics.params.is_empty();
                if has_repr_c && !has_generics {
                    if let syn::Fields::Named(fields) = &s.fields {
                        let field_list: Vec<(String, String)> = fields
                            .named
                            .iter()
                            .filter_map(|f| {
                                let name = f.ident.as_ref()?.to_string();
                                let ty = rust_type_to_c(&f.ty)?;
                                Some((name, ty))
                            })
                            .collect();

                        if !field_list.is_empty() {
                            structs.push(FfiStruct {
                                name: s.ident.to_string(),
                                fields: field_list,
                            });
                        }
                    }
                }
            }
        }
    }

    structs
}

fn parse_repr_attr(attrs: &[syn::Attribute]) -> Option<(String, bool)> {
    for attr in attrs {
        if attr.path().is_ident("repr") {
            let mut repr_type = None;
            let mut has_c = false;

            let _ = attr.parse_nested_meta(|meta| {
                let ident = meta
                    .path
                    .get_ident()
                    .map(|i| i.to_string())
                    .unwrap_or_default();
                if ident == "C" {
                    has_c = true;
                } else if matches!(
                    ident.as_str(),
                    "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
                ) {
                    repr_type = Some(ident);
                }
                Ok(())
            });

            if let Some(repr) = repr_type {
                return Some((repr, has_c));
            }
        }
    }
    None
}

fn collect_ffi_enums(src_dir: &PathBuf) -> Vec<FfiEnum> {
    let mut enums = Vec::new();

    for entry in WalkDir::new(src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let content = match fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let syntax = match syn::parse_file(&content) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for item in &syntax.items {
            if let syn::Item::Enum(e) = item {
                let Some((repr, has_c)) = parse_repr_attr(&e.attrs) else {
                    continue;
                };

                let mut variants = Vec::new();
                let mut next_value: i64 = 0;
                let mut is_data_enum = false;

                for variant in &e.variants {
                    if let Some((_, expr)) = &variant.discriminant {
                        next_value = parse_discriminant_expr(expr).unwrap_or(next_value);
                    }

                    let data = match &variant.fields {
                        syn::Fields::Unit => EnumVariantData::Unit,
                        syn::Fields::Unnamed(fields) => {
                            is_data_enum = true;
                            let types: Vec<String> = fields
                                .unnamed
                                .iter()
                                .filter_map(|f| rust_type_to_c(&f.ty))
                                .collect();
                            EnumVariantData::Tuple(types)
                        }
                        syn::Fields::Named(fields) => {
                            is_data_enum = true;
                            let named: Vec<(String, String)> = fields
                                .named
                                .iter()
                                .filter_map(|f| {
                                    let name = f.ident.as_ref()?.to_string();
                                    let ty = rust_type_to_c(&f.ty)?;
                                    Some((name, ty))
                                })
                                .collect();
                            EnumVariantData::Struct(named)
                        }
                    };

                    variants.push(EnumVariant {
                        name: variant.ident.to_string(),
                        discriminant: next_value,
                        data,
                    });
                    next_value += 1;
                }

                if is_data_enum && !has_c {
                    continue;
                }

                enums.push(FfiEnum {
                    name: e.ident.to_string(),
                    repr,
                    is_data_enum,
                    variants,
                });
            }
        }
    }

    enums
}

fn collect_ffi_traits(src_dir: &PathBuf) -> Vec<FfiTrait> {
    let mut traits = Vec::new();

    for entry in WalkDir::new(src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let content = match fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let syntax = match syn::parse_file(&content) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for item in &syntax.items {
            if let syn::Item::Trait(t) = item {
                let has_ffi_trait = t.attrs.iter().any(|attr| attr.path().is_ident("ffi_trait"));
                if !has_ffi_trait {
                    continue;
                }

                let mut methods = Vec::new();
                for trait_item in &t.items {
                    if let syn::TraitItem::Fn(method) = trait_item {
                        let method_name = method.sig.ident.to_string();
                        let is_async = method.sig.asyncness.is_some();

                        let mut params = Vec::new();
                        for input in &method.sig.inputs {
                            if let FnArg::Typed(pat_type) = input {
                                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                                    let param_name = pat_ident.ident.to_string();
                                    let param_type = rust_type_to_c(&pat_type.ty)
                                        .unwrap_or_else(|| "void*".to_string());
                                    params.push((param_name, param_type));
                                }
                            }
                        }

                        let return_type = match &method.sig.output {
                            ReturnType::Default => None,
                            ReturnType::Type(_, ty) => rust_type_to_c(ty),
                        };

                        methods.push(FfiTraitMethod {
                            name: method_name,
                            params,
                            return_type,
                            is_async,
                        });
                    }
                }

                traits.push(FfiTrait {
                    name: t.ident.to_string(),
                    methods,
                });
            }
        }
    }

    traits
}

fn collect_ffi_exports(src_dir: &PathBuf) -> (Vec<FfiExport>, Vec<FfiStreamExport>) {
    let mut exports = Vec::new();
    let mut stream_exports = Vec::new();

    for entry in WalkDir::new(src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let content = match fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let syntax = match syn::parse_file(&content) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for item in &syntax.items {
            if let syn::Item::Fn(func) = item {
                if has_ffi_export_attr(func) {
                    if let Some(export) = parse_ffi_function(func) {
                        exports.push(export);
                    }
                }
            }
            if let syn::Item::Impl(impl_block) = item {
                if has_ffi_class_attr(impl_block) {
                    let (class_exports, class_streams) = parse_ffi_class(impl_block);
                    exports.extend(class_exports);
                    stream_exports.extend(class_streams);
                }
            }
        }
    }

    (exports, stream_exports)
}

fn has_ffi_class_attr(impl_block: &syn::ItemImpl) -> bool {
    impl_block
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("ffi_class"))
}

fn to_snake_case(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn extract_ffi_stream_info(attrs: &[syn::Attribute]) -> Option<(String, StreamMode)> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("ffi_stream") {
            return None;
        }

        let mut item_type: Option<String> = None;
        let mut mode = StreamMode::default();

        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("item") {
                let ty: syn::Type = meta.value()?.parse()?;
                item_type = rust_type_to_c(&ty);
            } else if meta.path.is_ident("mode") {
                let mode_str: syn::LitStr = meta.value()?.parse()?;
                mode = match mode_str.value().as_str() {
                    "batch" => StreamMode::Batch,
                    "callback" => StreamMode::Callback,
                    _ => StreamMode::Async,
                };
            }
            Ok(())
        });

        item_type.map(|ty| (ty, mode))
    })
}

fn parse_ffi_class(impl_block: &syn::ItemImpl) -> (Vec<FfiExport>, Vec<FfiStreamExport>) {
    let mut exports = Vec::new();
    let mut stream_exports = Vec::new();

    let type_name = match impl_block.self_ty.as_ref() {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    };

    let type_name = match type_name {
        Some(name) => name,
        None => return (exports, stream_exports),
    };

    let snake_name = to_snake_case(&type_name);

    exports.push(FfiExport {
        name: format!("{}_new", snake_name),
        params: vec![],
        return_kind: FfiReturnKind::Primitive(format!("struct {} *", type_name)),
    });

    exports.push(FfiExport {
        name: format!("{}_free", snake_name),
        params: vec![("handle".to_string(), format!("struct {} *", type_name))],
        return_kind: FfiReturnKind::Unit,
    });

    for item in &impl_block.items {
        if let syn::ImplItem::Fn(method) = item {
            let has_self = method
                .sig
                .inputs
                .first()
                .map(|arg| matches!(arg, FnArg::Receiver(_)))
                .unwrap_or(false);

            if !has_self {
                continue;
            }

            if method.attrs.iter().any(|a| a.path().is_ident("skip")) {
                continue;
            }

            let method_name = method.sig.ident.to_string();
            if method_name == "new" {
                continue;
            }

            if let Some((item_type, mode)) = extract_ffi_stream_info(&method.attrs) {
                stream_exports.push(FfiStreamExport {
                    class_name: snake_name.clone(),
                    method_name: method_name.clone(),
                    item_type,
                    mode,
                });
                continue;
            }

            let mut params = vec![("handle".to_string(), format!("struct {} *", type_name))];

            for arg in method.sig.inputs.iter().skip(1) {
                if let FnArg::Typed(pat_type) = arg {
                    let param_name = match pat_type.pat.as_ref() {
                        Pat::Ident(ident) => ident.ident.to_string(),
                        _ => continue,
                    };

                    if is_string_param(&pat_type.ty) {
                        params.push((format!("{}_ptr", param_name), "const uint8_t*".to_string()));
                        params.push((format!("{}_len", param_name), "uintptr_t".to_string()));
                    } else if let Some(arg_types) = extract_callback_arg_types(&pat_type.ty) {
                        let cb_sig = format!("void (*)(void*, {})", arg_types.join(", "));
                        params.push((format!("{}_cb", param_name), cb_sig));
                        params.push((format!("{}_ud", param_name), "void*".to_string()));
                    } else if let Some((c_type, is_mut)) = extract_slice_type(&pat_type.ty) {
                        let ptr_type = if is_mut {
                            format!("{}*", c_type)
                        } else {
                            format!("const {}*", c_type)
                        };
                        params.push((format!("{}_ptr", param_name), ptr_type));
                        params.push((format!("{}_len", param_name), "uintptr_t".to_string()));
                    } else if let Some(c_type) = rust_type_to_c(&pat_type.ty) {
                        params.push((param_name, c_type));
                    }
                }
            }

            let return_kind = match &method.sig.output {
                ReturnType::Default => FfiReturnKind::Unit,
                ReturnType::Type(_, ty) => classify_return_type(ty),
            };

            exports.push(FfiExport {
                name: format!("{}_{}", snake_name, method_name),
                params,
                return_kind,
            });
        }
    }

    (exports, stream_exports)
}

fn has_ffi_export_attr(func: &ItemFn) -> bool {
    func.attrs
        .iter()
        .any(|attr| attr.path().is_ident("ffi_export"))
}

fn classify_return_type(ty: &Type) -> FfiReturnKind {
    let type_str = quote::quote!(#ty).to_string().replace(" ", "");

    if type_str == "String" || type_str == "std::string::String" {
        return FfiReturnKind::String;
    }

    if type_str == "()" {
        return FfiReturnKind::Unit;
    }

    if let Type::Path(path) = ty {
        if let Some(segment) = path.path.segments.last() {
            if segment.ident == "Vec" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        if let Some(c_type) = rust_type_to_c(inner_ty) {
                            return FfiReturnKind::Vec(c_type);
                        }
                    }
                }
            }
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        let inner_str = quote::quote!(#inner_ty).to_string().replace(" ", "");
                        if inner_str == "String" || inner_str == "std::string::String" {
                            return FfiReturnKind::ResultString;
                        } else if inner_str == "()" {
                            return FfiReturnKind::Unit;
                        } else if let Some(c_type) = rust_type_to_c(inner_ty) {
                            return FfiReturnKind::ResultPrimitive(c_type);
                        }
                    }
                }
            }
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        if let Some(c_type) = rust_type_to_c(inner_ty) {
                            return FfiReturnKind::OptionPrimitive(c_type);
                        }
                    }
                }
            }
        }
    }

    rust_type_to_c(ty)
        .map(FfiReturnKind::Primitive)
        .unwrap_or(FfiReturnKind::Unit)
}

fn is_string_param(ty: &Type) -> bool {
    let type_str = quote::quote!(#ty).to_string().replace(" ", "");
    type_str == "&str"
        || (type_str.starts_with("&'") && type_str.ends_with("str"))
        || type_str == "String"
        || type_str == "std::string::String"
}

fn extract_callback_arg_types(ty: &Type) -> Option<Vec<String>> {
    if let Type::ImplTrait(impl_trait) = ty {
        for bound in &impl_trait.bounds {
            if let syn::TypeParamBound::Trait(trait_bound) = bound {
                let path = &trait_bound.path;
                if let Some(segment) = path.segments.last() {
                    let ident = segment.ident.to_string();
                    if ident == "Fn" || ident == "FnMut" || ident == "FnOnce" {
                        if let syn::PathArguments::Parenthesized(args) = &segment.arguments {
                            let arg_types: Vec<String> = args
                                .inputs
                                .iter()
                                .filter_map(|t| rust_type_to_c(t))
                                .collect();
                            return Some(arg_types);
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_slice_type(ty: &Type) -> Option<(String, bool)> {
    if let Type::Reference(ref_ty) = ty {
        if let Type::Slice(slice_ty) = ref_ty.elem.as_ref() {
            let is_mut = ref_ty.mutability.is_some();
            if let Some(c_type) = rust_type_to_c(&slice_ty.elem) {
                return Some((c_type, is_mut));
            }
        }
    }
    None
}

fn extract_generic_inner_type(ty: &Type, wrapper: &str) -> Option<Type> {
    if let Type::Path(path) = ty {
        if let Some(segment) = path.path.segments.last() {
            if segment.ident == wrapper {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty.clone());
                    }
                }
            }
        }
    }
    None
}

fn rust_to_cbindgen_name(rust_type: &str) -> String {
    match rust_type {
        "int8_t" => "i8",
        "int16_t" => "i16",
        "int32_t" => "i32",
        "int64_t" => "i64",
        "uint8_t" => "u8",
        "uint16_t" => "u16",
        "uint32_t" => "u32",
        "uint64_t" => "u64",
        "float" => "f32",
        "double" => "f64",
        "bool" => "bool",
        other => other,
    }
    .to_string()
}

fn classify_async_callback_type(ty: &Type) -> String {
    let type_str = quote::quote!(#ty).to_string().replace(" ", "");

    if type_str == "String" || type_str == "std::string::String" {
        return "struct FfiString".to_string();
    }

    if let Some(inner_ty) = extract_generic_inner_type(ty, "Vec") {
        let inner_c = rust_type_to_c(&inner_ty).unwrap_or_else(|| "void".to_string());
        let cbindgen_name = rust_to_cbindgen_name(&inner_c);
        return format!("struct FfiBuf_{}", cbindgen_name);
    }

    if let Some(inner_ty) = extract_generic_inner_type(ty, "Option") {
        let inner_c = rust_type_to_c(&inner_ty).unwrap_or_else(|| "void".to_string());
        let cbindgen_name = rust_to_cbindgen_name(&inner_c);
        return format!("struct FfiOption_{}", cbindgen_name);
    }

    if let Type::Path(path) = ty {
        if let Some(segment) = path.path.segments.last() {
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        let inner_str = quote::quote!(#inner_ty).to_string().replace(" ", "");

                        if inner_str == "String" || inner_str == "std::string::String" {
                            return "struct FfiString".to_string();
                        }

                        if inner_str == "()" {
                            return "void".to_string();
                        }

                        if let Some(vec_inner) = extract_generic_inner_type(inner_ty, "Vec") {
                            let inner_c =
                                rust_type_to_c(&vec_inner).unwrap_or_else(|| "void".to_string());
                            let cbindgen_name = rust_to_cbindgen_name(&inner_c);
                            return format!("struct FfiBuf_{}", cbindgen_name);
                        }

                        return rust_type_to_c(inner_ty).unwrap_or_else(|| "void".to_string());
                    }
                }
            }
        }
    }

    rust_type_to_c(ty).unwrap_or_else(|| "void".to_string())
}

fn parse_ffi_function(func: &ItemFn) -> Option<FfiExport> {
    let name = func.sig.ident.to_string();
    let is_async = func.sig.asyncness.is_some();

    let mut params: Vec<(String, String)> = Vec::new();

    for arg in func.sig.inputs.iter() {
        if let FnArg::Typed(pat_type) = arg {
            let param_name = match pat_type.pat.as_ref() {
                Pat::Ident(ident) => ident.ident.to_string(),
                _ => continue,
            };

            if is_string_param(&pat_type.ty) {
                params.push((format!("{}_ptr", param_name), "const uint8_t*".to_string()));
                params.push((format!("{}_len", param_name), "uintptr_t".to_string()));
            } else if let Some(arg_types) = extract_callback_arg_types(&pat_type.ty) {
                let cb_sig = format!("void (*)(void*, {})", arg_types.join(", "));
                params.push((format!("{}_cb", param_name), cb_sig));
                params.push((format!("{}_ud", param_name), "void*".to_string()));
            } else if let Some((c_type, is_mut)) = extract_slice_type(&pat_type.ty) {
                let ptr_type = if is_mut {
                    format!("{}*", c_type)
                } else {
                    format!("const {}*", c_type)
                };
                params.push((format!("{}_ptr", param_name), ptr_type));
                params.push((format!("{}_len", param_name), "uintptr_t".to_string()));
            } else if let Some(c_type) = rust_type_to_c(&pat_type.ty) {
                params.push((param_name, c_type));
            }
        }
    }

    if is_async {
        let result_type = match &func.sig.output {
            ReturnType::Default => "void".to_string(),
            ReturnType::Type(_, ty) => classify_async_callback_type(ty),
        };

        return Some(FfiExport {
            name,
            params,
            return_kind: FfiReturnKind::AsyncPoll(result_type),
        });
    }

    let return_kind = match &func.sig.output {
        ReturnType::Default => FfiReturnKind::Unit,
        ReturnType::Type(_, ty) => classify_return_type(ty),
    };

    Some(FfiExport {
        name,
        params,
        return_kind,
    })
}

fn is_callback_typedef(name: &str) -> bool {
    name.ends_with("Callback") || name.ends_with("Handler") || name.ends_with("Fn")
}

fn rust_type_to_c(ty: &Type) -> Option<String> {
    let type_str = quote::quote!(#ty).to_string().replace(" ", "");

    match type_str.as_str() {
        "i8" => Some("int8_t".to_string()),
        "i16" => Some("int16_t".to_string()),
        "i32" => Some("int32_t".to_string()),
        "i64" => Some("int64_t".to_string()),
        "u8" => Some("uint8_t".to_string()),
        "u16" => Some("uint16_t".to_string()),
        "u32" => Some("uint32_t".to_string()),
        "u64" => Some("uint64_t".to_string()),
        "usize" => Some("uintptr_t".to_string()),
        "isize" => Some("intptr_t".to_string()),
        "f32" => Some("float".to_string()),
        "f64" => Some("double".to_string()),
        "bool" => Some("bool".to_string()),
        "()" => None,
        "&str" => Some("const char *".to_string()),
        "String" => Some("const char *".to_string()),
        _ => {
            if type_str.starts_with("&'") && type_str.contains("str") {
                return Some("const char *".to_string());
            }
            if type_str.starts_with("Box<dyn") {
                let inner = type_str
                    .trim_start_matches("Box<dyn")
                    .trim_end_matches(">")
                    .trim();
                return Some(format!("struct Foreign{}*", inner));
            }
            if type_str.starts_with("Box<") && type_str.ends_with(">") {
                let inner = &type_str[4..type_str.len() - 1];
                return Some(format!("struct {}*", inner));
            }
            if is_callback_typedef(&type_str) {
                Some(type_str)
            } else if type_str.starts_with("*const") {
                let inner = type_str.trim_start_matches("*const");
                rust_type_to_c_ptr(inner, "const")
            } else if type_str.starts_with("*mut") {
                let inner = type_str.trim_start_matches("*mut");
                rust_type_to_c_ptr(inner, "")
            } else {
                Some(type_str)
            }
        }
    }
}

fn rust_type_to_c_ptr(inner: &str, qualifier: &str) -> Option<String> {
    let c_inner = match inner {
        "u8" => "uint8_t",
        "i8" => "int8_t",
        "c_void" | "core::ffi::c_void" => "void",
        _ => {
            return Some(
                format!("{} struct {}*", qualifier, inner)
                    .trim()
                    .to_string(),
            );
        }
    };
    if qualifier.is_empty() {
        Some(format!("{}*", c_inner))
    } else {
        Some(format!("{} {}*", qualifier, c_inner))
    }
}

fn format_param(name: &str, ty: &str) -> String {
    if ty.contains("(*)") {
        ty.replace("(*)", &format!("(*{})", name))
    } else {
        format!("{} {}", ty, name)
    }
}

fn generate_export_declaration(export: &FfiExport) -> String {
    let base_params: Vec<String> = export
        .params
        .iter()
        .map(|(name, ty)| format_param(name, ty))
        .collect();

    match &export.return_kind {
        FfiReturnKind::Vec(inner_ty) => {
            let len_params = if base_params.is_empty() {
                "void".to_string()
            } else {
                base_params.join(", ")
            };

            let mut copy_params = base_params.clone();
            copy_params.push(format!("{} *dst", inner_ty));
            copy_params.push("uintptr_t dst_cap".to_string());
            copy_params.push("uintptr_t *written".to_string());

            format!(
                "uintptr_t mffi_{}_len({});\nstruct FfiStatus mffi_{}_copy_into({});\n",
                export.name,
                len_params,
                export.name,
                copy_params.join(", ")
            )
        }
        FfiReturnKind::AsyncPoll(result_type) => {
            let entry_params = if base_params.is_empty() {
                "void".to_string()
            } else {
                base_params.join(", ")
            };

            let mut out = String::new();
            out.push_str(&format!(
                "RustFutureHandle mffi_{}({});\n",
                export.name, entry_params
            ));
            out.push_str(&format!(
                "void mffi_{}_poll(RustFutureHandle handle, uint64_t callback_data, RustFutureContinuationCallback callback);\n",
                export.name
            ));
            out.push_str(&format!(
                "{} mffi_{}_complete(RustFutureHandle handle, struct FfiStatus* out_status);\n",
                result_type, export.name
            ));
            out.push_str(&format!(
                "void mffi_{}_cancel(RustFutureHandle handle);\n",
                export.name
            ));
            out.push_str(&format!(
                "void mffi_{}_free(RustFutureHandle handle);\n",
                export.name
            ));
            out
        }
        _ => {
            let mut params = base_params;
            let ret_type = match &export.return_kind {
                FfiReturnKind::Unit => "struct FfiStatus".to_string(),
                FfiReturnKind::Primitive(ty) => ty.clone(),
                FfiReturnKind::String | FfiReturnKind::ResultString => {
                    params.push("struct FfiString *out".to_string());
                    "struct FfiStatus".to_string()
                }
                FfiReturnKind::ResultPrimitive(ty) => {
                    params.push(format!("{} *out", ty));
                    "struct FfiStatus".to_string()
                }
                FfiReturnKind::OptionPrimitive(ty) => {
                    params.push(format!("{} *out", ty));
                    "int32_t".to_string()
                }
                FfiReturnKind::Vec(_) | FfiReturnKind::AsyncPoll(_) => unreachable!(),
            };

            let params_str = if params.is_empty() {
                "void".to_string()
            } else {
                params.join(", ")
            };

            format!("{} mffi_{}({});\n", ret_type, export.name, params_str)
        }
    }
}

fn to_camel_case(snake: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    for ch in snake.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

fn generate_struct_typedef(s: &FfiStruct) -> String {
    let fields: String = s
        .fields
        .iter()
        .map(|(name, ty)| format!("  {} {};\n", ty, to_camel_case(name)))
        .collect();
    format!("typedef struct {} {{\n{}}} {};\n\n", s.name, fields, s.name)
}

fn repr_to_c_type(repr: &str) -> &'static str {
    match repr {
        "i8" => "int8_t",
        "i16" => "int16_t",
        "i32" => "int32_t",
        "i64" => "int64_t",
        "u8" => "uint8_t",
        "u16" => "uint16_t",
        "u32" => "uint32_t",
        "u64" => "uint64_t",
        _ => "int32_t",
    }
}

fn generate_enum_typedef(e: &FfiEnum) -> String {
    let c_type = repr_to_c_type(&e.repr);

    if !e.is_data_enum {
        let mut out = format!("typedef {} {};\n", c_type, e.name);
        for variant in &e.variants {
            out.push_str(&format!(
                "#define {}_{} {}\n",
                e.name, variant.name, variant.discriminant
            ));
        }
        out.push('\n');
        return out;
    }

    let mut out = format!(
        "typedef struct {} {{\n  {} tag;\n  union {{\n",
        e.name, c_type
    );

    for variant in &e.variants {
        match &variant.data {
            EnumVariantData::Unit => {}
            EnumVariantData::Tuple(types) => {
                if types.len() == 1 {
                    out.push_str(&format!("    {} {};\n", types[0], variant.name));
                } else {
                    out.push_str(&format!("    struct {{ "));
                    for (i, ty) in types.iter().enumerate() {
                        out.push_str(&format!("{} _{}; ", ty, i));
                    }
                    out.push_str(&format!("}} {};\n", variant.name));
                }
            }
            EnumVariantData::Struct(fields) => {
                out.push_str(&format!("    struct {{ "));
                for (name, ty) in fields {
                    out.push_str(&format!("{} {}; ", ty, name));
                }
                out.push_str(&format!("}} {};\n", variant.name));
            }
        }
    }

    out.push_str("  } payload;\n");
    out.push_str(&format!("}} {};\n", e.name));

    for variant in &e.variants {
        out.push_str(&format!(
            "#define {}_TAG_{} {}\n",
            e.name, variant.name, variant.discriminant
        ));
    }
    out.push('\n');
    out
}

fn collect_generic_type_instantiations(exports: &[FfiExport]) -> Vec<(String, String, String)> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut types = Vec::new();

    for export in exports {
        if export.name.ends_with("_async") {
            for (_, param_type) in &export.params {
                if param_type.starts_with("void (*)(void*, struct FfiStatus, struct FfiBuf_") {
                    let inner = param_type
                        .trim_start_matches("void (*)(void*, struct FfiStatus, struct FfiBuf_")
                        .trim_end_matches(")");
                    let key = format!("FfiBuf_{}", inner);
                    if !seen.contains(&key) {
                        seen.insert(key);
                        types.push((
                            "FfiBuf".to_string(),
                            inner.to_string(),
                            c_type_for_generic(inner),
                        ));
                    }
                } else if param_type
                    .starts_with("void (*)(void*, struct FfiStatus, struct FfiOption_")
                {
                    let inner = param_type
                        .trim_start_matches("void (*)(void*, struct FfiStatus, struct FfiOption_")
                        .trim_end_matches(")");
                    let key = format!("FfiOption_{}", inner);
                    if !seen.contains(&key) {
                        seen.insert(key);
                        types.push((
                            "FfiOption".to_string(),
                            inner.to_string(),
                            c_type_for_generic(inner),
                        ));
                    }
                }
            }
        }
    }

    types
}

fn c_type_for_generic(rust_name: &str) -> String {
    match rust_name {
        "i8" => "int8_t",
        "i16" => "int16_t",
        "i32" => "int32_t",
        "i64" => "int64_t",
        "u8" => "uint8_t",
        "u16" => "uint16_t",
        "u32" => "uint32_t",
        "u64" => "uint64_t",
        "f32" => "float",
        "f64" => "double",
        "bool" => "bool",
        other => other,
    }
    .to_string()
}

fn to_pascal_case(snake: &str) -> String {
    snake
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

fn generate_stream_declarations(stream: &FfiStreamExport) -> String {
    let base_name = format!("mffi_{}_{}", stream.class_name, stream.method_name);
    let item_type = &stream.item_type;
    let pascal_class_name = to_pascal_case(&stream.class_name);

    format!(
        "SubscriptionHandle {}(const struct {} *handle);\n\
         uintptr_t {}_pop_batch(SubscriptionHandle subscription_handle, struct {} *output_ptr, uintptr_t output_capacity);\n\
         int32_t {}_wait(SubscriptionHandle subscription_handle, uint32_t timeout_milliseconds);\n\
         void {}_poll(SubscriptionHandle subscription_handle, uint64_t callback_data, StreamContinuationCallback callback);\n\
         void {}_unsubscribe(SubscriptionHandle subscription_handle);\n\
         void {}_free(SubscriptionHandle subscription_handle);\n\n",
        base_name,
        pascal_class_name,
        base_name,
        item_type,
        base_name,
        base_name,
        base_name,
        base_name,
    )
}

fn append_macro_exports(
    header_path: &PathBuf,
    exports: &[FfiExport],
    structs: &[FfiStruct],
    enums: &[FfiEnum],
    stream_exports: &[FfiStreamExport],
    traits: &[FfiTrait],
) {
    let mut header = fs::read_to_string(header_path).unwrap_or_default();

    if let Some(pos) = header.rfind("#endif") {
        let generic_types = collect_generic_type_instantiations(exports);

        for (wrapper, inner, _) in &generic_types {
            let empty_pattern = format!(
                "typedef struct {}_{} {{\n}} {}_{};",
                wrapper, inner, wrapper, inner
            );
            header = header.replace(&empty_pattern, "");
        }

        let generic_defs: String = generic_types
            .iter()
            .filter(|(wrapper, inner, _)| !header.contains(&format!("typedef struct {}_{} {{\n  ", wrapper, inner)))
            .map(|(wrapper, inner, c_type)| {
                if wrapper == "FfiBuf" {
                    format!(
                        "typedef struct FfiBuf_{} {{\n  {} *ptr;\n  uintptr_t len;\n  uintptr_t cap;\n}} FfiBuf_{};\n\nvoid mffi_free_buf_{}(struct FfiBuf_{} buf);\n\n",
                        inner, c_type, inner, inner, inner
                    )
                } else {
                    format!(
                        "typedef struct FfiOption_{} {{\n  bool isSome;\n  {} value;\n}} FfiOption_{};\n\nbool mffi_option_{}_is_some(struct FfiOption_{} opt);\n\n",
                        inner, c_type, inner, inner, inner
                    )
                }
            })
            .collect::<String>();

        let enum_defs: String = enums
            .iter()
            .filter(|e| {
                !header.contains(&format!("typedef {} {};", repr_to_c_type(&e.repr), e.name))
            })
            .map(generate_enum_typedef)
            .collect();

        let has_async = exports
            .iter()
            .any(|e| matches!(e.return_kind, FfiReturnKind::AsyncPoll(_)));
        let rust_future_defs = if has_async && !header.contains("RustFutureHandle") {
            "typedef const void* RustFutureHandle;\ntypedef void (*RustFutureContinuationCallback)(uint64_t callback_data, RustFuturePoll poll_result);\n\n"
        } else {
            ""
        };
        let has_streams = !stream_exports.is_empty();
        let atomic_cas_defs = if (has_async || has_streams) && !header.contains("mffi_atomic_u8_cas")
        {
            include_str!("templates/atomics.h")
        } else {
            ""
        };

        let stream_continuation_defs = if has_streams
            && !header.contains("StreamContinuationCallback")
        {
            "typedef void (*StreamContinuationCallback)(uint64_t callback_data, int8_t poll_result);\n\n"
        } else {
            ""
        };

        let struct_defs: String = structs
            .iter()
            .filter(|s| !header.contains(&format!("typedef struct {} {{", s.name)))
            .map(generate_struct_typedef)
            .collect();

        let declarations: String = exports.iter().map(generate_export_declaration).collect();

        let stream_declarations: String = stream_exports
            .iter()
            .map(generate_stream_declarations)
            .collect();

        let trait_defs: String = traits
            .iter()
            .filter(|t| !header.contains(&format!("typedef struct {}VTable", t.name)))
            .map(generate_trait_typedef)
            .collect();

        let marker = "\n/* Macro-generated types and exports */\n";
        header.insert_str(
            pos,
            &format!(
                "{}{}{}{}{}{}{}{}{}{}\n",
                marker,
                generic_defs,
                enum_defs,
                rust_future_defs,
                atomic_cas_defs,
                stream_continuation_defs,
                struct_defs,
                trait_defs,
                declarations,
                stream_declarations
            ),
        );
        fs::write(header_path, header).expect("Failed to write header");
    }
}

fn generate_trait_typedef(t: &FfiTrait) -> String {
    let trait_name = &t.name;
    let vtable_name = format!("{}VTable", trait_name);
    let foreign_name = format!("Foreign{}", trait_name);
    let snake_name = trait_name_to_snake(&trait_name);

    let mut vtable_fields = vec![
        "  void (*free)(uint64_t handle);".to_string(),
        "  uint64_t (*clone)(uint64_t handle);".to_string(),
    ];

    for method in &t.methods {
        let method_snake = trait_name_to_snake(&method.name);
        let mut params = vec!["uint64_t handle".to_string()];

        for (param_name, param_type) in &method.params {
            params.push(format!("{} {}", param_type, param_name));
        }

        if method.is_async {
            let callback_return = method
                .return_type
                .as_ref()
                .map(|t| format!(", {}", t))
                .unwrap_or_default();
            params.push(format!(
                "void (*callback)(uint64_t{}, struct FfiStatus)",
                callback_return
            ));
            params.push("uint64_t callback_data".to_string());
            vtable_fields.push(format!(
                "  void (*{})({});",
                method_snake,
                params.join(", ")
            ));
        } else {
            if let Some(ref ret_ty) = method.return_type {
                params.push(format!("{} *out", ret_ty));
            }
            params.push("struct FfiStatus *status".to_string());
            vtable_fields.push(format!(
                "  void (*{})({});",
                method_snake,
                params.join(", ")
            ));
        }
    }

    format!(
        "typedef struct {} {{\n{}\n}} {};\n\n\
         typedef struct {} {{\n  const struct {} *vtable;\n  uint64_t handle;\n}} {};\n\n\
         void mffi_register_{}_vtable(const struct {} *vtable);\n\
         struct {} *mffi_create_{}(uint64_t handle);\n\n",
        vtable_name,
        vtable_fields.join("\n"),
        vtable_name,
        foreign_name,
        vtable_name,
        foreign_name,
        snake_name,
        vtable_name,
        foreign_name,
        snake_name,
    )
}

fn trait_name_to_snake(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}
