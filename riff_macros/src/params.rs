use quote::quote;
use syn::{FnArg, Pat};

use crate::util::{ParamTransform, classify_param_transform, len_ident, ptr_ident};

pub struct FfiParams {
    pub ffi_params: Vec<proc_macro2::TokenStream>,
    pub conversions: Vec<proc_macro2::TokenStream>,
    pub call_args: Vec<proc_macro2::TokenStream>,
}

pub fn transform_params(inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>) -> FfiParams {
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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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

pub struct AsyncFfiParams {
    pub ffi_params: Vec<proc_macro2::TokenStream>,
    pub pre_spawn: Vec<proc_macro2::TokenStream>,
    pub thread_setup: Vec<proc_macro2::TokenStream>,
    pub call_args: Vec<proc_macro2::TokenStream>,
    pub move_vars: Vec<syn::Ident>,
}

pub fn transform_params_async(
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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);
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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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
                ParamTransform::Callback(_) => {
                    panic!("Callbacks are not supported in async functions");
                }
                ParamTransform::SliceRef(inner_ty) => {
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);
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
                ParamTransform::SliceMut(_) => {
                    panic!("Mutable slices are not supported in async functions");
                }
                ParamTransform::BoxedTrait(_) => {
                    panic!("Box<dyn Trait> parameters are not yet supported in async functions");
                }
                ParamTransform::VecParam(inner_ty) => {
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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

pub fn transform_method_params(inputs: impl Iterator<Item = syn::FnArg>) -> FfiParams {
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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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

pub fn transform_method_params_async(inputs: impl Iterator<Item = syn::FnArg>) -> AsyncFfiParams {
    let mut ffi_params = Vec::new();
    let mut pre_spawn = Vec::new();
    let mut thread_setup = Vec::new();
    let mut call_args = Vec::new();
    let mut move_vars = Vec::new();

    for arg in inputs {
        if let FnArg::Typed(pat_type) = arg {
            let name = match pat_type.pat.as_ref() {
                Pat::Ident(ident) => ident.ident.clone(),
                _ => continue,
            };

            match classify_param_transform(&pat_type.ty) {
                ParamTransform::StrRef => {
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);
                    let owned_name = syn::Ident::new(&format!("{}_owned", name), name.span());

                    ffi_params.push(quote! { #ptr_name: *const u8 });
                    ffi_params.push(quote! { #len_name: usize });

                    pre_spawn.push(quote! {
                        let #owned_name: String = if #ptr_name.is_null() {
                            String::new()
                        } else {
                            match core::str::from_utf8(unsafe { core::slice::from_raw_parts(#ptr_name, #len_name) }) {
                                Ok(s) => s.to_string(),
                                Err(_) => panic!(concat!(stringify!(#name), " is not valid UTF-8")),
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
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

                    ffi_params.push(quote! { #ptr_name: *const u8 });
                    ffi_params.push(quote! { #len_name: usize });

                    pre_spawn.push(quote! {
                        let #name: String = if #ptr_name.is_null() {
                            String::new()
                        } else {
                            match core::str::from_utf8(unsafe { core::slice::from_raw_parts(#ptr_name, #len_name) }) {
                                Ok(s) => s.to_string(),
                                Err(_) => panic!(concat!(stringify!(#name), " is not valid UTF-8")),
                            }
                        };
                    });

                    move_vars.push(name.clone());
                    call_args.push(quote! { #name });
                }
                ParamTransform::Callback(_) => {
                    panic!("Callbacks are not supported in async methods");
                }
                ParamTransform::SliceRef(_) | ParamTransform::SliceMut(_) => {
                    panic!("Slices are not supported in async methods");
                }
                ParamTransform::BoxedTrait(_) => {
                    panic!("Box<dyn Trait> is not supported in async methods");
                }
                ParamTransform::VecParam(inner_ty) => {
                    let ptr_name = ptr_ident(&name);
                    let len_name = len_ident(&name);

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
