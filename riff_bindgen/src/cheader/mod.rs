use riff_ffi_rules::naming;

use crate::model::{
    CallbackTrait, Class, Enumeration, Function, Method, Module, Parameter, Primitive, Record,
    StreamMethod, TraitMethod, Type,
};

pub struct CHeaderGenerator;

impl CHeaderGenerator {
    pub fn generate(module: &Module) -> String {
        let prefix = naming::ffi_prefix();
        let mut out = String::new();

        out.push_str(&Self::generate_preamble(&prefix));
        out.push_str(&Self::generate_async_types_if_needed(module, &prefix));
        out.push_str(&Self::generate_stream_types_if_needed(module));
        out.push_str(&Self::generate_ffi_generic_types(module));
        out.push_str(&Self::generate_enums(&module.enums));
        out.push_str(&Self::generate_records(&module.records));
        out.push_str(&Self::generate_traits(&module.callback_traits, &prefix));
        out.push_str(&Self::generate_functions(&module.functions));
        out.push_str(&Self::generate_classes(&module.classes, &prefix));
        out.push_str(&Self::generate_free_string(&prefix));

        out
    }

    fn generate_preamble(prefix: &str) -> String {
        format!(
            r#"#pragma once

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdatomic.h>

typedef struct {{ int32_t code; }} FfiStatus;
typedef struct {{ uint8_t* ptr; size_t len; size_t cap; }} FfiString;
typedef struct {{ FfiString message; }} FfiError;

static inline bool {prefix}_atomic_u8_cas(uint8_t* state, uint8_t expected, uint8_t desired) {{
    return atomic_compare_exchange_strong_explicit((_Atomic uint8_t*)state, &expected, desired, memory_order_acq_rel, memory_order_acquire);
}}

static inline uint64_t {prefix}_atomic_u64_exchange(uint64_t* slot, uint64_t value) {{
    return atomic_exchange_explicit((_Atomic uint64_t*)slot, value, memory_order_acq_rel);
}}

static inline bool {prefix}_atomic_u64_cas(uint64_t* slot, uint64_t expected, uint64_t desired) {{
    return atomic_compare_exchange_strong_explicit((_Atomic uint64_t*)slot, &expected, desired, memory_order_acq_rel, memory_order_acquire);
}}

static inline uint64_t {prefix}_atomic_u64_load(uint64_t* slot) {{
    return atomic_load_explicit((_Atomic uint64_t*)slot, memory_order_acquire);
}}

"#
        )
    }

    fn generate_async_types_if_needed(module: &Module, _prefix: &str) -> String {
        if !module.has_async() {
            return String::new();
        }

        "typedef const void* RustFutureHandle;\n\
         typedef void (*RustFutureContinuationCallback)(uint64_t callback_data, int8_t poll_result);\n\n"
            .to_string()
    }

    fn generate_stream_types_if_needed(module: &Module) -> String {
        if !module.has_streams() {
            return String::new();
        }

        "typedef void* SubscriptionHandle;\n\
         typedef void (*StreamContinuationCallback)(uint64_t callback_data, int8_t poll_result);\n\n"
            .to_string()
    }

    fn generate_ffi_generic_types(module: &Module) -> String {
        use std::collections::HashSet;
        let mut buf_types: HashSet<String> = HashSet::new();
        let mut option_types: HashSet<String> = HashSet::new();

        let collect_from_type =
            |ty: &Type, buf_types: &mut HashSet<String>, option_types: &mut HashSet<String>| {
                match ty {
                    Type::Vec(inner) if inner.is_primitive() => {
                        buf_types.insert(Self::primitive_to_cbindgen_name(inner));
                    }
                    Type::Option(inner) if inner.is_primitive() => {
                        option_types.insert(Self::primitive_to_cbindgen_name(inner));
                    }
                    Type::Result { ok, .. } => {
                        if let Type::Vec(inner) = ok.as_ref() {
                            if inner.is_primitive() {
                                buf_types.insert(Self::primitive_to_cbindgen_name(inner));
                            }
                        }
                        if let Type::Option(inner) = ok.as_ref() {
                            if inner.is_primitive() {
                                option_types.insert(Self::primitive_to_cbindgen_name(inner));
                            }
                        }
                    }
                    _ => {}
                }
            };

        for func in &module.functions {
            if func.is_async {
                if let Some(ref ty) = func.output {
                    collect_from_type(ty, &mut buf_types, &mut option_types);
                }
            }
        }

        for class in &module.classes {
            for method in &class.methods {
                if method.is_async {
                    if let Some(ref ty) = method.output {
                        collect_from_type(ty, &mut buf_types, &mut option_types);
                    }
                }
            }
        }

        let mut out = String::new();

        for ty_name in &buf_types {
            let c_type = Self::cbindgen_name_to_c_type(ty_name);
            out.push_str(&format!(
                "typedef struct FfiBuf_{} {{ {}* ptr; size_t len; size_t cap; }} FfiBuf_{};\n",
                ty_name, c_type, ty_name
            ));
        }

        for ty_name in &option_types {
            let c_type = Self::cbindgen_name_to_c_type(ty_name);
            out.push_str(&format!(
                "typedef struct FfiOption_{} {{ bool isSome; {} value; }} FfiOption_{};\n",
                ty_name, c_type, ty_name
            ));
        }

        for ty_name in &buf_types {
            out.push_str(&format!(
                "void riff_free_buf_{}(FfiBuf_{} buf);\n",
                ty_name, ty_name
            ));
        }

        if !out.is_empty() {
            out.push('\n');
        }

        out
    }

    fn cbindgen_name_to_c_type(name: &str) -> &'static str {
        match name {
            "i8" => "int8_t",
            "i16" => "int16_t",
            "i32" => "int32_t",
            "i64" => "int64_t",
            "u8" => "uint8_t",
            "u16" => "uint16_t",
            "u32" => "uint32_t",
            "u64" => "uint64_t",
            "f32" => "float",
            "f64" => "double",
            "bool" => "bool",
            "usize" => "size_t",
            "isize" => "ptrdiff_t",
            _ => "void",
        }
    }

    fn generate_enums(enums: &[Enumeration]) -> String {
        enums
            .iter()
            .filter(|e| !Self::is_internal_enum(&e.name))
            .map(Self::generate_enum)
            .collect()
    }

    fn is_internal_enum(name: &str) -> bool {
        matches!(
            name,
            "StreamPollResult"
                | "ContinuationState"
                | "WaitResult"
                | "RustFuturePoll"
                | "SchedulerStateTag"
        )
    }

    fn generate_enum(e: &Enumeration) -> String {
        let c_type = "int32_t";

        if e.is_c_style() {
            let mut out = format!("typedef {} {};\n", c_type, e.name);
            let mut next_value: i64 = 0;

            for variant in &e.variants {
                let value = variant.discriminant.unwrap_or(next_value);
                out.push_str(&format!("#define {}_{} {}\n", e.name, variant.name, value));
                next_value = value + 1;
            }
            out.push('\n');
            out
        } else {
            let mut out = format!(
                "typedef struct {} {{\n  {} tag;\n  union {{\n",
                e.name, c_type
            );

            for variant in &e.variants {
                if variant.is_unit() {
                    continue;
                }

                if variant.fields.len() == 1 {
                    let field = &variant.fields[0];
                    out.push_str(&format!(
                        "    {} {};\n",
                        Self::type_to_c(&field.field_type),
                        variant.name
                    ));
                } else {
                    out.push_str("    struct { ");
                    for field in &variant.fields {
                        out.push_str(&format!(
                            "{} {}; ",
                            Self::type_to_c(&field.field_type),
                            field.name
                        ));
                    }
                    out.push_str(&format!("}} {};\n", variant.name));
                }
            }

            out.push_str("  } payload;\n");
            out.push_str(&format!("}} {};\n", e.name));

            let mut next_value: i64 = 0;
            for variant in &e.variants {
                let value = variant.discriminant.unwrap_or(next_value);
                out.push_str(&format!(
                    "#define {}_TAG_{} {}\n",
                    e.name, variant.name, value
                ));
                next_value = value + 1;
            }
            out.push('\n');
            out
        }
    }

    fn generate_records(records: &[Record]) -> String {
        records
            .iter()
            .filter(|r| !Self::is_internal_type(&r.name))
            .map(Self::generate_record)
            .collect()
    }

    fn is_internal_type(name: &str) -> bool {
        name.starts_with("Ffi")
            || name.starts_with("Pending")
            || name == "MaybeUninit"
            || name == "PhantomData"
    }

    fn generate_record(r: &Record) -> String {
        let fields: String = r
            .fields
            .iter()
            .map(|f| {
                format!(
                    "  {} {};\n",
                    Self::type_to_c(&f.field_type),
                    naming::snake_to_camel(&f.name)
                )
            })
            .collect();
        format!("typedef struct {} {{\n{}}} {};\n\n", r.name, fields, r.name)
    }

    fn generate_traits(traits: &[CallbackTrait], prefix: &str) -> String {
        traits
            .iter()
            .map(|t| Self::generate_trait(t, prefix))
            .collect()
    }

    fn generate_trait(t: &CallbackTrait, prefix: &str) -> String {
        let trait_name = &t.name;
        let vtable_name = format!("{}VTable", trait_name);
        let foreign_name = format!("Foreign{}", trait_name);
        let snake_name = naming::to_snake_case(trait_name);

        let mut vtable_fields = vec![
            "  void (*free)(uint64_t handle);".to_string(),
            "  uint64_t (*clone)(uint64_t handle);".to_string(),
        ];

        for method in &t.methods {
            vtable_fields.push(Self::generate_trait_method_field(method));
        }

        format!(
            "typedef struct {} {{\n{}\n}} {};\n\n\
             typedef struct {} {{\n  const struct {} *vtable;\n  uint64_t handle;\n}} {};\n\n\
             void {}_register_{}_vtable(const struct {} *vtable);\n\
             struct {} *{}_create_{}(uint64_t handle);\n\n",
            vtable_name,
            vtable_fields.join("\n"),
            vtable_name,
            foreign_name,
            vtable_name,
            foreign_name,
            prefix,
            snake_name,
            vtable_name,
            foreign_name,
            prefix,
            snake_name,
        )
    }

    fn generate_trait_method_field(method: &TraitMethod) -> String {
        let method_snake = naming::to_snake_case(&method.name);
        let mut params = vec!["uint64_t handle".to_string()];

        for param in &method.inputs {
            params.push(format!(
                "{} {}",
                Self::type_to_c(&param.param_type),
                param.name
            ));
        }

        if method.is_async {
            let callback_return = method
                .output
                .as_ref()
                .map(|t| format!(", {}", Self::type_to_c(t)))
                .unwrap_or_default();
            params.push(format!(
                "void (*callback)(uint64_t{}, FfiStatus)",
                callback_return
            ));
            params.push("uint64_t callback_data".to_string());
        } else {
            if let Some(ref ret_ty) = method.output {
                params.push(format!("{} *out", Self::type_to_c(ret_ty)));
            }
            params.push("FfiStatus *status".to_string());
        }

        format!("  void (*{})({});", method_snake, params.join(", "))
    }

    fn generate_functions(functions: &[Function]) -> String {
        functions
            .iter()
            .map(|f| Self::generate_function(f))
            .collect()
    }

    fn generate_function(func: &Function) -> String {
        let ffi_name = naming::function_ffi_name(&func.name);
        let params = Self::build_params(&func.inputs);

        if func.is_async {
            Self::generate_async_function(&ffi_name, &params, &func.output)
        } else {
            Self::generate_sync_function(&ffi_name, &params, &func.output)
        }
    }

    fn generate_sync_function(
        ffi_name: &str,
        params: &[(String, String)],
        output: &Option<Type>,
    ) -> String {
        match output {
            Some(Type::Vec(inner)) => {
                let inner_c = Self::type_to_c(inner);
                Self::generate_vec_return_function(ffi_name, params, &inner_c)
            }
            Some(Type::String) => Self::generate_string_return_function(ffi_name, params),
            Some(Type::Result { ok, err }) => {
                Self::generate_result_return_function_with_err(ffi_name, params, ok, Some(err))
            }
            Some(Type::Option(inner)) if inner.is_primitive() => {
                Self::generate_option_return_function(ffi_name, params, inner)
            }
            _ => {
                let ret_type = Self::return_type_to_c(output);
                let params_str = Self::format_params(params);
                format!("{} {}({});\n", ret_type, ffi_name, params_str)
            }
        }
    }

    fn generate_async_function(
        ffi_name: &str,
        params: &[(String, String)],
        output: &Option<Type>,
    ) -> String {
        let params_str = Self::format_params(params);
        let result_type = Self::async_result_type(output);
        let err_out_param = Self::async_error_out_param(output);

        format!(
            "RustFutureHandle {}({});\n\
             void {}_poll(RustFutureHandle handle, uint64_t callback_data, RustFutureContinuationCallback callback);\n\
             {} {}_complete(RustFutureHandle handle, FfiStatus* out_status{});\n\
             void {}_cancel(RustFutureHandle handle);\n\
             void {}_free(RustFutureHandle handle);\n",
            ffi_name, params_str, ffi_name, result_type, ffi_name, err_out_param, ffi_name, ffi_name
        )
    }

    fn async_error_out_param(output: &Option<Type>) -> String {
        let Some(Type::Result { err, .. }) = output else {
            return String::new();
        };
        match err.as_ref() {
            Type::Enum(name) | Type::Record(name) => format!(", {}* out_err", name),
            Type::String => ", FfiError* out_err".to_string(),
            _ => String::new(),
        }
    }

    fn generate_vec_return_function(
        ffi_name: &str,
        params: &[(String, String)],
        inner_c: &str,
    ) -> String {
        let params_str = Self::format_params(params);

        let mut copy_params: Vec<(String, String)> = params.to_vec();
        copy_params.push(("dst".to_string(), format!("{} *", inner_c)));
        copy_params.push(("dst_cap".to_string(), "uintptr_t".to_string()));
        copy_params.push(("written".to_string(), "uintptr_t *".to_string()));
        let copy_params_str = Self::format_params(&copy_params);

        format!(
            "uintptr_t {}_len({});\nFfiStatus {}_copy_into({});\n",
            ffi_name, params_str, ffi_name, copy_params_str
        )
    }

    fn generate_string_return_function(ffi_name: &str, params: &[(String, String)]) -> String {
        let mut new_params = params.to_vec();
        new_params.push(("out".to_string(), "FfiString *".to_string()));
        let params_str = Self::format_params(&new_params);
        format!("FfiStatus {}({});\n", ffi_name, params_str)
    }

    fn generate_result_return_function_with_err(
        ffi_name: &str,
        params: &[(String, String)],
        ok_type: &Type,
        err_type: Option<&Type>,
    ) -> String {
        let err_out_type = err_type.and_then(Self::error_out_type);

        match ok_type {
            Type::Void => {
                let mut new_params = params.to_vec();
                if let Some(err_type_str) = &err_out_type {
                    new_params.push(("out_err".to_string(), format!("{} *", err_type_str)));
                }
                let params_str = Self::format_params(&new_params);
                format!("FfiStatus {}({});\n", ffi_name, params_str)
            }
            Type::String => {
                let mut new_params = params.to_vec();
                let out_name = if err_out_type.is_some() { "out_ok" } else { "out" };
                new_params.push((out_name.to_string(), "FfiString *".to_string()));
                if let Some(err_type_str) = &err_out_type {
                    new_params.push(("out_err".to_string(), format!("{} *", err_type_str)));
                }
                let params_str = Self::format_params(&new_params);
                format!("FfiStatus {}({});\n", ffi_name, params_str)
            }
            _ => {
                let mut new_params = params.to_vec();
                let out_name = if err_out_type.is_some() { "out_ok" } else { "out" };
                new_params.push((out_name.to_string(), format!("{} *", Self::type_to_c(ok_type))));
                if let Some(err_type_str) = &err_out_type {
                    new_params.push(("out_err".to_string(), format!("{} *", err_type_str)));
                }
                let params_str = Self::format_params(&new_params);
                format!("FfiStatus {}({});\n", ffi_name, params_str)
            }
        }
    }

    fn error_out_type(err: &Type) -> Option<String> {
        match err {
            Type::Enum(name) | Type::Record(name) => Some(name.clone()),
            Type::String => Some("FfiError".to_string()),
            _ => None,
        }
    }

    fn generate_option_return_function(
        ffi_name: &str,
        params: &[(String, String)],
        inner: &Type,
    ) -> String {
        let mut new_params = params.to_vec();
        new_params.push(("out".to_string(), format!("{} *", Self::type_to_c(inner))));
        let params_str = Self::format_params(&new_params);
        format!("int32_t {}({});\n", ffi_name, params_str)
    }

    fn generate_classes(classes: &[Class], prefix: &str) -> String {
        classes
            .iter()
            .map(|c| Self::generate_class(c, prefix))
            .collect()
    }

    fn generate_class(class: &Class, prefix: &str) -> String {
        let mut out = String::new();
        let snake_name = naming::to_snake_case(&class.name);
        let class_prefix = format!("{}_{}", prefix, snake_name);

        out.push_str(&format!(
            "struct {} * {}_new(void);\n",
            class.name, class_prefix
        ));
        out.push_str(&format!(
            "FfiStatus {}_free(struct {} * handle);\n",
            class_prefix, class.name
        ));

        for method in &class.methods {
            out.push_str(&Self::generate_method(method, &class.name, &class_prefix));
        }

        for stream in &class.streams {
            out.push_str(&Self::generate_stream(stream, &class.name, &class_prefix));
        }

        out
    }

    fn generate_method(method: &Method, class_name: &str, class_prefix: &str) -> String {
        let ffi_name = format!("{}_{}", class_prefix, method.name);

        let mut params: Vec<(String, String)> =
            vec![("handle".to_string(), format!("struct {} *", class_name))];

        for p in &method.inputs {
            params.extend(Self::param_to_c(&p.name, &p.param_type));
        }

        if method.is_async {
            Self::generate_async_function(&ffi_name, &params, &method.output)
        } else {
            Self::generate_sync_function(&ffi_name, &params, &method.output)
        }
    }

    fn generate_stream(stream: &StreamMethod, class_name: &str, class_prefix: &str) -> String {
        let base_name = format!("{}_{}", class_prefix, stream.name);
        let item_type = Self::type_to_c(&stream.item_type);

        format!(
            "SubscriptionHandle {}(const struct {} *handle);\n\
             uintptr_t {}_pop_batch(SubscriptionHandle subscription_handle, {} *output_ptr, uintptr_t output_capacity);\n\
             int32_t {}_wait(SubscriptionHandle subscription_handle, uint32_t timeout_milliseconds);\n\
             void {}_poll(SubscriptionHandle subscription_handle, uint64_t callback_data, StreamContinuationCallback callback);\n\
             void {}_unsubscribe(SubscriptionHandle subscription_handle);\n\
             void {}_free(SubscriptionHandle subscription_handle);\n",
            base_name, class_name, base_name, item_type, base_name, base_name, base_name, base_name,
        )
    }

    fn generate_free_string(prefix: &str) -> String {
        format!("\nvoid {}_free_string(FfiString s);\n", prefix)
    }

    fn build_params(inputs: &[Parameter]) -> Vec<(String, String)> {
        inputs
            .iter()
            .flat_map(|p| Self::param_to_c(&p.name, &p.param_type))
            .collect()
    }

    fn param_to_c(name: &str, ty: &Type) -> Vec<(String, String)> {
        match ty {
            Type::String => vec![
                (format!("{}_ptr", name), "const uint8_t*".to_string()),
                (format!("{}_len", name), "uintptr_t".to_string()),
            ],
            Type::Slice(inner) => vec![
                (
                    format!("{}_ptr", name),
                    format!("const {}*", Self::type_to_c(inner)),
                ),
                (format!("{}_len", name), "uintptr_t".to_string()),
            ],
            Type::MutSlice(inner) => vec![
                (
                    format!("{}_ptr", name),
                    format!("{}*", Self::type_to_c(inner)),
                ),
                (format!("{}_len", name), "uintptr_t".to_string()),
            ],
            Type::Vec(inner) => vec![
                (
                    format!("{}_ptr", name),
                    format!("const {}*", Self::type_to_c(inner)),
                ),
                (format!("{}_len", name), "uintptr_t".to_string()),
            ],
            Type::Callback(inner) => {
                let inner_c_type = Self::type_to_c(inner);
                let callback_type = if inner.is_void() {
                    "void (*)(void*)".to_string()
                } else {
                    format!("void (*)(void*, {})", inner_c_type)
                };
                vec![
                    (format!("{}_cb", name), callback_type),
                    (format!("{}_ud", name), "void*".to_string()),
                ]
            }
            _ => vec![(name.to_string(), Self::type_to_c(ty))],
        }
    }

    fn format_params(params: &[(String, String)]) -> String {
        if params.is_empty() {
            "void".to_string()
        } else {
            params
                .iter()
                .map(|(name, ty)| {
                    if ty.contains("(*)") {
                        ty.replace("(*)", &format!("(*{})", name))
                    } else {
                        format!("{} {}", ty, name)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn return_type_to_c(output: &Option<Type>) -> String {
        match output {
            None | Some(Type::Void) => "FfiStatus".to_string(),
            Some(Type::Primitive(p)) => p.c_type_name().to_string(),
            Some(Type::Record(name)) => name.clone(),
            Some(Type::Enum(name)) => name.clone(),
            _ => "FfiStatus".to_string(),
        }
    }

    fn async_result_type(output: &Option<Type>) -> String {
        match output {
            None | Some(Type::Void) => "void".to_string(),
            Some(Type::String) => "FfiString".to_string(),
            Some(Type::Vec(inner)) => {
                let inner_c = Self::primitive_to_cbindgen_name(inner);
                format!("FfiBuf_{}", inner_c)
            }
            Some(Type::Option(inner)) => {
                let inner_c = Self::primitive_to_cbindgen_name(inner);
                format!("FfiOption_{}", inner_c)
            }
            Some(Type::Result { ok, .. }) => Self::async_result_type(&Some(*ok.clone())),
            Some(ty) => Self::type_to_c(ty),
        }
    }

    fn primitive_to_cbindgen_name(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => match p {
                Primitive::I8 => "i8",
                Primitive::I16 => "i16",
                Primitive::I32 => "i32",
                Primitive::I64 => "i64",
                Primitive::U8 => "u8",
                Primitive::U16 => "u16",
                Primitive::U32 => "u32",
                Primitive::U64 => "u64",
                Primitive::F32 => "f32",
                Primitive::F64 => "f64",
                Primitive::Bool => "bool",
                Primitive::Usize => "usize",
                Primitive::Isize => "isize",
            }
            .to_string(),
            _ => Self::type_to_c(ty),
        }
    }

    fn type_to_c(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => p.c_type_name().to_string(),
            Type::String => "FfiString".to_string(),
            Type::Bytes => "uint8_t*".to_string(),
            Type::Record(name) | Type::Enum(name) => name.clone(),
            Type::Object(name) => format!("struct {}*", name),
            Type::BoxedTrait(name) => format!("struct Foreign{}*", name),
            Type::Vec(inner) => format!("{}*", Self::type_to_c(inner)),
            Type::Option(inner) => Self::type_to_c(inner),
            Type::Slice(inner) => format!("const {}*", Self::type_to_c(inner)),
            Type::MutSlice(inner) => format!("{}*", Self::type_to_c(inner)),
            Type::Callback(_) => "void*".to_string(),
            Type::Void => "void".to_string(),
            Type::Result { ok, .. } => Self::type_to_c(ok),
        }
    }
}
