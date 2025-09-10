use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, FnArg, ItemFn, Pat, ReturnType, Type};

#[proc_macro_derive(FfiType)]
pub fn derive_ffi_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let has_repr_c = input.attrs.iter().any(|attr| {
        attr.path().is_ident("repr")
            && attr
                .parse_args::<syn::Ident>()
                .map(|id| id == "C")
                .unwrap_or(false)
    });

    if !has_repr_c {
        return syn::Error::new_spanned(&input, "FfiType requires #[repr(C)]")
            .to_compile_error()
            .into();
    }

    TokenStream::from(quote! {})
}

enum ParamKind {
    StrRef(syn::Ident),
    Primitive(syn::PatType),
}

fn classify_param(pat_type: &syn::PatType) -> ParamKind {
    let type_str = quote::quote!(#pat_type.ty).to_string().replace(" ", "");
    let name = match pat_type.pat.as_ref() {
        Pat::Ident(ident) => ident.ident.clone(),
        _ => syn::Ident::new("arg", proc_macro2::Span::call_site()),
    };

    if type_str.contains("&str") || type_str.contains("&'") && type_str.contains("str") {
        ParamKind::StrRef(name)
    } else {
        ParamKind::Primitive(pat_type.clone())
    }
}

enum StringParamKind {
    None,
    StrRef,
    OwnedString,
}

fn classify_string_param(ty: &Type) -> StringParamKind {
    let type_str = quote::quote!(#ty).to_string().replace(" ", "");
    if type_str == "&str" || (type_str.starts_with("&'") && type_str.ends_with("str")) {
        StringParamKind::StrRef
    } else if type_str == "String" || type_str == "std::string::String" {
        StringParamKind::OwnedString
    } else {
        StringParamKind::None
    }
}

struct FfiParams {
    ffi_params: Vec<proc_macro2::TokenStream>,
    conversions: Vec<proc_macro2::TokenStream>,
    call_args: Vec<proc_macro2::TokenStream>,
}

fn transform_params(inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>) -> FfiParams {
    let mut ffi_params = Vec::new();
    let mut conversions = Vec::new();
    let mut call_args = Vec::new();

    for arg in inputs.iter() {
        if let FnArg::Typed(pat_type) = arg {
            let name = match pat_type.pat.as_ref() {
                Pat::Ident(ident) => ident.ident.clone(),
                _ => continue,
            };

            match classify_string_param(&pat_type.ty) {
                StringParamKind::StrRef => {
                    let ptr_name = syn::Ident::new(&format!("{}_ptr", name), name.span());
                    let len_name = syn::Ident::new(&format!("{}_len", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *const u8 });
                    ffi_params.push(quote! { #len_name: usize });

                    conversions.push(quote! {
                        let #name: &str = if #ptr_name.is_null() {
                            ""
                        } else {
                            match core::str::from_utf8(core::slice::from_raw_parts(#ptr_name, #len_name)) {
                                Ok(s) => s,
                                Err(_) => return crate::fail_with_error(
                                    crate::FfiStatus::INVALID_ARG,
                                    concat!(stringify!(#name), " is not valid UTF-8")
                                ),
                            }
                        };
                    });

                    call_args.push(quote! { #name });
                }
                StringParamKind::OwnedString => {
                    let ptr_name = syn::Ident::new(&format!("{}_ptr", name), name.span());
                    let len_name = syn::Ident::new(&format!("{}_len", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *const u8 });
                    ffi_params.push(quote! { #len_name: usize });

                    conversions.push(quote! {
                        let #name: String = if #ptr_name.is_null() {
                            String::new()
                        } else {
                            match core::str::from_utf8(core::slice::from_raw_parts(#ptr_name, #len_name)) {
                                Ok(s) => s.to_string(),
                                Err(_) => return crate::fail_with_error(
                                    crate::FfiStatus::INVALID_ARG,
                                    concat!(stringify!(#name), " is not valid UTF-8")
                                ),
                            }
                        };
                    });

                    call_args.push(quote! { #name });
                }
                StringParamKind::None => {
                    let ty = &pat_type.ty;
                    ffi_params.push(quote! { #name: #ty });
                    call_args.push(quote! { #name });
                }
            }
        }
    }

    FfiParams { ffi_params, conversions, call_args }
}

enum ReturnKind {
    Unit,
    Primitive,
    String,
    ResultPrimitive(syn::Type),
    ResultString,
    Vec(syn::Type),
}

fn extract_vec_inner(ty: &Type) -> Option<syn::Type> {
    if let Type::Path(path) = ty {
        if let Some(segment) = path.path.segments.last() {
            if segment.ident == "Vec" {
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

fn classify_return(output: &ReturnType) -> ReturnKind {
    match output {
        ReturnType::Default => ReturnKind::Unit,
        ReturnType::Type(_, ty) => {
            let type_str = quote::quote!(#ty).to_string().replace(" ", "");

            if type_str == "String" || type_str == "std::string::String" {
                return ReturnKind::String;
            }

            if let Some(inner) = extract_vec_inner(ty) {
                return ReturnKind::Vec(inner);
            }

            if let Type::Path(path) = ty.as_ref() {
                if let Some(segment) = path.path.segments.last() {
                    if segment.ident == "Result" {
                        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                                let inner_str = quote::quote!(#inner_ty).to_string().replace(" ", "");
                                if inner_str == "String" || inner_str == "std::string::String" {
                                    return ReturnKind::ResultString;
                                } else if inner_str == "()" {
                                    return ReturnKind::Unit;
                                } else {
                                    return ReturnKind::ResultPrimitive(inner_ty.clone());
                                }
                            }
                        }
                    }
                }
            }

            ReturnKind::Primitive
        }
    }
}

#[proc_macro_attribute]
pub fn ffi_export(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let fn_name = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_vis = &input.vis;

    let export_name = format!("mffi_{}", fn_name);
    let export_ident = syn::Ident::new(&export_name, fn_name.span());

    let FfiParams { ffi_params, conversions, call_args } = transform_params(fn_inputs);

    let has_params = !ffi_params.is_empty();
    let has_conversions = !conversions.is_empty();

    let expanded = match classify_return(fn_output) {
        ReturnKind::String => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    let result = #fn_name(#(#call_args),*);
                    *out = crate::FfiString::from(result);
                    crate::FfiStatus::OK
                }
            } else {
                quote! {
                    let result = #fn_name(#(#call_args),*);
                    *out = crate::FfiString::from(result);
                    crate::FfiStatus::OK
                }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        #(#ffi_params),*,
                        out: *mut crate::FfiString
                    ) -> crate::FfiStatus {
                        if out.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        out: *mut crate::FfiString
                    ) -> crate::FfiStatus {
                        if out.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #body
                    }
                }
            }
        }
        ReturnKind::ResultString => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    match #fn_name(#(#call_args),*) {
                        Ok(value) => {
                            *out = crate::FfiString::from(value);
                            crate::FfiStatus::OK
                        }
                        Err(e) => crate::fail_with_error(crate::FfiStatus::INTERNAL_ERROR, &e.to_string())
                    }
                }
            } else {
                quote! {
                    match #fn_name(#(#call_args),*) {
                        Ok(value) => {
                            *out = crate::FfiString::from(value);
                            crate::FfiStatus::OK
                        }
                        Err(e) => crate::fail_with_error(crate::FfiStatus::INTERNAL_ERROR, &e.to_string())
                    }
                }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        #(#ffi_params),*,
                        out: *mut crate::FfiString
                    ) -> crate::FfiStatus {
                        if out.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        out: *mut crate::FfiString
                    ) -> crate::FfiStatus {
                        if out.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #body
                    }
                }
            }
        }
        ReturnKind::ResultPrimitive(inner_ty) => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    match #fn_name(#(#call_args),*) {
                        Ok(value) => {
                            *out = value;
                            crate::FfiStatus::OK
                        }
                        Err(e) => crate::fail_with_error(crate::FfiStatus::INTERNAL_ERROR, &e.to_string())
                    }
                }
            } else {
                quote! {
                    match #fn_name(#(#call_args),*) {
                        Ok(value) => {
                            *out = value;
                            crate::FfiStatus::OK
                        }
                        Err(e) => crate::fail_with_error(crate::FfiStatus::INTERNAL_ERROR, &e.to_string())
                    }
                }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        #(#ffi_params),*,
                        out: *mut #inner_ty
                    ) -> crate::FfiStatus {
                        if out.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        out: *mut #inner_ty
                    ) -> crate::FfiStatus {
                        if out.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #body
                    }
                }
            }
        }
        ReturnKind::Unit => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    #fn_name(#(#call_args),*);
                    crate::FfiStatus::OK
                }
            } else {
                quote! {
                    #fn_name(#(#call_args),*);
                    crate::FfiStatus::OK
                }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(#(#ffi_params),*) -> crate::FfiStatus {
                        #body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis extern "C" fn #export_ident() -> crate::FfiStatus {
                        #fn_name();
                        crate::FfiStatus::OK
                    }
                }
            }
        }
        ReturnKind::Primitive => {
            let fn_output = &input.sig.output;
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    #fn_name(#(#call_args),*)
                }
            } else {
                quote! { #fn_name(#(#call_args),*) }
            };

            if has_params {
                if has_conversions {
                    quote! {
                        #input

                        #[unsafe(no_mangle)]
                        #fn_vis unsafe extern "C" fn #export_ident(#(#ffi_params),*) #fn_output {
                            #body
                        }
                    }
                } else {
                    quote! {
                        #input

                        #[unsafe(no_mangle)]
                        #fn_vis extern "C" fn #export_ident(#(#ffi_params),*) #fn_output {
                            #body
                        }
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis extern "C" fn #export_ident() #fn_output {
                        #fn_name()
                    }
                }
            }
        }
        ReturnKind::Vec(inner_ty) => {
            let len_ident = syn::Ident::new(&format!("mffi_{}_len", fn_name), fn_name.span());
            let copy_into_ident = syn::Ident::new(&format!("mffi_{}_copy_into", fn_name), fn_name.span());

            let len_body = if has_conversions {
                quote! {
                    #(#conversions)*
                    #fn_name(#(#call_args),*).len()
                }
            } else {
                quote! { #fn_name(#(#call_args),*).len() }
            };

            let copy_body = if has_conversions {
                quote! {
                    #(#conversions)*
                    let items = #fn_name(#(#call_args),*);
                    let to_copy = items.len().min(dst_cap);
                    core::ptr::copy_nonoverlapping(items.as_ptr(), dst, to_copy);
                    *written = to_copy;
                    if to_copy < items.len() {
                        crate::FfiStatus::BUFFER_TOO_SMALL
                    } else {
                        crate::FfiStatus::OK
                    }
                }
            } else {
                quote! {
                    let items = #fn_name(#(#call_args),*);
                    let to_copy = items.len().min(dst_cap);
                    core::ptr::copy_nonoverlapping(items.as_ptr(), dst, to_copy);
                    *written = to_copy;
                    if to_copy < items.len() {
                        crate::FfiStatus::BUFFER_TOO_SMALL
                    } else {
                        crate::FfiStatus::OK
                    }
                }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #len_ident(#(#ffi_params),*) -> usize {
                        #len_body
                    }

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #copy_into_ident(
                        #(#ffi_params),*,
                        dst: *mut #inner_ty,
                        dst_cap: usize,
                        written: *mut usize
                    ) -> crate::FfiStatus {
                        if dst.is_null() || written.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #copy_body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis extern "C" fn #len_ident() -> usize {
                        #fn_name().len()
                    }

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #copy_into_ident(
                        dst: *mut #inner_ty,
                        dst_cap: usize,
                        written: *mut usize
                    ) -> crate::FfiStatus {
                        if dst.is_null() || written.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        let items = #fn_name();
                        let to_copy = items.len().min(dst_cap);
                        core::ptr::copy_nonoverlapping(items.as_ptr(), dst, to_copy);
                        *written = to_copy;
                        if to_copy < items.len() {
                            crate::FfiStatus::BUFFER_TOO_SMALL
                        } else {
                            crate::FfiStatus::OK
                        }
                    }
                }
            }
        }
    };

    TokenStream::from(expanded)
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

#[proc_macro_attribute]
pub fn ffi_class(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as syn::ItemImpl);

    let self_ty = match input.self_ty.as_ref() {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.clone()),
        _ => None,
    };

    let type_name = match self_ty {
        Some(name) => name,
        None => {
            return syn::Error::new_spanned(&input, "ffi_class requires a named type")
                .to_compile_error()
                .into();
        }
    };

    let snake_name = to_snake_case(&type_name.to_string());
    let new_ident = syn::Ident::new(&format!("mffi_{}_new", snake_name), type_name.span());
    let free_ident = syn::Ident::new(&format!("mffi_{}_free", snake_name), type_name.span());

    let method_exports: Vec<_> = input
        .items
        .iter()
        .filter_map(|item| {
            if let syn::ImplItem::Fn(method) = item {
                if method.vis == syn::Visibility::Public(syn::token::Pub::default()) {
                    return generate_method_export(&type_name, &snake_name, method);
                }
            }
            None
        })
        .collect();

    let expanded = quote! {
        #input

        #[unsafe(no_mangle)]
        pub extern "C" fn #new_ident() -> *mut #type_name {
            Box::into_raw(Box::new(#type_name::new()))
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #free_ident(handle: *mut #type_name) {
            if !handle.is_null() {
                drop(Box::from_raw(handle));
            }
        }

        #(#method_exports)*
    };

    TokenStream::from(expanded)
}

fn generate_method_export(
    type_name: &syn::Ident,
    snake_name: &str,
    method: &syn::ImplItemFn,
) -> Option<proc_macro2::TokenStream> {
    let method_name = &method.sig.ident;
    let export_name = syn::Ident::new(
        &format!("mffi_{}_{}", snake_name, method_name),
        method_name.span(),
    );

    let has_self = method
        .sig
        .inputs
        .first()
        .map(|arg| matches!(arg, FnArg::Receiver(_)))
        .unwrap_or(false);

    if !has_self {
        return None;
    }

    let other_args: Vec<_> = method
        .sig
        .inputs
        .iter()
        .skip(1)
        .filter_map(|arg| {
            if let FnArg::Typed(pat_type) = arg {
                Some(pat_type)
            } else {
                None
            }
        })
        .collect();

    let arg_idents: Vec<_> = other_args
        .iter()
        .filter_map(|pt| {
            if let Pat::Ident(ident) = pt.pat.as_ref() {
                Some(&ident.ident)
            } else {
                None
            }
        })
        .collect();

    let fn_output = &method.sig.output;

    let call_expr = quote! { (*handle).#method_name(#(#arg_idents),*) };

    Some(quote! {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #export_name(
            handle: *mut #type_name,
            #(#other_args),*
        ) #fn_output {
            #call_expr
        }
    })
}
