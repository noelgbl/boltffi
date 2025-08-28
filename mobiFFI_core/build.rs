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

        for item in syntax.items {
            if let syn::Item::Fn(func) = item {
                if has_ffi_export_attr(&func) {
                    if let Some(export) = parse_ffi_function(&func) {
                        exports.push(export);
                    }
                }
            }
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

fn parse_ffi_function(func: &ItemFn) -> Option<FfiExport> {
    let name = func.sig.ident.to_string();

    let params: Vec<(String, String)> = func
        .sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pat_type) = arg {
                let param_name = match pat_type.pat.as_ref() {
                    Pat::Ident(ident) => ident.ident.to_string(),
                    _ => return None,
                };
                let param_type = rust_type_to_c(&pat_type.ty)?;
                Some((param_name, param_type))
            } else {
                None
            }
        })
        .collect();

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

fn append_macro_exports(header_path: &PathBuf, exports: &[FfiExport]) {
    let mut header = fs::read_to_string(header_path).unwrap_or_default();

    if let Some(pos) = header.rfind("#endif") {
        let declarations: String = exports
            .iter()
            .map(|e| {
                let mut params: Vec<String> = e
                    .params
                    .iter()
                    .map(|(name, ty)| format!("{} {}", ty, name))
                    .collect();

                let ret_type = match &e.return_kind {
                    FfiReturnKind::Unit => {
                        "struct FfiStatus".to_string()
                    }
                    FfiReturnKind::Primitive(ty) => ty.clone(),
                    FfiReturnKind::String | FfiReturnKind::ResultString => {
                        params.push("struct FfiString *out".to_string());
                        "struct FfiStatus".to_string()
                    }
                    FfiReturnKind::ResultPrimitive(ty) => {
                        params.push(format!("{} *out", ty));
                        "struct FfiStatus".to_string()
                    }
                };

                let params_str = if params.is_empty() {
                    "void".to_string()
                } else {
                    params.join(", ")
                };

                format!("{} mffi_{}({});\n", ret_type, e.name, params_str)
            })
            .collect();

        let marker = "\n/* Macro-generated exports */\n";
        header.insert_str(pos, &format!("{}{}\n", marker, declarations));
        fs::write(header_path, header).expect("Failed to write header");
    }
}
