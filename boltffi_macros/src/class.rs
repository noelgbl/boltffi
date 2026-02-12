use boltffi_ffi_rules::naming;
use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::{FnArg, ReturnType, Type};

use crate::callback_registry;
use crate::custom_types;
use crate::params::{FfiParams, transform_method_params, transform_method_params_async};
use crate::returns::{
    OptionReturnAbi, ReturnKind, classify_async_return_abi, classify_return,
    get_async_complete_conversion, get_async_default_ffi_value, get_async_ffi_return_type,
    get_async_rust_return_type,
};

pub fn ffi_class_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let thread_unsafe = attr.to_string().contains("thread_unsafe");
    let input = syn::parse_macro_input!(item as syn::ItemImpl);

    let custom_types = match custom_types::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error().into(),
    };
    let callback_registry = match callback_registry::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error().into(),
    };

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

    let type_name_str = type_name.to_string();
    let free_ident = syn::Ident::new(
        naming::class_ffi_free(&type_name_str).as_str(),
        type_name.span(),
    );

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
                            &type_name_str,
                            method,
                            &item_type,
                        ));
                    }
                    if method.sig.asyncness.is_some() {
                        return generate_async_method_export(
                            &type_name,
                            &type_name_str,
                            method,
                            &custom_types,
                            &callback_registry,
                        );
                    }
                    return generate_method_export(
                        &type_name,
                        &type_name_str,
                        method,
                        &custom_types,
                        &callback_registry,
                    );
                }
            }
            None
        })
        .collect();

    let thread_safety_assertion = if thread_unsafe {
        quote! {}
    } else {
        let span = type_name.span();
        quote_spanned! {span=>
            #[allow(dead_code)]
            const _: () = {
                #[diagnostic::on_unimplemented(
                    message = "BoltFFI: `{Self}` must be thread-safe (Send + Sync) to be exported via FFI",
                    note = "exported classes can be called from any thread in the foreign language",
                    note = "use #[boltffi::export(thread_unsafe)] to opt out if you guarantee single-threaded access"
                )]
                trait BoltFFIThreadSafe: Send + Sync {}
                impl<T: Send + Sync> BoltFFIThreadSafe for T {}
                fn _assert<T: BoltFFIThreadSafe>() {}
                fn _check() { _assert::<#type_name>(); }
            };
        }
    };

    let expanded = quote! {
        #input

        #thread_safety_assertion

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

fn should_wire_encode(kind: &ReturnKind) -> bool {
    matches!(
        kind,
        ReturnKind::String
            | ReturnKind::Vec(_)
            | ReturnKind::Option(_)
            | ReturnKind::ResultString { .. }
            | ReturnKind::ResultPrimitive { .. }
            | ReturnKind::ResultUnit { .. }
    )
}

fn convert_to_wire_encoded(kind: ReturnKind) -> ReturnKind {
    match kind {
        ReturnKind::String => {
            let ty: syn::Type = syn::parse_quote!(String);
            ReturnKind::WireEncoded(ty)
        }
        ReturnKind::Vec(inner) => {
            let ty: syn::Type = syn::parse_quote!(Vec<#inner>);
            ReturnKind::WireEncoded(ty)
        }
        ReturnKind::Option(abi) => {
            let inner_ty = match &abi {
                OptionReturnAbi::OutValue { inner } => inner.clone(),
                OptionReturnAbi::OutFfiString => syn::parse_quote!(String),
                OptionReturnAbi::Vec { inner } => syn::parse_quote!(Vec<#inner>),
            };
            let ty: syn::Type = syn::parse_quote!(Option<#inner_ty>);
            ReturnKind::WireEncoded(ty)
        }
        ReturnKind::ResultString { err } => {
            let ty: syn::Type = syn::parse_quote!(Result<String, #err>);
            ReturnKind::WireEncoded(ty)
        }
        ReturnKind::ResultPrimitive { ok, err } => {
            let ty: syn::Type = syn::parse_quote!(Result<#ok, #err>);
            ReturnKind::WireEncoded(ty)
        }
        ReturnKind::ResultUnit { err } => {
            let ty: syn::Type = syn::parse_quote!(Result<(), #err>);
            ReturnKind::WireEncoded(ty)
        }
        other => other,
    }
}

fn is_factory_constructor(method: &syn::ImplItemFn, type_name: &syn::Ident) -> bool {
    let has_self = method
        .sig
        .inputs
        .first()
        .map(|arg| matches!(arg, FnArg::Receiver(_)))
        .unwrap_or(false);

    if has_self {
        return false;
    }

    is_factory_return(&method.sig.output, type_name)
}

fn is_factory_return(output: &ReturnType, type_name: &syn::Ident) -> bool {
    match output {
        ReturnType::Default => false,
        ReturnType::Type(_, ty) => match ty.as_ref() {
            Type::Path(type_path) => {
                is_self_type_path(&type_path.path, type_name)
                    || is_result_of_self_type_path(&type_path.path, type_name)
            }
            _ => false,
        },
    }
}

fn is_self_type_path(path: &syn::Path, type_name: &syn::Ident) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == "Self" || segment.ident == *type_name)
}

fn is_result_of_self_type_path(path: &syn::Path, type_name: &syn::Ident) -> bool {
    let Some(result_segment) = path.segments.last() else {
        return false;
    };
    if result_segment.ident != "Result" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(args) = &result_segment.arguments else {
        return false;
    };
    let Some(syn::GenericArgument::Type(Type::Path(ok_type_path))) = args.args.first() else {
        return false;
    };
    is_self_type_path(&ok_type_path.path, type_name)
}

fn generate_factory_constructor_export(
    type_name: &syn::Ident,
    class_name: &str,
    method: &syn::ImplItemFn,
    custom_types: &custom_types::CustomTypeRegistry,
    callback_registry: &callback_registry::CallbackTraitRegistry,
) -> Option<proc_macro2::TokenStream> {
    let method_name = &method.sig.ident;
    let export_name = if method_name == "new" {
        naming::class_ffi_new(class_name)
    } else {
        naming::method_ffi_name(class_name, &method_name.to_string())
    };
    let export_name = syn::Ident::new(export_name.as_str(), method_name.span());

    let inputs = method.sig.inputs.iter().cloned();
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_method_params(inputs, custom_types, callback_registry);

    let call_expr = quote! { #type_name::#method_name(#(#call_args),*) };

    let is_fallible = matches!(
        &method.sig.output,
        ReturnType::Type(_, ty)
            if matches!(ty.as_ref(), Type::Path(type_path) if is_result_of_self_type_path(&type_path.path, type_name))
    );

    let body = if is_fallible {
        let call = quote! { #call_expr };
        if conversions.is_empty() {
            quote! {
                match #call {
                    Ok(value) => Box::into_raw(Box::new(value)),
                    Err(error) => {
                        ::boltffi::__private::set_last_error(format!("{error:?}"));
                        ::core::ptr::null_mut()
                    }
                }
            }
        } else {
            quote! {
                #(#conversions)*
                match #call {
                    Ok(value) => Box::into_raw(Box::new(value)),
                    Err(error) => {
                        ::boltffi::__private::set_last_error(format!("{error:?}"));
                        ::core::ptr::null_mut()
                    }
                }
            }
        }
    } else if conversions.is_empty() {
        quote! { Box::into_raw(Box::new(#call_expr)) }
    } else {
        quote! {
            #(#conversions)*
            Box::into_raw(Box::new(#call_expr))
        }
    };

    if ffi_params.is_empty() {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub extern "C" fn #export_name() -> *mut #type_name {
                #body
            }
        })
    } else {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                #(#ffi_params),*
            ) -> *mut #type_name {
                #body
            }
        })
    }
}

fn generate_method_export(
    type_name: &syn::Ident,
    class_name: &str,
    method: &syn::ImplItemFn,
    custom_types: &custom_types::CustomTypeRegistry,
    callback_registry: &callback_registry::CallbackTraitRegistry,
) -> Option<proc_macro2::TokenStream> {
    let method_name = &method.sig.ident;
    let export_name = syn::Ident::new(
        naming::method_ffi_name(class_name, &method_name.to_string()).as_str(),
        method_name.span(),
    );

    let has_self = method
        .sig
        .inputs
        .first()
        .map(|arg| matches!(arg, FnArg::Receiver(_)))
        .unwrap_or(false);

    if !has_self {
        if is_factory_constructor(method, type_name) {
            return generate_factory_constructor_export(
                type_name,
                class_name,
                method,
                custom_types,
                callback_registry,
            );
        }
        return generate_static_method_export(
            type_name,
            class_name,
            method,
            custom_types,
            callback_registry,
        );
    }

    let other_inputs = method.sig.inputs.iter().skip(1).cloned();
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_method_params(other_inputs, custom_types, callback_registry);

    let has_conversions = !conversions.is_empty();

    let call_expr = quote! { (*handle).#method_name(#(#call_args),*) };

    let return_kind = classify_return(&method.sig.output);
    let return_kind = if should_wire_encode(&return_kind) {
        convert_to_wire_encoded(return_kind)
    } else {
        return_kind
    };

    let (body, return_type, is_wire_encoded) = match return_kind {
        ReturnKind::Unit => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    #call_expr;
                    ::boltffi::__private::FfiStatus::OK
                }
            } else {
                quote! {
                    #call_expr;
                    ::boltffi::__private::FfiStatus::OK
                }
            };
            (body, quote! { -> ::boltffi::__private::FfiStatus }, false)
        }
        ReturnKind::Primitive => {
            let fn_output = &method.sig.output;
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    #call_expr
                }
            } else {
                call_expr
            };
            (body, quote! { #fn_output }, false)
        }
        ReturnKind::WireEncoded(inner_ty) => {
            let needs_custom = custom_types::contains_custom_types(&inner_ty, custom_types);
            let result_ident = syn::Ident::new("result", method_name.span());

            let body = if needs_custom {
                let wire_ty = custom_types::wire_type_for(&inner_ty, custom_types);
                let wire_value_ident = syn::Ident::new("__boltffi_wire_value", method_name.span());
                let to_wire =
                    custom_types::to_wire_expr_owned(&inner_ty, custom_types, &result_ident);
                quote! {
                    #(#conversions)*
                    let #result_ident: #inner_ty = #call_expr;
                    let #wire_value_ident: #wire_ty = { #to_wire };
                    ::boltffi::__private::FfiBuf::wire_encode(&#wire_value_ident)
                }
            } else {
                quote! {
                    #(#conversions)*
                    let #result_ident: #inner_ty = #call_expr;
                    ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
                }
            };
            (body, quote! { -> ::boltffi::__private::FfiBuf<u8> }, true)
        }
        ReturnKind::String
        | ReturnKind::ResultString { .. }
        | ReturnKind::ResultPrimitive { .. }
        | ReturnKind::ResultUnit { .. }
        | ReturnKind::Vec(_)
        | ReturnKind::Option(_) => unreachable!("converted to WireEncoded"),
    };

    if is_wire_encoded {
        if ffi_params.is_empty() {
            Some(quote! {
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #export_name(
                    out: *mut ::boltffi::__private::FfiBuf<u8>,
                    handle: *mut #type_name
                ) {
                    if out.is_null() {
                        return;
                    }
                    let __boltffi_encoded: ::boltffi::__private::FfiBuf<u8> = { #body };
                    out.write(__boltffi_encoded);
                }

                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #export_name(
                    handle: *mut #type_name
                ) #return_type {
                    #body
                }
            })
        } else {
            Some(quote! {
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #export_name(
                    out: *mut ::boltffi::__private::FfiBuf<u8>,
                    handle: *mut #type_name,
                    #(#ffi_params),*
                ) {
                    if out.is_null() {
                        return;
                    }
                    let __boltffi_encoded: ::boltffi::__private::FfiBuf<u8> = { #body };
                    out.write(__boltffi_encoded);
                }

                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #export_name(
                    handle: *mut #type_name,
                    #(#ffi_params),*
                ) #return_type {
                    #body
                }
            })
        }
    } else if ffi_params.is_empty() {
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

fn generate_static_method_export(
    type_name: &syn::Ident,
    class_name: &str,
    method: &syn::ImplItemFn,
    custom_types: &custom_types::CustomTypeRegistry,
    callback_registry: &callback_registry::CallbackTraitRegistry,
) -> Option<proc_macro2::TokenStream> {
    let method_name = &method.sig.ident;
    let export_name = syn::Ident::new(
        naming::method_ffi_name(class_name, &method_name.to_string()).as_str(),
        method_name.span(),
    );

    let all_inputs = method.sig.inputs.iter().cloned();
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_method_params(all_inputs, custom_types, callback_registry);

    let has_conversions = !conversions.is_empty();
    let call_expr = quote! { #type_name::#method_name(#(#call_args),*) };

    let return_kind = classify_return(&method.sig.output);
    let return_kind = if should_wire_encode(&return_kind) {
        convert_to_wire_encoded(return_kind)
    } else {
        return_kind
    };

    let (body, return_type, is_wire_encoded) = match return_kind {
        ReturnKind::Unit => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    #call_expr;
                    ::boltffi::__private::FfiStatus::OK
                }
            } else {
                quote! {
                    #call_expr;
                    ::boltffi::__private::FfiStatus::OK
                }
            };
            (body, quote! { -> ::boltffi::__private::FfiStatus }, false)
        }
        ReturnKind::Primitive => {
            let fn_output = &method.sig.output;
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    #call_expr
                }
            } else {
                call_expr
            };
            (body, quote! { #fn_output }, false)
        }
        ReturnKind::WireEncoded(inner_ty) => {
            let needs_custom = custom_types::contains_custom_types(&inner_ty, custom_types);
            let result_ident = syn::Ident::new("result", method_name.span());

            let body = if needs_custom {
                let wire_ty = custom_types::wire_type_for(&inner_ty, custom_types);
                let wire_value_ident = syn::Ident::new("__boltffi_wire_value", method_name.span());
                if has_conversions {
                    quote! {
                        #(#conversions)*
                        let #result_ident = #call_expr;
                        let #wire_value_ident: #wire_ty = ::boltffi::__private::IntoWire::into_wire(#result_ident);
                        ::boltffi::__private::FfiBuf::wire_encode(&#wire_value_ident)
                    }
                } else {
                    quote! {
                        let #result_ident = #call_expr;
                        let #wire_value_ident: #wire_ty = ::boltffi::__private::IntoWire::into_wire(#result_ident);
                        ::boltffi::__private::FfiBuf::wire_encode(&#wire_value_ident)
                    }
                }
            } else if has_conversions {
                quote! {
                    #(#conversions)*
                    let #result_ident = #call_expr;
                    ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
                }
            } else {
                quote! {
                    let #result_ident = #call_expr;
                    ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
                }
            };
            (body, quote! { -> ::boltffi::__private::FfiBuf<u8> }, true)
        }
        _ => return None,
    };

    if is_wire_encoded {
        if ffi_params.is_empty() {
            Some(quote! {
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #export_name(
                    out: *mut ::boltffi::__private::FfiBuf<u8>
                ) {
                    if out.is_null() {
                        return;
                    }
                    let __boltffi_encoded: ::boltffi::__private::FfiBuf<u8> = { #body };
                    out.write(__boltffi_encoded);
                }

                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #export_name() #return_type {
                    #body
                }
            })
        } else {
            Some(quote! {
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #export_name(
                    out: *mut ::boltffi::__private::FfiBuf<u8>,
                    #(#ffi_params),*
                ) {
                    if out.is_null() {
                        return;
                    }
                    let __boltffi_encoded: ::boltffi::__private::FfiBuf<u8> = { #body };
                    out.write(__boltffi_encoded);
                }

                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #export_name(#(#ffi_params),*) #return_type {
                    #body
                }
            })
        }
    } else if ffi_params.is_empty() {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name() #return_type {
                #body
            }
        })
    } else {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(#(#ffi_params),*) #return_type {
                #body
            }
        })
    }
}

fn generate_async_method_export(
    type_name: &syn::Ident,
    class_name: &str,
    method: &syn::ImplItemFn,
    custom_types: &custom_types::CustomTypeRegistry,
    callback_registry: &callback_registry::CallbackTraitRegistry,
) -> Option<proc_macro2::TokenStream> {
    let method_name = &method.sig.ident;
    let method_name_str = method_name.to_string();

    let has_self = method
        .sig
        .inputs
        .first()
        .map(|arg| matches!(arg, FnArg::Receiver(_)))
        .unwrap_or(false);

    if !has_self {
        return None;
    }

    let base_name = naming::method_ffi_name(class_name, &method_name_str);
    let entry_ident = syn::Ident::new(base_name.as_str(), method_name.span());
    let poll_ident = syn::Ident::new(&format!("{}_poll", base_name), method_name.span());
    let poll_sync_ident = syn::Ident::new(&format!("{}_poll_sync", base_name), method_name.span());
    let complete_ident = syn::Ident::new(&format!("{}_complete", base_name), method_name.span());
    let panic_message_ident =
        syn::Ident::new(&format!("{}_panic_message", base_name), method_name.span());
    let cancel_ident = syn::Ident::new(&format!("{}_cancel", base_name), method_name.span());
    let free_ident = syn::Ident::new(&format!("{}_free", base_name), method_name.span());

    let other_inputs = method.sig.inputs.iter().skip(1).cloned();
    let params = transform_method_params_async(other_inputs, custom_types, callback_registry);

    let fn_output = &method.sig.output;
    let return_abi = classify_async_return_abi(fn_output);

    let ffi_return_type = get_async_ffi_return_type(&return_abi);
    let rust_return_type = get_async_rust_return_type(&return_abi);
    let complete_conversion = get_async_complete_conversion(&return_abi);
    let default_value = get_async_default_ffi_value(&return_abi);

    let ffi_params = &params.ffi_params;
    let pre_spawn = &params.pre_spawn;
    let thread_setup = &params.thread_setup;
    let call_args = &params.call_args;
    let move_vars = &params.move_vars;

    let future_body = quote! {
        #(#thread_setup)*
        instance.#method_name(#(#call_args),*).await
    };

    let entry_fn = if ffi_params.is_empty() {
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #entry_ident(
                handle: *mut #type_name
            ) -> ::boltffi::__private::RustFutureHandle {
                let instance = &*handle;
                ::boltffi::__private::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    } else {
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #entry_ident(
                handle: *mut #type_name,
                #(#ffi_params),*
            ) -> ::boltffi::__private::RustFutureHandle {
                let instance = &*handle;
                #(#pre_spawn)*
                #(let _ = &#move_vars;)*
                ::boltffi::__private::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    };

    let native_complete_fn = quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #complete_ident(
            handle: ::boltffi::__private::RustFutureHandle,
            out_status: *mut ::boltffi::__private::FfiStatus,
        ) -> #ffi_return_type {
            match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                Some(result) => { #complete_conversion }
                None => {
                    if !out_status.is_null() { *out_status = ::boltffi::__private::FfiStatus::CANCELLED; }
                    #default_value
                }
            }
        }
    };

    let wasm_complete_fn = quote! {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #complete_ident(
            out: *mut ::boltffi::__private::FfiBuf<u8>,
            handle: ::boltffi::__private::RustFutureHandle,
            _out_status: *mut ::boltffi::__private::FfiStatus,
        ) {
            if out.is_null() {
                return;
            }
            let buf = match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                Some(result) => ::boltffi::__private::FfiBuf::wire_encode(&result),
                None => ::boltffi::__private::FfiBuf::empty(),
            };
            out.write(buf);
        }
    };

    let native_poll_fn = quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        pub extern "C" fn #poll_ident(
            handle: ::boltffi::__private::RustFutureHandle,
            callback_data: u64,
            callback: ::boltffi::__private::RustFutureContinuationCallback,
        ) {
            unsafe { ::boltffi::__private::rustfuture::rust_future_poll::<#rust_return_type>(handle, callback, callback_data) }
        }
    };

    let wasm_poll_fn = quote! {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn #poll_sync_ident(
            handle: ::boltffi::__private::RustFutureHandle,
        ) -> i32 {
            unsafe { ::boltffi::__private::rust_future_poll_sync::<#rust_return_type>(handle) }
        }
    };

    let wasm_panic_message_fn = quote! {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #panic_message_ident(
            handle: ::boltffi::__private::RustFutureHandle,
        ) -> ::boltffi::__private::FfiBuf<u8> {
            match ::boltffi::__private::rust_future_panic_message::<#rust_return_type>(handle) {
                Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                None => ::boltffi::__private::FfiBuf::empty(),
            }
        }
    };

    Some(quote! {
        #entry_fn

        #native_poll_fn
        #wasm_poll_fn
        #wasm_panic_message_fn

        #native_complete_fn
        #wasm_complete_fn

        #[unsafe(no_mangle)]
        pub extern "C" fn #cancel_ident(handle: ::boltffi::__private::RustFutureHandle) {
            unsafe { ::boltffi::__private::rustfuture::rust_future_cancel::<#rust_return_type>(handle) }
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn #free_ident(handle: ::boltffi::__private::RustFutureHandle) {
            unsafe { ::boltffi::__private::rustfuture::rust_future_free::<#rust_return_type>(handle) }
        }
    })
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
    class_name: &str,
    method: &syn::ImplItemFn,
    item_type: &syn::Type,
) -> proc_macro2::TokenStream {
    let method_name = &method.sig.ident;
    let stream_name = method_name.to_string();

    let subscribe_ident = syn::Ident::new(
        naming::stream_ffi_subscribe(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let pop_batch_ident = syn::Ident::new(
        naming::stream_ffi_pop_batch(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let wait_ident = syn::Ident::new(
        naming::stream_ffi_wait(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let poll_ident = syn::Ident::new(
        naming::stream_ffi_poll(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let unsubscribe_ident = syn::Ident::new(
        naming::stream_ffi_unsubscribe(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let free_ident = syn::Ident::new(
        naming::stream_ffi_free(class_name, &stream_name).as_str(),
        method_name.span(),
    );

    quote! {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #subscribe_ident(
            handle: *const #type_name,
        ) -> ::boltffi::__private::SubscriptionHandle {
            if handle.is_null() {
                return std::ptr::null_mut();
            }
            let instance = unsafe { &*handle };
            let subscription = instance.#method_name();
            std::sync::Arc::into_raw(subscription) as ::boltffi::__private::SubscriptionHandle
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #pop_batch_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
            output_ptr: *mut #item_type,
            output_capacity: usize,
        ) -> usize {
            if subscription_handle.is_null() || output_ptr.is_null() || output_capacity == 0 {
                return 0;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
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
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
            timeout_milliseconds: u32,
        ) -> i32 {
            if subscription_handle.is_null() {
                return ::boltffi::__private::WaitResult::Unsubscribed as i32;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
            };
            subscription.wait_for_events(timeout_milliseconds) as i32
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #poll_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
            callback_data: u64,
            callback: ::boltffi::__private::StreamContinuationCallback,
        ) {
            if subscription_handle.is_null() {
                callback(callback_data, ::boltffi::__private::StreamPollResult::Closed);
                return;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
            };
            subscription.poll(callback_data, callback);
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #unsubscribe_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
        ) {
            if subscription_handle.is_null() {
                return;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
            };
            subscription.unsubscribe();
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #free_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
        ) {
            if subscription_handle.is_null() {
                return;
            }
            drop(unsafe {
                std::sync::Arc::from_raw(
                    subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>
                )
            });
        }
    }
}
