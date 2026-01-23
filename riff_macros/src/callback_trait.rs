use proc_macro::TokenStream;
use quote::{format_ident, quote};
use riff_ffi_rules::naming;
use syn::{FnArg, Pat, ReturnType, Type};

use crate::custom_types;

pub fn ffi_trait_impl(item: TokenStream) -> TokenStream {
    let item_trait = syn::parse_macro_input!(item as syn::ItemTrait);
    expand_ffi_trait(item_trait)
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

fn expand_ffi_trait(item_trait: syn::ItemTrait) -> Result<proc_macro2::TokenStream, syn::Error> {
    let custom_types = custom_types::registry_for_current_crate()?;
    let trait_name = &item_trait.ident;
    let trait_name_snake = to_snake_case_ident(&trait_name.to_string());
    let vtable_name = syn::Ident::new(&format!("{}VTable", trait_name), trait_name.span());
    let foreign_name = syn::Ident::new(&format!("Foreign{}", trait_name), trait_name.span());
    let vtable_static = syn::Ident::new(
        &format!("{}_VTABLE", trait_name_snake.to_string().to_uppercase()),
        trait_name.span(),
    );
    let register_fn = syn::Ident::new(
        &format!(
            "{}_register_{}_vtable",
            naming::ffi_prefix(),
            trait_name_snake
        ),
        trait_name.span(),
    );
    let create_fn = syn::Ident::new(
        &format!(
            "{}_create_{}_handle",
            naming::ffi_prefix(),
            trait_name_snake
        ),
        trait_name.span(),
    );

    let has_async_trait_attr = item_trait.attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|s| s.ident == "async_trait")
    });

    let async_trait_attr = item_trait
        .attrs
        .iter()
        .find(|attr| {
            attr.path()
                .segments
                .last()
                .is_some_and(|s| s.ident == "async_trait")
        })
        .cloned();

    let mut vtable_fields = vec![
        quote! { pub free: extern "C" fn(handle: u64) },
        quote! { pub clone: extern "C" fn(handle: u64) -> u64 },
    ];

    let has_async_methods = item_trait.items.iter().any(|item| {
        matches!(item, syn::TraitItem::Fn(method) if method.sig.asyncness.is_some())
    });

    let is_object_safe = !has_async_methods || has_async_trait_attr;

    let foreign_impls = item_trait
        .items
        .iter()
        .filter_map(|item| match item {
            syn::TraitItem::Fn(method) => Some(method),
            _ => None,
        })
        .map(|method| expand_method(method, &mut vtable_fields, &custom_types))
        .collect::<Result<Vec<_>, _>>()?;

    let expanded = quote! {
        #item_trait

        #[repr(C)]
        pub struct #vtable_name {
            #(#vtable_fields),*
        }

        #[derive(Debug)]
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

        #async_trait_attr
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
        pub extern "C" fn #create_fn(handle: u64) -> ::riff::__private::CallbackHandle {
            let vtable = #vtable_static.load(std::sync::atomic::Ordering::Acquire);
            if vtable.is_null() {
                return ::riff::__private::CallbackHandle::NULL;
            }
            ::riff::__private::CallbackHandle::new(handle, vtable as *const std::ffi::c_void)
        }
    };

    let concrete_impl = quote! {
        impl ::riff::__private::FromCallbackHandle for #foreign_name {
            unsafe fn arc_from_callback_handle(handle: ::riff::__private::CallbackHandle) -> std::sync::Arc<Self> {
                debug_assert!(!handle.is_null());
                std::sync::Arc::new(Self {
                    vtable: handle.vtable() as *const #vtable_name,
                    handle: handle.handle(),
                })
            }

            unsafe fn box_from_callback_handle(handle: ::riff::__private::CallbackHandle) -> Box<Self> {
                debug_assert!(!handle.is_null());
                Box::new(Self {
                    vtable: handle.vtable() as *const #vtable_name,
                    handle: handle.handle(),
                })
            }
        }
    };

    let dyn_impl = if is_object_safe {
        quote! {
            impl ::riff::__private::FromCallbackHandle for dyn #trait_name {
                unsafe fn arc_from_callback_handle(handle: ::riff::__private::CallbackHandle) -> std::sync::Arc<Self> {
                    debug_assert!(!handle.is_null());
                    let foreign = #foreign_name {
                        vtable: handle.vtable() as *const #vtable_name,
                        handle: handle.handle(),
                    };
                    std::sync::Arc::new(foreign)
                }

                unsafe fn box_from_callback_handle(handle: ::riff::__private::CallbackHandle) -> Box<Self> {
                    debug_assert!(!handle.is_null());
                    let foreign = #foreign_name {
                        vtable: handle.vtable() as *const #vtable_name,
                        handle: handle.handle(),
                    };
                    Box::new(foreign)
                }
            }
        }
    } else {
        quote! {}
    };

    let foreign_type_impl = if is_object_safe {
        quote! {
            impl ::riff::__private::CallbackForeignType for dyn #trait_name {
                type Foreign = #foreign_name;
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        #expanded
        #concrete_impl
        #dyn_impl
        #foreign_type_impl
    })
}

fn expand_method(
    method: &syn::TraitItemFn,
    vtable_fields: &mut Vec<proc_macro2::TokenStream>,
    custom_types: &custom_types::CustomTypeRegistry,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let method_name = &method.sig.ident;
    let method_name_snake = to_snake_case_ident(&method_name.to_string());
    let is_async = method.sig.asyncness.is_some();

    let (param_types, param_names, call_args, prelude_stmts) = method
        .sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(pat_type) => match pat_type.pat.as_ref() {
                Pat::Ident(pat_ident) => Some((pat_ident.ident.clone(), pat_type.ty.clone())),
                _ => None,
            },
            FnArg::Receiver(_) => None,
        })
        .map(|(param_name, param_type)| {
            lower_callback_param(&param_name, &param_type, custom_types)
        })
        .fold(
            (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            |(mut ffi, mut rust, mut call, mut preludes), lowering| {
                ffi.push(lowering.ffi_param);
                rust.push(lowering.rust_param);
                call.push(lowering.call_arg);
                lowering
                    .prelude
                    .into_iter()
                    .for_each(|stmt| preludes.push(stmt));
                (ffi, rust, call, preludes)
            },
        );

    let return_type = match &method.sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(ty.clone()),
    };

    if is_async {
        let callback_type = if let Some(ref ret_ty) = return_type {
            let ffi_ret = rust_type_to_ffi_param_type(ret_ty);
            quote! { extern "C" fn(callback_data: u64, result: #ffi_ret, status: ::riff::__private::FfiStatus) }
        } else {
            quote! { extern "C" fn(callback_data: u64, status: ::riff::__private::FfiStatus) }
        };

        vtable_fields.push(quote! {
            pub #method_name_snake: extern "C" fn(
                handle: u64,
                #(#param_types,)*
                callback: #callback_type,
                callback_data: u64
            )
        });

        let output_type = return_type
            .as_ref()
            .map(|t| quote! { -> #t })
            .unwrap_or_default();

        let impl_body = match return_type.as_deref() {
            Some(ret_ty) => {
                let error_expr = parse_result_type(ret_ty)
                    .map(|(_, err_ty)| {
                        quote! {
                            Err(<#err_ty as ::core::convert::From<::riff::UnexpectedFfiCallbackError>>::from(::riff::UnexpectedFfiCallbackError))
                        }
                    })
                    .unwrap_or_else(|| quote! { result });

                quote! {
                    use std::sync::{Arc, Mutex};
                    use std::task::Waker;

                    struct AsyncState<T> {
                        result: Option<T>,
                        status: ::riff::__private::FfiStatus,
                        waker: Option<Waker>,
                    }

                    struct AsyncContext<T> {
                        state: Mutex<AsyncState<T>>,
                    }

                    let ctx = Arc::new(AsyncContext::<#ret_ty> {
                        state: Mutex::new(AsyncState {
                            result: None,
                            status: ::riff::__private::FfiStatus::OK,
                            waker: None,
                        }),
                    });

                    extern "C" fn callback(data: u64, result: #ret_ty, status: ::riff::__private::FfiStatus) {
                        let ctx = unsafe { Arc::from_raw(data as *const AsyncContext<#ret_ty>) };
                        let waker = ctx
                            .state
                            .lock()
                            .ok()
                            .and_then(|mut guard| {
                                guard.result = Some(result);
                                guard.status = status;
                                guard.waker.take()
                            });
                        if let Some(waker) = waker {
                            waker.wake();
                        }
                    }

                    let ctx_ptr = Arc::into_raw(Arc::clone(&ctx)) as u64;
                    {
                        #(#prelude_stmts)*
                        unsafe {
                            ((*self.vtable).#method_name_snake)(
                                self.handle,
                                #(#call_args,)*
                                callback,
                                ctx_ptr
                            );
                        }
                    }

                    std::future::poll_fn(move |cx| {
                        let mut guard = ctx.state.lock().expect("async callback mutex poisoned");
                        if let Some(result) = guard.result.take() {
                            let status = guard.status;
                            if status.is_err() {
                                return std::task::Poll::Ready(#error_expr);
                            }
                            std::task::Poll::Ready(result)
                        } else {
                            guard.waker = Some(cx.waker().clone());
                            std::task::Poll::Pending
                        }
                    }).await
                }
            }
            None => quote! {
                use std::sync::{Arc, Mutex};
                use std::task::Waker;

                struct AsyncState {
                    completed: bool,
                    status: ::riff::__private::FfiStatus,
                    waker: Option<Waker>,
                }

                struct AsyncContext {
                    state: Mutex<AsyncState>,
                }

                let ctx = Arc::new(AsyncContext {
                    state: Mutex::new(AsyncState {
                        completed: false,
                        status: ::riff::__private::FfiStatus::OK,
                        waker: None,
                    }),
                });

                extern "C" fn callback(data: u64, status: ::riff::__private::FfiStatus) {
                    let ctx = unsafe { Arc::from_raw(data as *const AsyncContext) };
                    let waker = ctx
                        .state
                        .lock()
                        .ok()
                        .and_then(|mut guard| {
                            guard.completed = true;
                            guard.status = status;
                            guard.waker.take()
                        });
                    if let Some(waker) = waker {
                        waker.wake();
                    }
                }

                let ctx_ptr = Arc::into_raw(Arc::clone(&ctx)) as u64;
                {
                    #(#prelude_stmts)*
                    unsafe {
                        ((*self.vtable).#method_name_snake)(
                            self.handle,
                            #(#call_args,)*
                            callback,
                            ctx_ptr
                        );
                    }
                }

                std::future::poll_fn(move |cx| {
                    let mut guard = ctx.state.lock().expect("async callback mutex poisoned");
                    if guard.completed {
                        std::task::Poll::Ready(())
                    } else {
                        guard.waker = Some(cx.waker().clone());
                        std::task::Poll::Pending
                    }
                }).await
            },
        };

        Ok(quote! {
            async fn #method_name(&self, #(#param_names,)*) #output_type {
                #impl_body
            }
        })
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
                status: *mut ::riff::__private::FfiStatus
            )
        });

        let output_type = return_type
            .as_ref()
            .map(|t| quote! { -> #t })
            .unwrap_or_default();

        let impl_body = match return_type.as_deref() {
            Some(ret_ty) => {
                let error_expr = parse_result_type(ret_ty).map(|(_, err_ty)| {
                    quote! {
                        return Err(<#err_ty as ::core::convert::From<::riff::UnexpectedFfiCallbackError>>::from(::riff::UnexpectedFfiCallbackError));
                    }
                });

                quote! {
                    #(#prelude_stmts)*
                    let mut out: #ret_ty = Default::default();
                    let mut status = ::riff::__private::FfiStatus::default();
                    unsafe {
                        ((*self.vtable).#method_name_snake)(
                            self.handle,
                            #(#call_args,)*
                            &mut out as *mut _,
                            &mut status
                        );
                    }
                    if status.is_err() {
                        #error_expr
                    }
                    out
                }
            }
            None => quote! {
                #(#prelude_stmts)*
                let mut status = ::riff::__private::FfiStatus::default();
                unsafe {
                    ((*self.vtable).#method_name_snake)(
                        self.handle,
                        #(#call_args,)*
                        &mut status
                    );
                }
            },
        };

        Ok(quote! {
            fn #method_name(&self, #(#param_names,)*) #output_type {
                #impl_body
            }
        })
    }
}

fn to_snake_case_ident(name: &str) -> syn::Ident {
    syn::Ident::new(&naming::to_snake_case(name), proc_macro2::Span::call_site())
}

fn rust_type_to_ffi_param_type(ty: &syn::Type) -> proc_macro2::TokenStream {
    let type_str = quote!(#ty).to_string().replace(' ', "");
    match type_str.as_str() {
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "bool"
        | "usize" | "isize" => quote!(#ty),
        "&str" => quote!(*const std::os::raw::c_char),
        "String" => quote!(*const std::os::raw::c_char),
        _ => quote!(#ty),
    }
}

struct CallbackParamLowering {
    ffi_param: proc_macro2::TokenStream,
    rust_param: proc_macro2::TokenStream,
    call_arg: proc_macro2::TokenStream,
    prelude: Option<proc_macro2::TokenStream>,
}

fn lower_callback_param(
    param_name: &syn::Ident,
    param_type: &syn::Type,
    custom_types: &custom_types::CustomTypeRegistry,
) -> CallbackParamLowering {
    let rust_param = quote! { #param_name: #param_type };

    let type_str = quote!(#param_type).to_string().replace(' ', "");
    if is_ffi_primitive(&type_str) {
        return CallbackParamLowering {
            ffi_param: quote! { #param_name: #param_type },
            rust_param,
            call_arg: quote! { #param_name },
            prelude: None,
        };
    }

    let ptr_name = format_ident!("{}_ptr", param_name);
    let len_name = format_ident!("{}_len", param_name);
    let wire_name = format_ident!("{}_wire", param_name);

    let prelude = if custom_types::contains_custom_types(param_type, custom_types) {
        let wire_ty = custom_types::wire_type_for(param_type, custom_types);
        let wire_value_name = format_ident!("{}_wire_value", param_name);
        let to_wire = custom_types::to_wire_expr_owned(param_type, custom_types, param_name);
        quote! {
            let #wire_value_name: #wire_ty = { #to_wire };
            let #wire_name = ::riff::__private::wire::encode(&#wire_value_name);
        }
    } else {
        quote! { let #wire_name = ::riff::__private::wire::encode(&#param_name); }
    };

    CallbackParamLowering {
        ffi_param: quote! { #ptr_name: *const u8, #len_name: usize },
        rust_param,
        call_arg: quote! { #wire_name.as_ptr(), #wire_name.len() },
        prelude: Some(prelude),
    }
}

fn is_ffi_primitive(type_str: &str) -> bool {
    matches!(
        type_str,
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
    )
}

fn parse_result_type(ty: &Type) -> Option<(Type, Type)> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let result_segment = type_path.path.segments.last()?;
    if result_segment.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &result_segment.arguments else {
        return None;
    };
    let mut types = args.args.iter().filter_map(|arg| match arg {
        syn::GenericArgument::Type(ty) => Some(ty.clone()),
        _ => None,
    });
    let ok = types.next()?;
    let err = types.next()?;
    Some((ok, err))
}
