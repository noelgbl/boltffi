use proc_macro::TokenStream;
use quote::quote;
use riff_ffi_rules::naming;
use syn::{FnArg, ReturnType, Type};

use crate::params::{transform_method_params, transform_method_params_async, FfiParams};
use crate::returns::{classify_async_return, get_complete_conversion, get_default_ffi_value, get_ffi_return_type, get_rust_return_type};

pub fn ffi_class_impl(item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemImpl);

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
    let new_ident = syn::Ident::new(&naming::class_ffi_new(&type_name_str), type_name.span());
    let free_ident = syn::Ident::new(&naming::class_ffi_free(&type_name_str), type_name.span());

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
                        return generate_async_method_export(&type_name, &type_name_str, method);
                    }
                    return generate_method_export(&type_name, &type_name_str, method);
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
    class_name: &str,
    method: &syn::ImplItemFn,
) -> Option<proc_macro2::TokenStream> {
    let method_name = &method.sig.ident;
    let export_name = syn::Ident::new(
        &naming::method_ffi_name(class_name, &method_name.to_string()),
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

fn generate_async_method_export(
    type_name: &syn::Ident,
    class_name: &str,
    method: &syn::ImplItemFn,
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
    let entry_ident = syn::Ident::new(&base_name, method_name.span());
    let poll_ident = syn::Ident::new(&format!("{}_poll", base_name), method_name.span());
    let complete_ident = syn::Ident::new(&format!("{}_complete", base_name), method_name.span());
    let cancel_ident = syn::Ident::new(&format!("{}_cancel", base_name), method_name.span());
    let free_ident = syn::Ident::new(&format!("{}_free", base_name), method_name.span());

    let other_inputs = method.sig.inputs.iter().skip(1).cloned();
    let params = transform_method_params_async(other_inputs);

    let fn_output = &method.sig.output;
    let return_kind = classify_async_return(fn_output);

    let ffi_return_type = get_ffi_return_type(&return_kind);
    let rust_return_type = get_rust_return_type(&return_kind);
    let complete_conversion = get_complete_conversion(&return_kind);
    let default_value = get_default_ffi_value(&return_kind);

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
            ) -> crate::RustFutureHandle {
                let instance = &*handle;
                crate::rustfuture::rust_future_new(async move {
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
            ) -> crate::RustFutureHandle {
                let instance = &*handle;
                #(#pre_spawn)*
                #(let _ = &#move_vars;)*
                crate::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    };

    use crate::returns::{AsyncErrorKind, AsyncReturnKind};

    let complete_fn = match &return_kind {
        AsyncReturnKind::Result(info) => {
            let out_err_type = match &info.err_kind {
                AsyncErrorKind::StringLike(_) => quote! { crate::FfiError },
                AsyncErrorKind::Typed(err) => quote! { #err },
            };
            quote! {
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #complete_ident(
                    handle: crate::RustFutureHandle,
                    out_status: *mut crate::FfiStatus,
                    out_err: *mut #out_err_type,
                ) -> #ffi_return_type {
                    match crate::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                        Some(result) => { #complete_conversion }
                        None => {
                            if !out_status.is_null() { *out_status = crate::FfiStatus::CANCELLED; }
                            #default_value
                        }
                    }
                }
            }
        }
        _ => {
            quote! {
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #complete_ident(
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
            }
        }
    };

    Some(quote! {
        #entry_fn

        #[unsafe(no_mangle)]
        pub extern "C" fn #poll_ident(
            handle: crate::RustFutureHandle,
            callback_data: u64,
            callback: crate::RustFutureContinuationCallback,
        ) {
            unsafe { crate::rustfuture::rust_future_poll::<#rust_return_type>(handle, callback, callback_data) }
        }

        #complete_fn

        #[unsafe(no_mangle)]
        pub extern "C" fn #cancel_ident(handle: crate::RustFutureHandle) {
            unsafe { crate::rustfuture::rust_future_cancel::<#rust_return_type>(handle) }
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn #free_ident(handle: crate::RustFutureHandle) {
            unsafe { crate::rustfuture::rust_future_free::<#rust_return_type>(handle) }
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
        &naming::stream_ffi_subscribe(class_name, &stream_name),
        method_name.span(),
    );
    let pop_batch_ident = syn::Ident::new(
        &naming::stream_ffi_pop_batch(class_name, &stream_name),
        method_name.span(),
    );
    let wait_ident = syn::Ident::new(
        &naming::stream_ffi_wait(class_name, &stream_name),
        method_name.span(),
    );
    let poll_ident = syn::Ident::new(
        &naming::stream_ffi_poll(class_name, &stream_name),
        method_name.span(),
    );
    let unsubscribe_ident = syn::Ident::new(
        &naming::stream_ffi_unsubscribe(class_name, &stream_name),
        method_name.span(),
    );
    let free_ident = syn::Ident::new(
        &naming::stream_ffi_free(class_name, &stream_name),
        method_name.span(),
    );

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
