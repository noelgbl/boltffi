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

enum ParamTransform {
    PassThrough,
    StrRef,
    OwnedString,
    Callback(Vec<syn::Type>),
    SliceRef(syn::Type),
    SliceMut(syn::Type),
}

fn extract_fn_arg_types(ty: &Type) -> Option<Vec<syn::Type>> {
    if let Type::BareFn(bare_fn) = ty {
        let args: Vec<syn::Type> = bare_fn
            .inputs
            .iter()
            .map(|arg| arg.ty.clone())
            .collect();
        return Some(args);
    }
    
    if let Type::ImplTrait(impl_trait) = ty {
        for bound in &impl_trait.bounds {
            if let syn::TypeParamBound::Trait(trait_bound) = bound {
                let path = &trait_bound.path;
                if let Some(segment) = path.segments.last() {
                    let ident = segment.ident.to_string();
                    if ident == "Fn" || ident == "FnMut" || ident == "FnOnce" {
                        if let syn::PathArguments::Parenthesized(args) = &segment.arguments {
                            let arg_types: Vec<syn::Type> = args.inputs.iter().cloned().collect();
                            return Some(arg_types);
                        }
                    }
                }
            }
        }
    }
    
    None
}

fn extract_slice_inner(ty: &Type) -> Option<(syn::Type, bool)> {
    if let Type::Reference(ref_ty) = ty {
        if let Type::Slice(slice_ty) = ref_ty.elem.as_ref() {
            let is_mut = ref_ty.mutability.is_some();
            return Some((*slice_ty.elem.clone(), is_mut));
        }
    }
    None
}

fn classify_param_transform(ty: &Type) -> ParamTransform {
    let type_str = quote::quote!(#ty).to_string().replace(" ", "");
    
    if let Some(arg_types) = extract_fn_arg_types(ty) {
        return ParamTransform::Callback(arg_types);
    }
    
    if let Some((inner_ty, is_mut)) = extract_slice_inner(ty) {
        return if is_mut {
            ParamTransform::SliceMut(inner_ty)
        } else {
            ParamTransform::SliceRef(inner_ty)
        };
    }
    
    if type_str.starts_with("*const") || type_str.starts_with("*mut") {
        return ParamTransform::PassThrough;
    }
    
    if type_str.contains("extern") && type_str.contains("fn(") {
        return ParamTransform::PassThrough;
    }
    
    if type_str == "&str" || (type_str.starts_with("&'") && type_str.ends_with("str")) {
        ParamTransform::StrRef
    } else if type_str == "String" || type_str == "std::string::String" {
        ParamTransform::OwnedString
    } else {
        ParamTransform::PassThrough
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

            match classify_param_transform(&pat_type.ty) {
                ParamTransform::StrRef => {
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
                ParamTransform::OwnedString => {
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
                ParamTransform::Callback(arg_types) => {
                    let cb_name = syn::Ident::new(&format!("{}_cb", name), name.span());
                    let ud_name = syn::Ident::new(&format!("{}_ud", name), name.span());
                    
                    ffi_params.push(quote! { #cb_name: extern "C" fn(*mut core::ffi::c_void, #(#arg_types),*) });
                    ffi_params.push(quote! { #ud_name: *mut core::ffi::c_void });
                    
                    let arg_names: Vec<syn::Ident> = arg_types
                        .iter()
                        .enumerate()
                        .map(|(i, _)| syn::Ident::new(&format!("__arg{}", i), name.span()))
                        .collect();
                    
                    conversions.push(quote! {
                        let #name = |#(#arg_names: #arg_types),*| {
                            #cb_name(#ud_name, #(#arg_names),*)
                        };
                    });
                    
                    call_args.push(quote! { #name });
                }
                ParamTransform::SliceRef(inner_ty) => {
                    let ptr_name = syn::Ident::new(&format!("{}_ptr", name), name.span());
                    let len_name = syn::Ident::new(&format!("{}_len", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *const #inner_ty });
                    ffi_params.push(quote! { #len_name: usize });

                    conversions.push(quote! {
                        let #name: &[#inner_ty] = if #ptr_name.is_null() {
                            &[]
                        } else {
                            core::slice::from_raw_parts(#ptr_name, #len_name)
                        };
                    });

                    call_args.push(quote! { #name });
                }
                ParamTransform::SliceMut(inner_ty) => {
                    let ptr_name = syn::Ident::new(&format!("{}_ptr", name), name.span());
                    let len_name = syn::Ident::new(&format!("{}_len", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *mut #inner_ty });
                    ffi_params.push(quote! { #len_name: usize });

                    conversions.push(quote! {
                        let #name: &mut [#inner_ty] = if #ptr_name.is_null() {
                            &mut []
                        } else {
                            core::slice::from_raw_parts_mut(#ptr_name, #len_name)
                        };
                    });

                    call_args.push(quote! { #name });
                }
                ParamTransform::PassThrough => {
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
    OptionPrimitive(syn::Type),
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
                    if segment.ident == "Option" {
                        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                                return ReturnKind::OptionPrimitive(inner_ty.clone());
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
        ReturnKind::OptionPrimitive(inner_ty) => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    match #fn_name(#(#call_args),*) {
                        Some(value) => {
                            *out = value;
                            1
                        }
                        None => 0
                    }
                }
            } else {
                quote! {
                    match #fn_name(#(#call_args),*) {
                        Some(value) => {
                            *out = value;
                            1
                        }
                        None => 0
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
                    ) -> i32 {
                        if out.is_null() {
                            return -1;
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
                    ) -> i32 {
                        if out.is_null() {
                            return -1;
                        }
                        match #fn_name() {
                            Some(value) => {
                                *out = value;
                                1
                            }
                            None => 0
                        }
                    }
                }
            }
        }
    };

    TokenStream::from(expanded)
}

fn to_snake_case(name: &str) -> String {
    name.to_ascii_lowercase()
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

fn transform_method_params(
    inputs: impl Iterator<Item = syn::FnArg>,
) -> FfiParams {
    let mut ffi_params = Vec::new();
    let mut conversions = Vec::new();
    let mut call_args = Vec::new();

    for arg in inputs {
        if let FnArg::Typed(pat_type) = arg {
            let name = match pat_type.pat.as_ref() {
                Pat::Ident(ident) => ident.ident.clone(),
                _ => continue,
            };

            match classify_param_transform(&pat_type.ty) {
                ParamTransform::StrRef => {
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
                ParamTransform::OwnedString => {
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
                ParamTransform::Callback(arg_types) => {
                    let cb_name = syn::Ident::new(&format!("{}_cb", name), name.span());
                    let ud_name = syn::Ident::new(&format!("{}_ud", name), name.span());
                    
                    ffi_params.push(quote! { #cb_name: extern "C" fn(*mut core::ffi::c_void, #(#arg_types),*) });
                    ffi_params.push(quote! { #ud_name: *mut core::ffi::c_void });
                    
                    let arg_names: Vec<syn::Ident> = arg_types
                        .iter()
                        .enumerate()
                        .map(|(i, _)| syn::Ident::new(&format!("__arg{}", i), name.span()))
                        .collect();
                    
                    conversions.push(quote! {
                        let #name = |#(#arg_names: #arg_types),*| {
                            #cb_name(#ud_name, #(#arg_names),*)
                        };
                    });
                    
                    call_args.push(quote! { #name });
                }
                ParamTransform::SliceRef(inner_ty) => {
                    let ptr_name = syn::Ident::new(&format!("{}_ptr", name), name.span());
                    let len_name = syn::Ident::new(&format!("{}_len", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *const #inner_ty });
                    ffi_params.push(quote! { #len_name: usize });

                    conversions.push(quote! {
                        let #name: &[#inner_ty] = if #ptr_name.is_null() {
                            &[]
                        } else {
                            core::slice::from_raw_parts(#ptr_name, #len_name)
                        };
                    });

                    call_args.push(quote! { #name });
                }
                ParamTransform::SliceMut(inner_ty) => {
                    let ptr_name = syn::Ident::new(&format!("{}_ptr", name), name.span());
                    let len_name = syn::Ident::new(&format!("{}_len", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *mut #inner_ty });
                    ffi_params.push(quote! { #len_name: usize });

                    conversions.push(quote! {
                        let #name: &mut [#inner_ty] = if #ptr_name.is_null() {
                            &mut []
                        } else {
                            core::slice::from_raw_parts_mut(#ptr_name, #len_name)
                        };
                    });

                    call_args.push(quote! { #name });
                }
                ParamTransform::PassThrough => {
                    let ty = &pat_type.ty;
                    ffi_params.push(quote! { #name: #ty });
                    call_args.push(quote! { #name });
                }
            }
        }
    }

    FfiParams { ffi_params, conversions, call_args }
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

    let other_inputs = method.sig.inputs.iter().skip(1).cloned();
    let FfiParams { ffi_params, conversions, call_args } = transform_method_params(other_inputs);

    let fn_output = &method.sig.output;
    let has_conversions = !conversions.is_empty();
    let is_unit_return = matches!(fn_output, ReturnType::Default);

    let call_expr = quote! { (*handle).#method_name(#(#call_args),*) };

    let (body, return_type) = if is_unit_return {
        let b = if has_conversions {
            quote! {
                #(#conversions)*
                #call_expr;
                crate::FfiStatus::OK
            }
        } else {
            quote! {
                #call_expr;
                crate::FfiStatus::OK
            }
        };
        (b, quote! { -> crate::FfiStatus })
    } else {
        let b = if has_conversions {
            quote! {
                #(#conversions)*
                #call_expr
            }
        } else {
            call_expr
        };
        (b, quote! { #fn_output })
    };

    if ffi_params.is_empty() {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name
            ) #return_type {
                #body
            }
        })
    } else {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name,
                #(#ffi_params),*
            ) #return_type {
                #body
            }
        })
    }
}
