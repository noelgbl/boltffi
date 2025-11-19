use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, FnArg, ItemFn, Pat, ReturnType, Type, parse_macro_input};

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
    BoxedTrait(syn::Ident),
    VecParam(syn::Type),
}

fn extract_fn_arg_types(ty: &Type) -> Option<Vec<syn::Type>> {
    if let Type::BareFn(bare_fn) = ty {
        let args: Vec<syn::Type> = bare_fn.inputs.iter().map(|arg| arg.ty.clone()).collect();
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

fn extract_boxed_dyn_trait(ty: &Type) -> Option<syn::Ident> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Box" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::TraitObject(trait_obj))) =
                        args.args.first()
                    {
                        if let Some(syn::TypeParamBound::Trait(trait_bound)) =
                            trait_obj.bounds.first()
                        {
                            if let Some(seg) = trait_bound.path.segments.last() {
                                return Some(seg.ident.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_vec_param_inner(ty: &Type) -> Option<syn::Type> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
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

    if let Some(trait_name) = extract_boxed_dyn_trait(ty) {
        return ParamTransform::BoxedTrait(trait_name);
    }

    if type_str.starts_with("*const") || type_str.starts_with("*mut") {
        return ParamTransform::PassThrough;
    }

    if type_str.contains("extern") && type_str.contains("fn(") {
        return ParamTransform::PassThrough;
    }

    if let Some(inner_ty) = extract_vec_param_inner(ty) {
        return ParamTransform::VecParam(inner_ty);
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

                    ffi_params.push(
                        quote! { #cb_name: extern "C" fn(*mut core::ffi::c_void, #(#arg_types),*) },
                    );
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
                ParamTransform::BoxedTrait(trait_name) => {
                    let foreign_type =
                        syn::Ident::new(&format!("Foreign{}", trait_name), trait_name.span());

                    ffi_params.push(quote! { #name: *mut #foreign_type });

                    conversions.push(quote! {
                        let #name: Box<dyn #trait_name> = if #name.is_null() {
                            return crate::fail_with_error(
                                crate::FfiStatus::NULL_POINTER,
                                concat!(stringify!(#name), " is null")
                            );
                        } else {
                            Box::from_raw(#name)
                        };
                    });

                    call_args.push(quote! { #name });
                }
                ParamTransform::VecParam(inner_ty) => {
                    let ptr_name = syn::Ident::new(&format!("{}_ptr", name), name.span());
                    let len_name = syn::Ident::new(&format!("{}_len", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *const #inner_ty });
                    ffi_params.push(quote! { #len_name: usize });

                    conversions.push(quote! {
                        let #name: Vec<#inner_ty> = if #ptr_name.is_null() {
                            Vec::new()
                        } else {
                            core::slice::from_raw_parts(#ptr_name, #len_name).to_vec()
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

    FfiParams {
        ffi_params,
        conversions,
        call_args,
    }
}

struct AsyncFfiParams {
    ffi_params: Vec<proc_macro2::TokenStream>,
    pre_spawn: Vec<proc_macro2::TokenStream>,
    thread_setup: Vec<proc_macro2::TokenStream>,
    call_args: Vec<proc_macro2::TokenStream>,
    move_vars: Vec<syn::Ident>,
}

fn transform_params_async(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
) -> AsyncFfiParams {
    let mut ffi_params = Vec::new();
    let mut pre_spawn = Vec::new();
    let mut thread_setup = Vec::new();
    let mut call_args = Vec::new();
    let mut move_vars = Vec::new();

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
                    let owned_name = syn::Ident::new(&format!("{}_owned", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *const u8 });
                    ffi_params.push(quote! { #len_name: usize });

                    pre_spawn.push(quote! {
                        let #owned_name: String = if #ptr_name.is_null() {
                            String::new()
                        } else {
                            match core::str::from_utf8(unsafe { core::slice::from_raw_parts(#ptr_name, #len_name) }) {
                                Ok(s) => s.to_string(),
                                Err(_) => {
                                    panic!(concat!(stringify!(#name), " is not valid UTF-8"));
                                }
                            }
                        };
                    });

                    thread_setup.push(quote! {
                        let #name: &str = &#owned_name;
                    });

                    move_vars.push(owned_name);
                    call_args.push(quote! { #name });
                }
                ParamTransform::OwnedString => {
                    let ptr_name = syn::Ident::new(&format!("{}_ptr", name), name.span());
                    let len_name = syn::Ident::new(&format!("{}_len", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *const u8 });
                    ffi_params.push(quote! { #len_name: usize });

                    pre_spawn.push(quote! {
                        let #name: String = if #ptr_name.is_null() {
                            String::new()
                        } else {
                            match core::str::from_utf8(unsafe { core::slice::from_raw_parts(#ptr_name, #len_name) }) {
                                Ok(s) => s.to_string(),
                                Err(_) => {
                                    panic!(concat!(stringify!(#name), " is not valid UTF-8"));
                                }
                            }
                        };
                    });

                    move_vars.push(name.clone());
                    call_args.push(quote! { #name });
                }
                ParamTransform::Callback(_arg_types) => {
                    panic!("Callbacks are not supported in async functions");
                }
                ParamTransform::SliceRef(inner_ty) => {
                    let ptr_name = syn::Ident::new(&format!("{}_ptr", name), name.span());
                    let len_name = syn::Ident::new(&format!("{}_len", name), name.span());
                    let owned_name = syn::Ident::new(&format!("{}_vec", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *const #inner_ty });
                    ffi_params.push(quote! { #len_name: usize });

                    pre_spawn.push(quote! {
                        let #owned_name: Vec<#inner_ty> = if #ptr_name.is_null() {
                            Vec::new()
                        } else {
                            unsafe { core::slice::from_raw_parts(#ptr_name, #len_name) }.to_vec()
                        };
                    });

                    thread_setup.push(quote! {
                        let #name: &[#inner_ty] = &#owned_name;
                    });

                    move_vars.push(owned_name);
                    call_args.push(quote! { #name });
                }
                ParamTransform::SliceMut(_inner_ty) => {
                    panic!("Mutable slices are not supported in async functions");
                }
                ParamTransform::BoxedTrait(_trait_name) => {
                    panic!("Box<dyn Trait> parameters are not yet supported in async functions");
                }
                ParamTransform::VecParam(inner_ty) => {
                    let ptr_name = syn::Ident::new(&format!("{}_ptr", name), name.span());
                    let len_name = syn::Ident::new(&format!("{}_len", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *const #inner_ty });
                    ffi_params.push(quote! { #len_name: usize });

                    pre_spawn.push(quote! {
                        let #name: Vec<#inner_ty> = if #ptr_name.is_null() {
                            Vec::new()
                        } else {
                            unsafe { core::slice::from_raw_parts(#ptr_name, #len_name) }.to_vec()
                        };
                    });

                    move_vars.push(name.clone());
                    call_args.push(quote! { #name });
                }
                ParamTransform::PassThrough => {
                    let ty = &pat_type.ty;
                    ffi_params.push(quote! { #name: #ty });
                    move_vars.push(name.clone());
                    call_args.push(quote! { #name });
                }
            }
        }
    }

    AsyncFfiParams {
        ffi_params,
        pre_spawn,
        thread_setup,
        call_args,
        move_vars,
    }
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
                                let inner_str =
                                    quote::quote!(#inner_ty).to_string().replace(" ", "");
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

enum AsyncReturnKind {
    Unit,
    Primitive(proc_macro2::TokenStream),
    String,
    Struct(proc_macro2::TokenStream),
    Vec(proc_macro2::TokenStream),
    Option(proc_macro2::TokenStream),
    ResultPrimitive(proc_macro2::TokenStream),
    ResultString,
    ResultStruct(proc_macro2::TokenStream),
    ResultVec(proc_macro2::TokenStream),
}

fn extract_generic_inner(ty: &Type, wrapper: &str) -> Option<syn::Type> {
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

fn classify_async_return(output: &ReturnType) -> AsyncReturnKind {
    match output {
        ReturnType::Default => AsyncReturnKind::Unit,
        ReturnType::Type(_, ty) => {
            let type_str = quote::quote!(#ty).to_string().replace(" ", "");

            if type_str == "String" || type_str == "std::string::String" {
                return AsyncReturnKind::String;
            }

            if let Some(inner_ty) = extract_generic_inner(ty, "Vec") {
                return AsyncReturnKind::Vec(quote! { #inner_ty });
            }

            if let Some(inner_ty) = extract_generic_inner(ty, "Option") {
                return AsyncReturnKind::Option(quote! { #inner_ty });
            }

            if let Type::Path(path) = ty.as_ref() {
                if let Some(segment) = path.path.segments.last() {
                    if segment.ident == "Result" {
                        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                                let inner_str =
                                    quote::quote!(#inner_ty).to_string().replace(" ", "");

                                if inner_str == "String" || inner_str == "std::string::String" {
                                    return AsyncReturnKind::ResultString;
                                } else if inner_str == "()" {
                                    return AsyncReturnKind::Unit;
                                } else if let Some(vec_inner) =
                                    extract_generic_inner(inner_ty, "Vec")
                                {
                                    return AsyncReturnKind::ResultVec(quote! { #vec_inner });
                                } else if is_primitive_type(&inner_str) {
                                    return AsyncReturnKind::ResultPrimitive(quote! { #inner_ty });
                                } else {
                                    return AsyncReturnKind::ResultStruct(quote! { #inner_ty });
                                }
                            }
                        }
                    }
                }
            }

            if is_primitive_type(&type_str) {
                AsyncReturnKind::Primitive(quote! { #ty })
            } else {
                AsyncReturnKind::Struct(quote! { #ty })
            }
        }
    }
}

fn is_primitive_type(s: &str) -> bool {
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

fn get_ffi_return_type(return_kind: &AsyncReturnKind) -> proc_macro2::TokenStream {
    match return_kind {
        AsyncReturnKind::Unit => quote! { () },
        AsyncReturnKind::Primitive(ty) => quote! { #ty },
        AsyncReturnKind::String => quote! { crate::FfiString },
        AsyncReturnKind::Struct(ty) => quote! { #ty },
        AsyncReturnKind::Vec(inner_ty) => quote! { crate::FfiBuf<#inner_ty> },
        AsyncReturnKind::Option(inner_ty) => quote! { crate::FfiOption<#inner_ty> },
        AsyncReturnKind::ResultPrimitive(ty) => quote! { #ty },
        AsyncReturnKind::ResultString => quote! { crate::FfiString },
        AsyncReturnKind::ResultStruct(ty) => quote! { #ty },
        AsyncReturnKind::ResultVec(inner_ty) => quote! { crate::FfiBuf<#inner_ty> },
    }
}

fn get_rust_return_type(return_kind: &AsyncReturnKind) -> proc_macro2::TokenStream {
    match return_kind {
        AsyncReturnKind::Unit => quote! { () },
        AsyncReturnKind::Primitive(ty) => quote! { #ty },
        AsyncReturnKind::String => quote! { String },
        AsyncReturnKind::Struct(ty) => quote! { #ty },
        AsyncReturnKind::Vec(inner_ty) => quote! { Vec<#inner_ty> },
        AsyncReturnKind::Option(inner_ty) => quote! { Option<#inner_ty> },
        AsyncReturnKind::ResultPrimitive(ty) => {
            quote! { Result<#ty, Box<dyn std::error::Error + Send + Sync>> }
        }
        AsyncReturnKind::ResultString => {
            quote! { Result<String, Box<dyn std::error::Error + Send + Sync>> }
        }
        AsyncReturnKind::ResultStruct(ty) => {
            quote! { Result<#ty, Box<dyn std::error::Error + Send + Sync>> }
        }
        AsyncReturnKind::ResultVec(inner_ty) => {
            quote! { Result<Vec<#inner_ty>, Box<dyn std::error::Error + Send + Sync>> }
        }
    }
}

fn get_complete_conversion(return_kind: &AsyncReturnKind) -> proc_macro2::TokenStream {
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
            crate::FfiOption::from(result)
        },
        AsyncReturnKind::ResultPrimitive(_) => quote! {
            match result {
                Ok(value) => {
                    if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
                    value
                }
                Err(e) => {
                    crate::set_last_error(&e.to_string());
                    if !out_status.is_null() { *out_status = crate::FfiStatus::INTERNAL_ERROR; }
                    Default::default()
                }
            }
        },
        AsyncReturnKind::ResultString => quote! {
            match result {
                Ok(value) => {
                    if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
                    crate::FfiString::from(value)
                }
                Err(e) => {
                    crate::set_last_error(&e.to_string());
                    if !out_status.is_null() { *out_status = crate::FfiStatus::INTERNAL_ERROR; }
                    crate::FfiString::default()
                }
            }
        },
        AsyncReturnKind::ResultStruct(_) => quote! {
            match result {
                Ok(value) => {
                    if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
                    value
                }
                Err(e) => {
                    crate::set_last_error(&e.to_string());
                    if !out_status.is_null() { *out_status = crate::FfiStatus::INTERNAL_ERROR; }
                    unsafe { core::mem::zeroed() }
                }
            }
        },
        AsyncReturnKind::ResultVec(_) => quote! {
            match result {
                Ok(value) => {
                    if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
                    crate::FfiBuf::from_vec(value)
                }
                Err(e) => {
                    crate::set_last_error(&e.to_string());
                    if !out_status.is_null() { *out_status = crate::FfiStatus::INTERNAL_ERROR; }
                    crate::FfiBuf::default()
                }
            }
        },
    }
}

fn get_default_ffi_value(return_kind: &AsyncReturnKind) -> proc_macro2::TokenStream {
    match return_kind {
        AsyncReturnKind::Unit => quote! { () },
        AsyncReturnKind::Primitive(_) => quote! { Default::default() },
        AsyncReturnKind::String => quote! { crate::FfiString::default() },
        AsyncReturnKind::Struct(_) => quote! { unsafe { core::mem::zeroed() } },
        AsyncReturnKind::Vec(_) => quote! { crate::FfiBuf::default() },
        AsyncReturnKind::Option(_) => quote! { crate::FfiOption::default() },
        AsyncReturnKind::ResultPrimitive(_) => quote! { Default::default() },
        AsyncReturnKind::ResultString => quote! { crate::FfiString::default() },
        AsyncReturnKind::ResultStruct(_) => quote! { unsafe { core::mem::zeroed() } },
        AsyncReturnKind::ResultVec(_) => quote! { crate::FfiBuf::default() },
    }
}

fn generate_async_export(input: &ItemFn) -> TokenStream {
    let fn_name = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_vis = &input.vis;
    let fn_block = &input.block;

    let base_name = format!("mffi_{}", fn_name);
    let entry_ident = syn::Ident::new(&base_name, fn_name.span());
    let poll_ident = syn::Ident::new(&format!("{}_poll", base_name), fn_name.span());
    let complete_ident = syn::Ident::new(&format!("{}_complete", base_name), fn_name.span());
    let cancel_ident = syn::Ident::new(&format!("{}_cancel", base_name), fn_name.span());
    let free_ident = syn::Ident::new(&format!("{}_free", base_name), fn_name.span());

    let AsyncFfiParams {
        ffi_params,
        pre_spawn,
        thread_setup,
        call_args,
        move_vars,
    } = transform_params_async(fn_inputs);
    let return_kind = classify_async_return(fn_output);

    let ffi_return_type = get_ffi_return_type(&return_kind);
    let rust_return_type = get_rust_return_type(&return_kind);
    let complete_conversion = get_complete_conversion(&return_kind);
    let default_value = get_default_ffi_value(&return_kind);

    let future_body = quote! {
        #(#thread_setup)*
        #fn_name(#(#call_args),*).await
    };

    let entry_fn = if ffi_params.is_empty() {
        quote! {
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #entry_ident() -> crate::RustFutureHandle {
                crate::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    } else {
        quote! {
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #entry_ident(#(#ffi_params),*) -> crate::RustFutureHandle {
                #(#pre_spawn)*
                #(let _ = &#move_vars;)*
                crate::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    };

    let expanded = quote! {
        #fn_vis async fn #fn_name(#fn_inputs) #fn_output #fn_block

        #entry_fn

        #[unsafe(no_mangle)]
        #fn_vis extern "C" fn #poll_ident(
            handle: crate::RustFutureHandle,
            callback_data: u64,
            callback: crate::RustFutureContinuationCallback,
        ) {
            unsafe { crate::rustfuture::rust_future_poll::<#rust_return_type>(handle, callback, callback_data) }
        }

        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #complete_ident(
            handle: crate::RustFutureHandle,
            out_status: *mut crate::FfiStatus,
        ) -> #ffi_return_type {
            match crate::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                Some(result) => { #complete_conversion }
                None => {
                    if !out_status.is_null() { *out_status = crate::FfiStatus::CANCELLED; }
                    #default_value
                }
            }
        }

        #[unsafe(no_mangle)]
        #fn_vis extern "C" fn #cancel_ident(handle: crate::RustFutureHandle) {
            unsafe { crate::rustfuture::rust_future_cancel::<#rust_return_type>(handle) }
        }

        #[unsafe(no_mangle)]
        #fn_vis extern "C" fn #free_ident(handle: crate::RustFutureHandle) {
            unsafe { crate::rustfuture::rust_future_free::<#rust_return_type>(handle) }
        }
    };

    TokenStream::from(expanded)
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect()
}

#[proc_macro_attribute]
pub fn ffi_export(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let fn_name = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_vis = &input.vis;
    let is_async = input.sig.asyncness.is_some();

    if is_async {
        return generate_async_export(&input);
    }

    let export_name = format!("mffi_{}", fn_name);
    let export_ident = syn::Ident::new(&export_name, fn_name.span());

    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_params(fn_inputs);

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
            let copy_into_ident =
                syn::Ident::new(&format!("mffi_{}_copy_into", fn_name), fn_name.span());

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
pub fn ffi_stream(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
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
                if method.attrs.iter().any(|a| a.path().is_ident("skip")) {
                    return None;
                }
                if matches!(method.vis, syn::Visibility::Public(_)) {
                    if let Some(item_type) = extract_ffi_stream_item(&method.attrs) {
                        return Some(generate_stream_exports(
                            &type_name,
                            &snake_name,
                            method,
                            &item_type,
                        ));
                    }
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

fn transform_method_params(inputs: impl Iterator<Item = syn::FnArg>) -> FfiParams {
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

                    ffi_params.push(
                        quote! { #cb_name: extern "C" fn(*mut core::ffi::c_void, #(#arg_types),*) },
                    );
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
                ParamTransform::BoxedTrait(trait_name) => {
                    let foreign_type =
                        syn::Ident::new(&format!("Foreign{}", trait_name), trait_name.span());

                    ffi_params.push(quote! { #name: *mut #foreign_type });

                    conversions.push(quote! {
                        let #name: Box<dyn #trait_name> = if #name.is_null() {
                            return crate::fail_with_error(
                                crate::FfiStatus::NULL_POINTER,
                                concat!(stringify!(#name), " is null")
                            );
                        } else {
                            Box::from_raw(#name)
                        };
                    });

                    call_args.push(quote! { #name });
                }
                ParamTransform::VecParam(inner_ty) => {
                    let ptr_name = syn::Ident::new(&format!("{}_ptr", name), name.span());
                    let len_name = syn::Ident::new(&format!("{}_len", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *const #inner_ty });
                    ffi_params.push(quote! { #len_name: usize });

                    conversions.push(quote! {
                        let #name: Vec<#inner_ty> = if #ptr_name.is_null() {
                            Vec::new()
                        } else {
                            core::slice::from_raw_parts(#ptr_name, #len_name).to_vec()
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

    FfiParams {
        ffi_params,
        conversions,
        call_args,
    }
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
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_method_params(other_inputs);

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

fn extract_ffi_stream_item(attrs: &[syn::Attribute]) -> Option<syn::Type> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("ffi_stream") {
            return None;
        }

        let mut item_type: Option<syn::Type> = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("item") {
                let value: syn::Type = meta.value()?.parse()?;
                item_type = Some(value);
            }
            Ok(())
        });

        item_type
    })
}

fn generate_stream_exports(
    type_name: &syn::Ident,
    snake_name: &str,
    method: &syn::ImplItemFn,
    item_type: &syn::Type,
) -> proc_macro2::TokenStream {
    let method_name = &method.sig.ident;
    let base_name = format!("mffi_{}_{}", snake_name, method_name);

    let subscribe_ident = syn::Ident::new(&base_name, method_name.span());
    let pop_batch_ident = syn::Ident::new(&format!("{}_pop_batch", base_name), method_name.span());
    let wait_ident = syn::Ident::new(&format!("{}_wait", base_name), method_name.span());
    let poll_ident = syn::Ident::new(&format!("{}_poll", base_name), method_name.span());
    let unsubscribe_ident =
        syn::Ident::new(&format!("{}_unsubscribe", base_name), method_name.span());
    let free_ident = syn::Ident::new(&format!("{}_free", base_name), method_name.span());

    quote! {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #subscribe_ident(
            handle: *const #type_name,
        ) -> crate::SubscriptionHandle {
            if handle.is_null() {
                return std::ptr::null_mut();
            }
            let instance = unsafe { &*handle };
            let subscription = instance.#method_name();
            std::sync::Arc::into_raw(subscription) as crate::SubscriptionHandle
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #pop_batch_ident(
            subscription_handle: crate::SubscriptionHandle,
            output_ptr: *mut #item_type,
            output_capacity: usize,
        ) -> usize {
            if subscription_handle.is_null() || output_ptr.is_null() || output_capacity == 0 {
                return 0;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const crate::EventSubscription<#item_type>)
            };
            let output_slice = unsafe {
                std::slice::from_raw_parts_mut(
                    output_ptr as *mut std::mem::MaybeUninit<#item_type>,
                    output_capacity,
                )
            };
            subscription.pop_batch_into(output_slice)
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #wait_ident(
            subscription_handle: crate::SubscriptionHandle,
            timeout_milliseconds: u32,
        ) -> i32 {
            if subscription_handle.is_null() {
                return crate::WaitResult::Unsubscribed as i32;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const crate::EventSubscription<#item_type>)
            };
            subscription.wait_for_events(timeout_milliseconds) as i32
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #poll_ident(
            subscription_handle: crate::SubscriptionHandle,
            callback_data: u64,
            callback: crate::StreamContinuationCallback,
        ) {
            if subscription_handle.is_null() {
                callback(callback_data, crate::StreamPollResult::Closed);
                return;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const crate::EventSubscription<#item_type>)
            };
            subscription.poll(callback_data, callback);
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #unsubscribe_ident(
            subscription_handle: crate::SubscriptionHandle,
        ) {
            if subscription_handle.is_null() {
                return;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const crate::EventSubscription<#item_type>)
            };
            subscription.unsubscribe();
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #free_ident(
            subscription_handle: crate::SubscriptionHandle,
        ) {
            if subscription_handle.is_null() {
                return;
            }
            drop(unsafe {
                std::sync::Arc::from_raw(
                    subscription_handle as *const crate::EventSubscription<#item_type>
                )
            });
        }
    }
}

#[proc_macro_attribute]
pub fn ffi_trait(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_trait = parse_macro_input!(item as syn::ItemTrait);
    expand_ffi_trait(item_trait)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

fn expand_ffi_trait(item_trait: syn::ItemTrait) -> Result<proc_macro2::TokenStream, syn::Error> {
    let trait_name = &item_trait.ident;
    let trait_name_snake = to_snake_case_ident(&trait_name.to_string());
    let vtable_name = syn::Ident::new(&format!("{}VTable", trait_name), trait_name.span());
    let foreign_name = syn::Ident::new(&format!("Foreign{}", trait_name), trait_name.span());
    let vtable_static = syn::Ident::new(
        &format!("{}_VTABLE", trait_name_snake.to_string().to_uppercase()),
        trait_name.span(),
    );
    let register_fn = syn::Ident::new(
        &format!("mffi_register_{}_vtable", trait_name_snake),
        trait_name.span(),
    );
    let create_fn = syn::Ident::new(
        &format!("mffi_create_{}", trait_name_snake),
        trait_name.span(),
    );

    let mut vtable_fields = vec![
        quote! { pub free: extern "C" fn(handle: u64) },
        quote! { pub clone: extern "C" fn(handle: u64) -> u64 },
    ];

    let mut foreign_impls = Vec::new();

    for item in &item_trait.items {
        if let syn::TraitItem::Fn(method) = item {
            let method_name = &method.sig.ident;
            let method_name_snake = to_snake_case_ident(&method_name.to_string());
            let is_async = method.sig.asyncness.is_some();

            let mut param_types = Vec::new();
            let mut param_names = Vec::new();
            let mut call_args = Vec::new();

            for input in &method.sig.inputs {
                if let FnArg::Typed(pat_type) = input {
                    if let Pat::Ident(pat_ident) = &*pat_type.pat {
                        let param_name = &pat_ident.ident;
                        let param_type = &pat_type.ty;

                        let ffi_type = rust_type_to_ffi_param_type(param_type);
                        param_types.push(quote! { #param_name: #ffi_type });
                        param_names.push(quote! { #param_name: #param_type });
                        call_args.push(quote! { #param_name });
                    }
                }
            }

            let return_type = match &method.sig.output {
                ReturnType::Default => None,
                ReturnType::Type(_, ty) => Some(ty.clone()),
            };

            let has_return = return_type.is_some();

            if is_async {
                let callback_type = if let Some(ref ret_ty) = return_type {
                    let ffi_ret = rust_type_to_ffi_param_type(ret_ty);
                    quote! { extern "C" fn(callback_data: u64, result: #ffi_ret, status: crate::FfiStatus) }
                } else {
                    quote! { extern "C" fn(callback_data: u64, status: crate::FfiStatus) }
                };

                vtable_fields.push(quote! {
                    pub #method_name_snake: extern "C" fn(
                        handle: u64,
                        #(#param_types,)*
                        callback: #callback_type,
                        callback_data: u64
                    )
                });

                let impl_body = if let Some(ref ret_ty) = return_type {
                    quote! {
                        use std::sync::Arc;
                        use std::sync::atomic::{AtomicBool, Ordering};

                        struct AsyncContext<T> {
                            result: std::cell::UnsafeCell<Option<T>>,
                            completed: AtomicBool,
                            waker: std::cell::UnsafeCell<Option<std::task::Waker>>,
                        }
                        unsafe impl<T> Send for AsyncContext<T> {}
                        unsafe impl<T> Sync for AsyncContext<T> {}

                        let ctx = Arc::new(AsyncContext::<#ret_ty> {
                            result: std::cell::UnsafeCell::new(None),
                            completed: AtomicBool::new(false),
                            waker: std::cell::UnsafeCell::new(None),
                        });

                        extern "C" fn callback<T: Copy>(data: u64, result: T, _status: crate::FfiStatus) {
                            let ctx = unsafe { Arc::from_raw(data as *const AsyncContext<T>) };
                            unsafe { *ctx.result.get() = Some(result) };
                            ctx.completed.store(true, Ordering::Release);
                            if let Some(waker) = unsafe { (*ctx.waker.get()).take() } {
                                waker.wake();
                            }
                        }

                        let ctx_ptr = Arc::into_raw(ctx.clone()) as u64;
                        unsafe {
                            ((*self.vtable).#method_name_snake)(
                                self.handle,
                                #(#call_args,)*
                                callback::<#ret_ty>,
                                ctx_ptr
                            );
                        }

                        std::future::poll_fn(move |cx| {
                            if ctx.completed.load(Ordering::Acquire) {
                                let result = unsafe { (*ctx.result.get()).take().unwrap() };
                                std::task::Poll::Ready(result)
                            } else {
                                unsafe { *ctx.waker.get() = Some(cx.waker().clone()) };
                                std::task::Poll::Pending
                            }
                        }).await
                    }
                } else {
                    quote! {
                        use std::sync::Arc;
                        use std::sync::atomic::{AtomicBool, Ordering};

                        struct AsyncContext {
                            completed: AtomicBool,
                            waker: std::cell::UnsafeCell<Option<std::task::Waker>>,
                        }
                        unsafe impl Send for AsyncContext {}
                        unsafe impl Sync for AsyncContext {}

                        let ctx = Arc::new(AsyncContext {
                            completed: AtomicBool::new(false),
                            waker: std::cell::UnsafeCell::new(None),
                        });

                        extern "C" fn callback(data: u64, _status: crate::FfiStatus) {
                            let ctx = unsafe { Arc::from_raw(data as *const AsyncContext) };
                            ctx.completed.store(true, Ordering::Release);
                            if let Some(waker) = unsafe { (*ctx.waker.get()).take() } {
                                waker.wake();
                            }
                        }

                        let ctx_ptr = Arc::into_raw(ctx.clone()) as u64;
                        unsafe {
                            ((*self.vtable).#method_name_snake)(
                                self.handle,
                                #(#call_args,)*
                                callback,
                                ctx_ptr
                            );
                        }

                        std::future::poll_fn(move |cx| {
                            if ctx.completed.load(Ordering::Acquire) {
                                std::task::Poll::Ready(())
                            } else {
                                unsafe { *ctx.waker.get() = Some(cx.waker().clone()) };
                                std::task::Poll::Pending
                            }
                        }).await
                    }
                };

                let output_type = return_type
                    .as_ref()
                    .map(|t| quote! { -> #t })
                    .unwrap_or_default();
                foreign_impls.push(quote! {
                    async fn #method_name(&self, #(#param_names,)*) #output_type {
                        #impl_body
                    }
                });
            } else {
                let out_param = if let Some(ref ret_ty) = return_type {
                    let ffi_ret = rust_type_to_ffi_param_type(ret_ty);
                    quote! { out: *mut #ffi_ret, }
                } else {
                    quote! {}
                };

                vtable_fields.push(quote! {
                    pub #method_name_snake: extern "C" fn(
                        handle: u64,
                        #(#param_types,)*
                        #out_param
                        status: *mut crate::FfiStatus
                    )
                });

                let impl_body = if let Some(ref ret_ty) = return_type {
                    quote! {
                        let mut out: #ret_ty = Default::default();
                        let mut status = crate::FfiStatus::default();
                        unsafe {
                            ((*self.vtable).#method_name_snake)(
                                self.handle,
                                #(#call_args,)*
                                &mut out as *mut _,
                                &mut status
                            );
                        }
                        out
                    }
                } else {
                    quote! {
                        let mut status = crate::FfiStatus::default();
                        unsafe {
                            ((*self.vtable).#method_name_snake)(
                                self.handle,
                                #(#call_args,)*
                                &mut status
                            );
                        }
                    }
                };

                let output_type = return_type
                    .as_ref()
                    .map(|t| quote! { -> #t })
                    .unwrap_or_default();
                foreign_impls.push(quote! {
                    fn #method_name(&self, #(#param_names,)*) #output_type {
                        #impl_body
                    }
                });
            }
        }
    }

    let expanded = quote! {
        #item_trait

        #[repr(C)]
        pub struct #vtable_name {
            #(#vtable_fields),*
        }

        pub struct #foreign_name {
            vtable: *const #vtable_name,
            handle: u64,
        }

        unsafe impl Send for #foreign_name {}
        unsafe impl Sync for #foreign_name {}

        impl Drop for #foreign_name {
            fn drop(&mut self) {
                unsafe { ((*self.vtable).free)(self.handle) };
            }
        }

        impl Clone for #foreign_name {
            fn clone(&self) -> Self {
                let new_handle = unsafe { ((*self.vtable).clone)(self.handle) };
                Self {
                    vtable: self.vtable,
                    handle: new_handle,
                }
            }
        }

        impl #trait_name for #foreign_name {
            #(#foreign_impls)*
        }

        static #vtable_static: std::sync::atomic::AtomicPtr<#vtable_name> =
            std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

        #[unsafe(no_mangle)]
        pub extern "C" fn #register_fn(vtable: *const #vtable_name) {
            #vtable_static.store(vtable as *mut _, std::sync::atomic::Ordering::Release);
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn #create_fn(handle: u64) -> *mut #foreign_name {
            let vtable = #vtable_static.load(std::sync::atomic::Ordering::Acquire);
            if vtable.is_null() {
                return std::ptr::null_mut();
            }
            Box::into_raw(Box::new(#foreign_name { vtable, handle }))
        }
    };

    Ok(expanded)
}

fn to_snake_case_ident(name: &str) -> syn::Ident {
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
    syn::Ident::new(&result, proc_macro2::Span::call_site())
}

fn rust_type_to_ffi_param_type(ty: &syn::Type) -> proc_macro2::TokenStream {
    let type_str = quote!(#ty).to_string().replace(" ", "");

    match type_str.as_str() {
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "bool"
        | "usize" | "isize" => quote!(#ty),
        "&str" => quote!(*const std::os::raw::c_char),
        "String" => quote!(*const std::os::raw::c_char),
        _ => quote!(#ty),
    }
}

#[proc_macro_attribute]
pub fn data(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_clone = item.clone();

    if let Ok(mut item_struct) = syn::parse::<syn::ItemStruct>(item_clone.clone()) {
        let has_repr = item_struct.attrs.iter().any(|a| a.path().is_ident("repr"));
        if !has_repr {
            item_struct.attrs.insert(0, syn::parse_quote!(#[repr(C)]));
        }
        return TokenStream::from(quote!(#item_struct));
    }

    if let Ok(mut item_enum) = syn::parse::<syn::ItemEnum>(item_clone) {
        let has_repr = item_enum.attrs.iter().any(|a| a.path().is_ident("repr"));
        if !has_repr {
            let has_data = item_enum.variants.iter().any(|v| !v.fields.is_empty());
            if has_data {
                item_enum
                    .attrs
                    .insert(0, syn::parse_quote!(#[repr(C, i32)]));
            } else {
                item_enum.attrs.insert(0, syn::parse_quote!(#[repr(i32)]));
            }
        }
        return TokenStream::from(quote!(#item_enum));
    }

    syn::Error::new_spanned(
        proc_macro2::TokenStream::from(item),
        "data can only be applied to struct or enum",
    )
    .to_compile_error()
    .into()
}

#[proc_macro_derive(Data)]
pub fn derive_data(input: TokenStream) -> TokenStream {
    let _ = input;
    TokenStream::new()
}

#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_clone = item.clone();

    if let Ok(item_fn) = syn::parse::<ItemFn>(item_clone.clone()) {
        return ffi_export(attr, TokenStream::from(quote!(#item_fn)));
    }

    if let Ok(item_impl) = syn::parse::<syn::ItemImpl>(item_clone.clone()) {
        return ffi_class(attr, TokenStream::from(quote!(#item_impl)));
    }

    if let Ok(item_trait) = syn::parse::<syn::ItemTrait>(item_clone) {
        return ffi_trait(attr, TokenStream::from(quote!(#item_trait)));
    }

    syn::Error::new_spanned(
        proc_macro2::TokenStream::from(item),
        "export can only be applied to fn, impl, or trait",
    )
    .to_compile_error()
    .into()
}

#[proc_macro_attribute]
pub fn skip(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn name(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
