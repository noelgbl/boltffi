use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, FnArg, ItemFn, Pat, ReturnType, Type};

#[proc_macro_derive(FfiType)]
pub fn derive_ffi_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

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

    let expanded = quote! {};

    TokenStream::from(expanded)
}

fn extract_arg_idents(inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>) -> Vec<&Pat> {
    inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pat_type) = arg {
                Some(pat_type.pat.as_ref())
            } else {
                None
            }
        })
        .collect()
}

enum ReturnKind {
    Unit,
    Primitive,
    String,
    ResultPrimitive(syn::Type),
    ResultString,
}

fn classify_return(output: &ReturnType) -> ReturnKind {
    match output {
        ReturnType::Default => ReturnKind::Unit,
        ReturnType::Type(_, ty) => {
            let type_str = quote::quote!(#ty).to_string().replace(" ", "");

            if type_str == "String" || type_str == "std::string::String" {
                return ReturnKind::String;
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

    let arg_idents = extract_arg_idents(fn_inputs);

    let export_name = format!("mffi_{}", fn_name);
    let export_ident = syn::Ident::new(&export_name, fn_name.span());

    let expanded = match classify_return(fn_output) {
        ReturnKind::String => {
            quote! {
                #input

                #[unsafe(no_mangle)]
                #fn_vis unsafe extern "C" fn #export_ident(
                    #fn_inputs,
                    out: *mut crate::FfiString
                ) -> crate::FfiStatus {
                    if out.is_null() {
                        return crate::FfiStatus::NULL_POINTER;
                    }
                    let result = #fn_name(#(#arg_idents),*);
                    *out = crate::FfiString::from(result);
                    crate::FfiStatus::OK
                }
            }
        }
        ReturnKind::ResultString => {
            quote! {
                #input

                #[unsafe(no_mangle)]
                #fn_vis unsafe extern "C" fn #export_ident(
                    #fn_inputs,
                    out: *mut crate::FfiString
                ) -> crate::FfiStatus {
                    if out.is_null() {
                        return crate::FfiStatus::NULL_POINTER;
                    }
                    match #fn_name(#(#arg_idents),*) {
                        Ok(value) => {
                            *out = crate::FfiString::from(value);
                            crate::FfiStatus::OK
                        }
                        Err(e) => crate::fail_with_error(crate::FfiStatus::INTERNAL_ERROR, &e.to_string())
                    }
                }
            }
        }
        ReturnKind::ResultPrimitive(inner_ty) => {
            quote! {
                #input

                #[unsafe(no_mangle)]
                #fn_vis unsafe extern "C" fn #export_ident(
                    #fn_inputs,
                    out: *mut #inner_ty
                ) -> crate::FfiStatus {
                    if out.is_null() {
                        return crate::FfiStatus::NULL_POINTER;
                    }
                    match #fn_name(#(#arg_idents),*) {
                        Ok(value) => {
                            *out = value;
                            crate::FfiStatus::OK
                        }
                        Err(e) => crate::fail_with_error(crate::FfiStatus::INTERNAL_ERROR, &e.to_string())
                    }
                }
            }
        }
        ReturnKind::Unit => {
            quote! {
                #input

                #[unsafe(no_mangle)]
                #fn_vis extern "C" fn #export_ident(#fn_inputs) -> crate::FfiStatus {
                    #fn_name(#(#arg_idents),*);
                    crate::FfiStatus::OK
                }
            }
        }
        ReturnKind::Primitive => {
            quote! {
                #input

                #[unsafe(no_mangle)]
                #fn_vis extern "C" fn #export_ident(#fn_inputs) #fn_output {
                    #fn_name(#(#arg_idents),*)
                }
            }
        }
    };

    TokenStream::from(expanded)
}
