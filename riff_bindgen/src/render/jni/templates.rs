use std::fmt::{self, Display, Formatter};

use askama::Template;

use super::plan::{
    JniAsyncCallbackInvoker, JniAsyncFunction, JniCallbackTrait, JniClass, JniClosureTrampoline,
    JniFunction, JniModule, JniParam, JniReturnAbi, JniWireCtor, JniWireFunction, JniWireMethod,
};

#[derive(Template)]
#[template(path = "jni_glue.txt", escape = "none")]
pub struct JniGlueTemplate<'a> {
    pub prefix: &'a str,
    pub jni_prefix: &'a str,
    pub package_path: &'a str,
    pub module_name: &'a str,
    pub class_name: &'a str,
    pub has_async: bool,
    pub has_async_callbacks: bool,
    pub functions: &'a [JniFunction],
    pub wire_functions: &'a [JniWireFunction],
    pub async_functions: &'a [JniAsyncFunction],
    pub classes: &'a [JniClass],
    pub callback_traits: &'a [JniCallbackTrait],
    pub async_callback_invokers: &'a [JniAsyncCallbackInvoker],
    pub closure_trampolines: &'a [JniClosureTrampoline],
}

impl<'a> JniGlueTemplate<'a> {
    pub fn new(module: &'a JniModule) -> Self {
        Self {
            prefix: module.prefix.as_str(),
            jni_prefix: module.jni_prefix.as_str(),
            package_path: module.package_path.as_str(),
            module_name: module.module_name.as_str(),
            class_name: module.class_name.as_str(),
            has_async: module.has_async,
            has_async_callbacks: module.has_async_callbacks,
            functions: &module.functions,
            wire_functions: &module.wire_functions,
            async_functions: &module.async_functions,
            classes: &module.classes,
            callback_traits: &module.callback_traits,
            async_callback_invokers: &module.async_callback_invokers,
            closure_trampolines: &module.closure_trampolines,
        }
    }
}

#[derive(Template)]
#[template(path = "jni_wire_function.txt", escape = "none")]
pub struct JniWireFunctionTemplate<'a> {
    pub ffi_name: &'a str,
    pub jni_name: &'a str,
    pub jni_params: &'a str,
    pub params: &'a [JniParam],
    pub return_abi: &'a JniReturnAbi,
}

impl<'a> JniWireFunctionTemplate<'a> {
    pub fn new(func: &'a JniWireFunction) -> Self {
        Self {
            ffi_name: func.ffi_name.as_str(),
            jni_name: func.jni_name.as_str(),
            jni_params: func.jni_params.as_str(),
            params: &func.params,
            return_abi: &func.return_abi,
        }
    }
}

impl Display for JniWireFunction {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        JniWireFunctionTemplate::new(self)
            .render()
            .map_err(|_| fmt::Error)
            .and_then(|rendered| formatter.write_str(&rendered))
    }
}

#[derive(Template)]
#[template(path = "jni_wire_method.txt", escape = "none")]
pub struct JniWireMethodTemplate<'a> {
    pub ffi_name: &'a str,
    pub jni_name: &'a str,
    pub jni_params: &'a str,
    pub params: &'a [JniParam],
    pub return_abi: &'a JniReturnAbi,
    pub include_handle: bool,
}

impl<'a> JniWireMethodTemplate<'a> {
    pub fn new(method: &'a JniWireMethod) -> Self {
        Self {
            ffi_name: method.ffi_name.as_str(),
            jni_name: method.jni_name.as_str(),
            jni_params: method.jni_params.as_str(),
            params: &method.params,
            return_abi: &method.return_abi,
            include_handle: method.include_handle,
        }
    }
}

impl Display for JniWireMethod {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        JniWireMethodTemplate::new(self)
            .render()
            .map_err(|_| fmt::Error)
            .and_then(|rendered| formatter.write_str(&rendered))
    }
}

#[derive(Template)]
#[template(path = "jni_wire_ctor.txt", escape = "none")]
pub struct JniWireCtorTemplate<'a> {
    pub ffi_name: &'a str,
    pub jni_name: &'a str,
    pub jni_params: &'a str,
    pub params: &'a [JniParam],
}

impl<'a> JniWireCtorTemplate<'a> {
    pub fn new(ctor: &'a JniWireCtor) -> Self {
        Self {
            ffi_name: ctor.ffi_name.as_str(),
            jni_name: ctor.jni_name.as_str(),
            jni_params: ctor.jni_params.as_str(),
            params: &ctor.params,
        }
    }
}

impl Display for JniWireCtor {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        JniWireCtorTemplate::new(self)
            .render()
            .map_err(|_| fmt::Error)
            .and_then(|rendered| formatter.write_str(&rendered))
    }
}

#[cfg(test)]
mod tests {
    use crate::ir;
    use crate::model::{
        CallbackTrait, Class, ClosureSignature, Constructor, ConstructorParam, Enumeration,
        Function, Method, Module, Parameter, Primitive, Receiver, Record, RecordField, ReturnType,
        TraitMethod, TraitMethodParam, Type, Variant,
    };
    use crate::render::jni::{JniEmitter, JniLowerer};

    fn build_test_module() -> Module {
        let point = Record::new("Point")
            .with_field(RecordField::new("x", Type::Primitive(Primitive::F64)))
            .with_field(RecordField::new("y", Type::Primitive(Primitive::F64)));

        let message = Record::new("Message").with_field(RecordField::new("text", Type::String));

        let color = Enumeration::new("Color")
            .with_variant(Variant::new("red").with_discriminant(0))
            .with_variant(Variant::new("green").with_discriminant(1))
            .with_variant(Variant::new("blue").with_discriminant(2));

        let shape = Enumeration::new("Shape").with_variant(
            Variant::new("circle")
                .with_field(RecordField::new("radius", Type::Primitive(Primitive::F64))),
        );

        let api_error = Enumeration::new("ApiError").as_error().with_variant(
            Variant::new("failed")
                .with_field(RecordField::new("code", Type::Primitive(Primitive::I32))),
        );

        let listener = CallbackTrait::new("Listener")
            .with_method(
                TraitMethod::new("on_value")
                    .with_param(TraitMethodParam::new(
                        "value",
                        Type::Primitive(Primitive::I32),
                    ))
                    .with_return(ReturnType::Void),
            )
            .with_method(
                TraitMethod::new("on_done")
                    .with_param(TraitMethodParam::new(
                        "status",
                        Type::Primitive(Primitive::U32),
                    ))
                    .with_return(ReturnType::value(Type::String))
                    .make_async(),
            );

        let engine = Class::new("Engine")
            .with_constructor(
                Constructor::new().with_param(ConstructorParam::new("name", Type::String)),
            )
            .with_method(
                Method::new("ping", Receiver::Ref).with_output(Type::Primitive(Primitive::I32)),
            )
            .with_method(
                Method::new("fetch", Receiver::Ref)
                    .with_output(Type::String)
                    .make_async(),
            );

        let closure = ClosureSignature::single_param(Type::Record("Point".to_string()));
        let closure_function = Function::new("with_closure")
            .with_param(Parameter::new("callback", Type::Closure(closure.clone())))
            .with_return(ReturnType::Void);

        Module::new("test")
            .with_record(point)
            .with_record(message)
            .with_enum(color)
            .with_enum(shape)
            .with_enum(api_error)
            .with_callback_trait(listener)
            .with_class(engine)
            .with_function(
                Function::new("add")
                    .with_param(Parameter::new("a", Type::Primitive(Primitive::I32)))
                    .with_param(Parameter::new("b", Type::Primitive(Primitive::I32)))
                    .with_output(Type::Primitive(Primitive::I32)),
            )
            .with_function(
                Function::new("echo_point")
                    .with_param(Parameter::new("point", Type::Record("Point".to_string())))
                    .with_output(Type::Record("Point".to_string())),
            )
            .with_function(
                Function::new("echo_color")
                    .with_param(Parameter::new("color", Type::Enum("Color".to_string())))
                    .with_output(Type::Enum("Color".to_string())),
            )
            .with_function(
                Function::new("echo_shape")
                    .with_param(Parameter::new("shape", Type::Enum("Shape".to_string())))
                    .with_output(Type::Enum("Shape".to_string())),
            )
            .with_function(Function::new("fallible").with_return(ReturnType::fallible(
                Type::Primitive(Primitive::I32),
                Type::Enum("ApiError".to_string()),
            )))
            .with_function(closure_function)
    }

    #[test]
    fn jni_ir_generates_valid_glue() {
        let module = build_test_module();
        let package = "com.example";
        let class_name = "BenchRiff";

        let mut ir_module = module.clone();
        let contract = ir::build_contract(&mut ir_module);
        let abi_contract = ir::Lowerer::new(&contract).to_abi_contract();
        let jni_module = JniLowerer::new(
            &contract,
            &abi_contract,
            package.to_string(),
            class_name.to_string(),
        )
        .lower();
        let ir_code = JniEmitter::emit(&jni_module);

        assert!(!ir_code.is_empty());
        assert!(ir_code.contains("riff_free_buf"));
        assert!(ir_code.contains("jbyteArray"));
        assert!(!ir_code.contains("NewDirectByteBuffer(env, (void*)_buf.ptr"));
    }
}
