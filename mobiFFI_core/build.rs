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

    let macro_exports = collect_ffi_exports(&crate_dir.join("src"));
    let repr_c_structs = collect_repr_c_structs(&crate_dir.join("src"));
    let ffi_enums = collect_ffi_enums(&crate_dir.join("src"));
    if !macro_exports.is_empty() || !repr_c_structs.is_empty() || !ffi_enums.is_empty() {
        append_macro_exports(&header_path, &macro_exports, &repr_c_structs, &ffi_enums);
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
}

struct FfiExport {
    name: String,
    params: Vec<(String, String)>,
    return_kind: FfiReturnKind,
}

struct FfiStruct {
    name: String,
    fields: Vec<(String, String)>,
}

struct FfiEnum {
    name: String,
    repr: String,
    variants: Vec<(String, i64)>,
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
                let repr = e.attrs.iter().find_map(|attr| {
                    if attr.path().is_ident("repr") {
                        if let Ok(arg) = attr.parse_args::<syn::Ident>() {
                            let r = arg.to_string();
                            if matches!(r.as_str(), "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64") {
                                return Some(r);
                            }
                        }
                    }
                    None
                });

                if let Some(repr) = repr {
                    let mut variants = Vec::new();
                    let mut next_value: i64 = 0;

                    for variant in &e.variants {
                        if let Some((_, expr)) = &variant.discriminant {
                            if let syn::Expr::Lit(lit) = expr {
                                if let syn::Lit::Int(int_lit) = &lit.lit {
                                    next_value = int_lit.base10_parse().unwrap_or(next_value);
                                }
                            }
                        }
                        variants.push((variant.ident.to_string(), next_value));
                        next_value += 1;
                    }

                    enums.push(FfiEnum {
                        name: e.ident.to_string(),
                        repr,
                        variants,
                    });
                }
            }
        }
    }

    enums
}

fn collect_ffi_exports(src_dir: &PathBuf) -> Vec<FfiExport> {
    let mut exports = Vec::new();

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
                    exports.extend(parse_ffi_class(impl_block));
                }
            }
        }
    }

    exports
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

fn parse_ffi_class(impl_block: &syn::ItemImpl) -> Vec<FfiExport> {
    let mut exports = Vec::new();

    let type_name = match impl_block.self_ty.as_ref() {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    };

    let type_name = match type_name {
        Some(name) => name,
        None => return exports,
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

            let method_name = method.sig.ident.to_string();
            if method_name == "new" {
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

    exports
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

fn parse_ffi_function(func: &ItemFn) -> Option<FfiExport> {
    let name = func.sig.ident.to_string();

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
        _ => {
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
        _ => return Some(format!("{} struct {}*", qualifier, inner).trim().to_string()),
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
                FfiReturnKind::Vec(_) => unreachable!(),
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

fn generate_struct_typedef(s: &FfiStruct) -> String {
    let fields: String = s
        .fields
        .iter()
        .map(|(name, ty)| format!("  {} {};\n", ty, name))
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
    let mut out = format!("typedef {} {};\n", c_type, e.name);
    for (variant, value) in &e.variants {
        out.push_str(&format!("#define {}_{} {}\n", e.name, variant, value));
    }
    out.push('\n');
    out
}

fn append_macro_exports(header_path: &PathBuf, exports: &[FfiExport], structs: &[FfiStruct], enums: &[FfiEnum]) {
    let mut header = fs::read_to_string(header_path).unwrap_or_default();

    if let Some(pos) = header.rfind("#endif") {
        let enum_defs: String = enums
            .iter()
            .filter(|e| !header.contains(&format!("typedef {} {};", repr_to_c_type(&e.repr), e.name)))
            .map(generate_enum_typedef)
            .collect();

        let struct_defs: String = structs
            .iter()
            .filter(|s| !header.contains(&format!("typedef struct {} {{", s.name)))
            .map(generate_struct_typedef)
            .collect();

        let declarations: String = exports
            .iter()
            .map(generate_export_declaration)
            .collect();

        let marker = "\n/* Macro-generated types and exports */\n";
        header.insert_str(pos, &format!("{}{}{}{}\n", marker, enum_defs, struct_defs, declarations));
        fs::write(header_path, header).expect("Failed to write header");
    }
}
