use quote::quote;
use syn::{ReturnType, Type};

pub enum OptionReturnAbi {
    OutValue { inner: syn::Type },
    OutFfiString,
    Vec { inner: syn::Type },
}

pub enum ReturnKind {
    Unit,
    Primitive,
    String,
    ResultPrimitive { ok: syn::Type, err: syn::Type },
    ResultString { err: syn::Type },
    ResultUnit { err: syn::Type },
    Vec(syn::Type),
    Option(OptionReturnAbi),
}

pub enum AsyncErrorKind {
    StringLike(proc_macro2::TokenStream),
    Typed(proc_macro2::TokenStream),
}

pub struct AsyncResultInfo {
    pub ok_type: proc_macro2::TokenStream,
    pub err_kind: AsyncErrorKind,
}

pub enum AsyncReturnKind {
    Unit,
    Primitive(proc_macro2::TokenStream),
    String,
    Struct(proc_macro2::TokenStream),
    Vec(proc_macro2::TokenStream),
    Option(proc_macro2::TokenStream),
    Result(AsyncResultInfo),
}

pub fn extract_vec_inner(ty: &Type) -> Option<syn::Type> {
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
        && segment.ident == "Vec"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty.clone());
    }
    None
}

pub fn extract_generic_inner(ty: &Type, wrapper: &str) -> Option<syn::Type> {
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
        && segment.ident == wrapper
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty.clone());
    }
    None
}

fn is_string_like_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path.path.segments.last().is_some_and(|s| s.ident == "String"),
        Type::Reference(reference) => match reference.elem.as_ref() {
            Type::Path(path) => path.path.segments.last().is_some_and(|s| s.ident == "str"),
            _ => false,
        },
        _ => false,
    }
}

pub fn is_primitive_type(s: &str) -> bool {
    matches!(
        s,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "f32"
            | "f64"
            | "bool"
            | "usize"
            | "isize"
            | "()"
    )
}

pub fn classify_return(output: &ReturnType) -> ReturnKind {
    match output {
        ReturnType::Default => ReturnKind::Unit,
        ReturnType::Type(_, ty) => {
            let type_str = quote!(#ty).to_string().replace(' ', "");

            if type_str == "String" || type_str == "std::string::String" {
                return ReturnKind::String;
            }

            if let Some(inner) = extract_vec_inner(ty) {
                return ReturnKind::Vec(inner);
            }

            if let Type::Path(path) = ty.as_ref()
                && let Some(segment) = path.path.segments.last()
            {
                if segment.ident == "Result"
                    && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                    && args.args.len() >= 2
                    && let Some(syn::GenericArgument::Type(ok_ty)) = args.args.first()
                    && let Some(syn::GenericArgument::Type(err_ty)) = args.args.iter().nth(1)
                {
                    let ok_str = quote!(#ok_ty).to_string().replace(' ', "");
                    if ok_str == "String" || ok_str == "std::string::String" {
                        return ReturnKind::ResultString { err: err_ty.clone() };
                    } else if ok_str == "()" {
                        return ReturnKind::ResultUnit { err: err_ty.clone() };
                    } else {
                        return ReturnKind::ResultPrimitive { ok: ok_ty.clone(), err: err_ty.clone() };
                    }
                }
                if segment.ident == "Option"
                    && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                    && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
                {
                    if let Some(vec_inner) = extract_vec_inner(inner_ty) {
                        return ReturnKind::Option(OptionReturnAbi::Vec { inner: vec_inner });
                    }

                    if is_string_like_type(inner_ty) {
                        return ReturnKind::Option(OptionReturnAbi::OutFfiString);
                    }

                    return ReturnKind::Option(OptionReturnAbi::OutValue {
                        inner: inner_ty.clone(),
                    });
                }
            }

            ReturnKind::Primitive
        }
    }
}

fn extract_result_types(ty: &Type) -> Option<(syn::Type, syn::Type)> {
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
        && segment.ident == "Result"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && args.args.len() >= 2
        && let Some(syn::GenericArgument::Type(ok_ty)) = args.args.first()
        && let Some(syn::GenericArgument::Type(err_ty)) = args.args.iter().nth(1)
    {
        return Some((ok_ty.clone(), err_ty.clone()));
    }
    None
}

fn classify_error_kind(err_ty: &syn::Type) -> AsyncErrorKind {
    let err_str = quote!(#err_ty).to_string().replace(' ', "");
    if err_str.contains("str") || err_str == "String" || err_str == "std::string::String" {
        AsyncErrorKind::StringLike(quote! { #err_ty })
    } else {
        AsyncErrorKind::Typed(quote! { #err_ty })
    }
}

pub fn classify_async_return(output: &ReturnType) -> AsyncReturnKind {
    match output {
        ReturnType::Default => AsyncReturnKind::Unit,
        ReturnType::Type(_, ty) => {
            let type_str = quote!(#ty).to_string().replace(' ', "");

            if type_str == "String" || type_str == "std::string::String" {
                return AsyncReturnKind::String;
            }

            if let Some(inner_ty) = extract_generic_inner(ty, "Vec") {
                return AsyncReturnKind::Vec(quote! { #inner_ty });
            }

            if let Some(inner_ty) = extract_generic_inner(ty, "Option") {
                return AsyncReturnKind::Option(quote! { #inner_ty });
            }

            if let Some((ok_ty, err_ty)) = extract_result_types(ty) {
                let ok_str = quote!(#ok_ty).to_string().replace(' ', "");
                let err_kind = classify_error_kind(&err_ty);
                
                let ok_type = if ok_str == "()" {
                    quote! { () }
                } else if ok_str == "String" || ok_str == "std::string::String" {
                    quote! { String }
                } else if let Some(vec_inner) = extract_generic_inner(&ok_ty, "Vec") {
                    quote! { Vec<#vec_inner> }
                } else {
                    quote! { #ok_ty }
                };
                
                return AsyncReturnKind::Result(AsyncResultInfo { ok_type, err_kind });
            }

            if is_primitive_type(&type_str) {
                AsyncReturnKind::Primitive(quote! { #ty })
            } else {
                AsyncReturnKind::Struct(quote! { #ty })
            }
        }
    }
}

pub fn get_ffi_return_type(return_kind: &AsyncReturnKind) -> proc_macro2::TokenStream {
    match return_kind {
        AsyncReturnKind::Unit => quote! { () },
        AsyncReturnKind::Primitive(ty) => quote! { #ty },
        AsyncReturnKind::String => quote! { crate::FfiString },
        AsyncReturnKind::Struct(ty) => quote! { #ty },
        AsyncReturnKind::Vec(inner_ty) => quote! { crate::FfiBuf<#inner_ty> },
        AsyncReturnKind::Option(inner_ty) => quote! { crate::FfiOption<#inner_ty> },
        AsyncReturnKind::Result(info) => {
            let ok = &info.ok_type;
            let ok_str = quote!(#ok).to_string().replace(' ', "");
            if ok_str == "()" {
                quote! { () }
            } else if ok_str == "String" {
                quote! { crate::FfiString }
            } else if ok_str.starts_with("Vec<") {
                let inner = extract_generic_inner_from_tokens(ok);
                quote! { crate::FfiBuf<#inner> }
            } else {
                quote! { #ok }
            }
        }
    }
}

fn extract_generic_inner_from_tokens(tokens: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let s = tokens.to_string();
    if let Some(start) = s.find('<') {
        if let Some(end) = s.rfind('>') {
            let inner = &s[start + 1..end];
            return inner.parse().unwrap_or_else(|_| quote! { () });
        }
    }
    quote! { () }
}

pub fn get_rust_return_type(return_kind: &AsyncReturnKind) -> proc_macro2::TokenStream {
    match return_kind {
        AsyncReturnKind::Unit => quote! { () },
        AsyncReturnKind::Primitive(ty) => quote! { #ty },
        AsyncReturnKind::String => quote! { String },
        AsyncReturnKind::Struct(ty) => quote! { #ty },
        AsyncReturnKind::Vec(inner_ty) => quote! { Vec<#inner_ty> },
        AsyncReturnKind::Option(inner_ty) => quote! { Option<#inner_ty> },
        AsyncReturnKind::Result(info) => {
            let ok = &info.ok_type;
            match &info.err_kind {
                AsyncErrorKind::StringLike(err) | AsyncErrorKind::Typed(err) => quote! { Result<#ok, #err> },
            }
        }
    }
}

pub fn get_complete_conversion(return_kind: &AsyncReturnKind) -> proc_macro2::TokenStream {
    match return_kind {
        AsyncReturnKind::Unit => quote! {
            if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
            ()
        },
        AsyncReturnKind::Primitive(_) => quote! {
            if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
            result
        },
        AsyncReturnKind::String => quote! {
            if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
            crate::FfiString::from(result)
        },
        AsyncReturnKind::Struct(_) => quote! {
            if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
            result
        },
        AsyncReturnKind::Vec(_) => quote! {
            if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
            crate::FfiBuf::from_vec(result)
        },
        AsyncReturnKind::Option(_) => quote! {
            if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
            match result {
                Some(v) => crate::FfiOption { is_some: 1, value: v },
                None => crate::FfiOption { is_some: 0, value: Default::default() },
            }
        },
        AsyncReturnKind::Result(info) => {
            let ok_convert = get_result_ok_conversion(&info.ok_type);
            let err_write = get_result_err_write(&info.err_kind);
            let default_val = get_result_default(&info.ok_type);
            quote! {
                match result {
                    Ok(v) => {
                        if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
                        #ok_convert
                    }
                    Err(e) => {
                        if !out_status.is_null() { *out_status = crate::FfiStatus::INTERNAL_ERROR; }
                        #err_write
                        #default_val
                    }
                }
            }
        }
    }
}

fn get_result_ok_conversion(ok_type: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let ok_str = ok_type.to_string().replace(' ', "");
    if ok_str == "()" {
        quote! { () }
    } else if ok_str == "String" {
        quote! { crate::FfiString::from(v) }
    } else if ok_str.starts_with("Vec<") {
        quote! { crate::FfiBuf::from_vec(v) }
    } else {
        quote! { v }
    }
}

fn get_result_err_write(err_kind: &AsyncErrorKind) -> proc_macro2::TokenStream {
    match err_kind {
        AsyncErrorKind::StringLike(_) => quote! {
            if !out_err.is_null() { *out_err = crate::FfiError::new(e.to_string()); }
        },
        AsyncErrorKind::Typed(_) => quote! {
            if !out_err.is_null() { *out_err = e; }
        },
    }
}

fn get_result_default(ok_type: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let ok_str = ok_type.to_string().replace(' ', "");
    if ok_str == "()" {
        quote! { () }
    } else if ok_str == "String" {
        quote! { crate::FfiString::default() }
    } else if ok_str.starts_with("Vec<") {
        quote! { crate::FfiBuf::default() }
    } else {
        quote! { Default::default() }
    }
}

pub fn get_default_ffi_value(return_kind: &AsyncReturnKind) -> proc_macro2::TokenStream {
    match return_kind {
        AsyncReturnKind::Unit => quote! { () },
        AsyncReturnKind::Primitive(_) => quote! { Default::default() },
        AsyncReturnKind::String => quote! { crate::FfiString::default() },
        AsyncReturnKind::Struct(_) => quote! { Default::default() },
        AsyncReturnKind::Vec(_) => quote! { crate::FfiBuf::default() },
        AsyncReturnKind::Option(_) => {
            quote! { crate::FfiOption { is_some: 0, value: Default::default() } }
        }
        AsyncReturnKind::Result(info) => get_result_default(&info.ok_type),
    }
}
