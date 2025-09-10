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
    if !macro_exports.is_empty() {
        append_macro_exports(&header_path, &macro_exports);
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
}

struct FfiExport {
    name: String,
    params: Vec<(String, String)>,
    return_kind: FfiReturnKind,
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
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
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
                    if let Some(c_type) = rust_type_to_c(&pat_type.ty) {
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
            if type_str.starts_with("*const") {
                let inner = type_str.trim_start_matches("*const");
                rust_type_to_c_ptr(inner, "const")
            } else if type_str.starts_with("*mut") {
                let inner = type_str.trim_start_matches("*mut");
                rust_type_to_c_ptr(inner, "")
            } else {
                Some(format!("struct {}", type_str))
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

fn generate_export_declaration(export: &FfiExport) -> String {
    let base_params: Vec<String> = export
        .params
        .iter()
        .map(|(name, ty)| format!("{} {}", ty, name))
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

fn append_macro_exports(header_path: &PathBuf, exports: &[FfiExport]) {
    let mut header = fs::read_to_string(header_path).unwrap_or_default();

    if let Some(pos) = header.rfind("#endif") {
        let declarations: String = exports
            .iter()
            .map(generate_export_declaration)
            .collect();

        let marker = "\n/* Macro-generated exports */\n";
        header.insert_str(pos, &format!("{}{}\n", marker, declarations));
        fs::write(header_path, header).expect("Failed to write header");
    }
}
