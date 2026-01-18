mod jni;
mod layout;
mod marshal;
mod call_plan;
mod names;
mod primitives;
mod return_abi;
mod templates;
mod types;
mod wire;

pub use return_abi::ReturnAbi;

use std::collections::HashSet;

use askama::Template;

pub use jni::JniGenerator;
pub use marshal::{JniParamInfo, JniReturnKind, ParamConversion};
pub use names::NamingConvention;
pub use templates::{
    AsyncFunctionTemplate, CStyleEnumTemplate, CallbackTraitTemplate, ClassTemplate,
    ClosureInterfaceTemplate, DataEnumCodecTemplate, NativeTemplate,
    PreambleTemplate, RecordReaderTemplate, RecordTemplate, RecordWriterTemplate,
    SealedEnumTemplate, WireFunctionTemplate,
};
pub use types::TypeMapper;

use crate::model::{
    CallbackTrait, Class, ClosureSignature, Enumeration, Function, Module, Record, ReturnType, Type,
};

pub fn is_primitive_only(func: &Function) -> bool {
    matches!(
        &func.returns,
        ReturnType::Void | ReturnType::Value(Type::Void) | ReturnType::Value(Type::Primitive(_))
    ) && func
        .inputs
        .iter()
        .all(|p| matches!(&p.param_type, Type::Primitive(_)))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FactoryStyle {
    #[default]
    Constructors,
    CompanionMethods,
}

#[derive(Debug, Clone, Default)]
pub struct KotlinOptions {
    pub factory_style: FactoryStyle,
}

pub struct Kotlin;

impl Kotlin {
    pub fn render_module(module: &Module) -> String {
        Self::render_module_with_package(module, &module.name)
    }

    pub fn render_module_with_package(module: &Module, package_name: &str) -> String {
        Self::render_module_with_options(module, package_name, &KotlinOptions::default())
    }

    pub fn render_module_with_options(
        module: &Module,
        package_name: &str,
        options: &KotlinOptions,
    ) -> String {
        let mut sections = Vec::new();

        sections.push(Self::render_preamble_with_package(package_name, module));

        module.enums.iter().for_each(|enumeration| {
            sections.push(Self::render_enum_with_module(enumeration, module));
            if enumeration.is_data_enum() || enumeration.is_error {
                sections.push(Self::render_data_enum_codec(enumeration));
            }
        });

        let blittable_vec_return_records = Self::find_blittable_vec_return_records(module);
        let blittable_vec_param_records = Self::find_blittable_vec_param_records(module);
        let async_return_records = Self::find_async_return_records(module);

        module.records.iter().for_each(|record| {
            sections.push(Self::render_record_with_module(record, module));
            let needs_reader = blittable_vec_return_records.contains(&record.name.as_str())
                || async_return_records.contains(&record.name.as_str());
            if needs_reader {
                sections.push(Self::render_record_reader(record));
            }
            if blittable_vec_param_records.contains(&record.name.as_str()) {
                sections.push(Self::render_record_writer(record));
            }
        });

        Self::collect_unique_closures(module)
            .iter()
            .for_each(|sig| sections.push(Self::render_closure_interface(sig)));

        module
            .functions
            .iter()
            .filter(|func| !func.is_async)
            .for_each(|function| sections.push(Self::render_function(function, module)));

        module
            .functions
            .iter()
            .filter(|func| func.is_async && Self::is_supported_async_function(func, module))
            .for_each(|function| sections.push(Self::render_function(function, module)));

        module
            .classes
            .iter()
            .for_each(|class| sections.push(Self::render_class(class, module, options)));

        module
            .callback_traits
            .iter()
            .for_each(|t| sections.push(Self::render_callback_trait(t, module)));

        sections.push(Self::render_native(module));

        let mut output = sections
            .into_iter()
            .map(|section| section.trim().to_string())
            .filter(|section| !section.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        output.push('\n');
        output
    }

    pub fn render_preamble(module: &Module) -> String {
        PreambleTemplate::from_module(module)
            .render()
            .expect("preamble template failed")
    }

    pub fn render_preamble_with_package(package_name: &str, module: &Module) -> String {
        PreambleTemplate::with_package_and_module(package_name, module)
            .render()
            .expect("preamble template failed")
    }

    pub fn render_enum(enumeration: &Enumeration) -> String {
        Self::render_enum_with_module(enumeration, &Module::new(""))
    }

    pub fn render_enum_with_module(enumeration: &Enumeration, module: &Module) -> String {
        if enumeration.is_c_style() && !enumeration.is_error {
            CStyleEnumTemplate::from_enum(enumeration)
                .render()
                .expect("c-style enum template failed")
        } else {
            SealedEnumTemplate::from_enum_with_module(enumeration, module)
                .render()
                .expect("sealed enum template failed")
        }
    }

    pub fn render_data_enum_codec(enumeration: &Enumeration) -> String {
        DataEnumCodecTemplate::from_enum(enumeration)
            .render()
            .expect("data enum codec template failed")
    }

    pub fn render_record(record: &Record) -> String {
        Self::render_record_with_module(record, &Module::new(""))
    }

    pub fn render_record_with_module(record: &Record, module: &Module) -> String {
        RecordTemplate::from_record_with_module(record, module)
            .render()
            .expect("record template failed")
    }

    pub fn render_record_reader(record: &Record) -> String {
        RecordReaderTemplate::from_record(record)
            .render()
            .expect("record reader template failed")
    }

    pub fn render_record_writer(record: &Record) -> String {
        RecordWriterTemplate::from_record(record)
            .render()
            .expect("record writer template failed")
    }

    pub fn render_closure_interface(sig: &ClosureSignature) -> String {
        ClosureInterfaceTemplate::from_signature(sig, "")
            .render()
            .expect("closure interface template failed")
    }

    fn collect_unique_closures(module: &Module) -> Vec<ClosureSignature> {
        let mut seen_signature_ids = HashSet::<String>::new();

        module
            .functions
            .iter()
            .flat_map(|function| function.inputs.iter())
            .map(|param| &param.param_type)
            .chain(
                module
                    .classes
                    .iter()
                    .flat_map(|class| class.methods.iter())
                    .flat_map(|method| method.inputs.iter())
                    .map(|param| &param.param_type),
            )
            .filter_map(|param_type| match param_type {
                Type::Closure(signature) => {
                    let signature_id = signature.signature_id();
                    seen_signature_ids
                        .insert(signature_id)
                        .then_some(signature.clone())
                }
                _ => None,
            })
            .collect()
    }

    pub fn render_function(function: &Function, module: &Module) -> String {
        if function.is_async {
            AsyncFunctionTemplate::from_function(function, module)
                .render()
                .expect("async function template failed")
        } else {
            WireFunctionTemplate::from_function(function, module)
                .render()
                .expect("wire function template failed")
        }
    }

    pub fn render_class(class: &Class, module: &Module, options: &KotlinOptions) -> String {
        ClassTemplate::from_class(class, module, options)
            .render()
            .expect("class template failed")
    }

    pub fn render_native(module: &Module) -> String {
        NativeTemplate::from_module(module)
            .render()
            .expect("native template failed")
    }

    pub fn render_callback_trait(callback_trait: &CallbackTrait, module: &Module) -> String {
        CallbackTraitTemplate::from_trait(callback_trait, module)
            .render()
            .expect("callback trait template failed")
    }

    fn find_blittable_vec_return_records(module: &Module) -> std::collections::HashSet<&str> {
        module
            .functions
            .iter()
            .filter_map(|func| {
                if let Some(Type::Vec(inner)) = func.returns.ok_type() {
                    if let Type::Record(record_name) = inner.as_ref() {
                        let is_blittable = module
                            .records
                            .iter()
                            .find(|record| record.name == *record_name)
                            .map(|record| record.is_blittable())
                            .unwrap_or(false);
                        if is_blittable {
                            return Some(record_name.as_str());
                        }
                    }
                }
                None
            })
            .collect()
    }

    fn find_blittable_vec_param_records(module: &Module) -> std::collections::HashSet<&str> {
        module
            .functions
            .iter()
            .flat_map(|func| func.inputs.iter())
            .filter_map(|param| match &param.param_type {
                Type::Vec(inner) | Type::Slice(inner) => match inner.as_ref() {
                    Type::Record(record_name) => {
                        let is_blittable = module
                            .records
                            .iter()
                            .find(|record| &record.name == record_name)
                            .map(|record| record.is_blittable())
                            .unwrap_or(false);
                        if is_blittable {
                            Some(record_name.as_str())
                        } else {
                            None
                        }
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    fn find_async_return_records(module: &Module) -> HashSet<&str> {
        module
            .functions
            .iter()
            .filter(|func| func.is_async)
            .filter_map(|func| {
                if let Some(Type::Record(record_name)) = func.returns.ok_type() {
                    let is_blittable = module
                        .records
                        .iter()
                        .find(|record| record.name == *record_name)
                        .map(|record| record.is_blittable())
                        .unwrap_or(false);
                    if is_blittable {
                        return Some(record_name.as_str());
                    }
                }
                None
            })
            .collect()
    }

    pub fn is_supported_function(func: &Function, module: &Module) -> bool {
        if func.is_async {
            return Self::is_supported_async_function(func, module);
        }
        if func.wire_encoded {
            return true;
        }
        let supported_output = match &func.returns {
            ReturnType::Void => true,
            ReturnType::Fallible { ok, .. } => Self::is_supported_result_ok(ok, module),
            ReturnType::Value(ty) => match ty {
                Type::Void => true,
                Type::Primitive(_) => true,
                Type::String => true,
                Type::Enum(_) => true,
                Type::Vec(inner) => match inner.as_ref() {
                    Type::Primitive(_) => true,
                    Type::Record(record_name) => Self::is_record_blittable(record_name, module),
                    _ => false,
                },
                Type::Option(inner) => Self::is_supported_option_inner(inner, module),
                _ => false,
            },
        };

        let supported_inputs = func.inputs.iter().all(|param| match &param.param_type {
            Type::Primitive(_) | Type::String | Type::Enum(_) | Type::Closure(_) => true,
            Type::Record(name) => Self::is_record_blittable(name, module),
            Type::Vec(inner) | Type::Slice(inner) => match inner.as_ref() {
                Type::Primitive(_) => true,
                Type::Record(record_name) => Self::is_record_blittable(record_name, module),
                _ => false,
            },
            _ => false,
        });

        supported_output && supported_inputs
    }

    fn is_supported_option_inner(inner: &Type, module: &Module) -> bool {
        match inner {
            Type::Primitive(_) | Type::String => true,
            Type::Record(name) => Self::is_record_blittable(name, module),
            Type::Enum(name) => module.enums.iter().any(|e| &e.name == name),
            Type::Vec(vec_inner) => match vec_inner.as_ref() {
                Type::Primitive(_) | Type::String => true,
                Type::Record(name) => Self::is_record_blittable(name, module),
                Type::Enum(name) => module
                    .enums
                    .iter()
                    .any(|e| &e.name == name && !e.is_data_enum()),
                _ => false,
            },
            _ => false,
        }
    }

    fn is_supported_result_ok(ok: &Type, module: &Module) -> bool {
        match ok {
            Type::Primitive(_) | Type::String | Type::Void => true,
            Type::Record(name) => Self::is_record_blittable(name, module),
            Type::Enum(name) => module.enums.iter().any(|e| &e.name == name),
            Type::Vec(inner) => match inner.as_ref() {
                Type::Primitive(_) => true,
                Type::Record(name) => Self::is_record_blittable(name, module),
                _ => false,
            },
            Type::Option(inner) => Self::is_supported_option_inner(inner, module),
            _ => false,
        }
    }

    fn is_record_blittable(record_name: &str, module: &Module) -> bool {
        module
            .records
            .iter()
            .find(|record| record.name == record_name)
            .map(|record| record.is_blittable())
            .unwrap_or(false)
    }

    fn is_supported_async_function(func: &Function, module: &Module) -> bool {
        call_plan::AsyncCallPlan::supports_call(&func.inputs, &func.returns, module)
    }

    pub fn is_supported_async_output(returns: &ReturnType, module: &Module) -> bool {
        call_plan::AsyncCallPlan::supports_returns(returns, module)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Constructor, Method, Module, Parameter, Primitive, Receiver, RecordField, Type, Variant,
    };

    #[test]
    fn test_kotlin_type_mapping() {
        assert_eq!(
            TypeMapper::map_type(&Type::Primitive(Primitive::I32)),
            "Int"
        );
        assert_eq!(
            TypeMapper::map_type(&Type::Primitive(Primitive::I64)),
            "Long"
        );
        assert_eq!(
            TypeMapper::map_type(&Type::Primitive(Primitive::Bool)),
            "Boolean"
        );
        assert_eq!(TypeMapper::map_type(&Type::String), "String");
        assert_eq!(TypeMapper::map_type(&Type::Bytes), "ByteArray");
        assert_eq!(
            TypeMapper::map_type(&Type::Vec(Box::new(Type::Primitive(Primitive::F64)))),
            "DoubleArray"
        );
        assert_eq!(
            TypeMapper::map_type(&Type::Vec(Box::new(Type::Record("Point".into())))),
            "List<Point>"
        );
    }

    #[test]
    fn test_kotlin_naming() {
        assert_eq!(
            NamingConvention::class_name("sensor_manager"),
            "SensorManager"
        );
        assert_eq!(NamingConvention::method_name("get_reading"), "getReading");
        assert_eq!(NamingConvention::enum_entry_name("active"), "ACTIVE");
    }

    #[test]
    fn test_kotlin_keyword_escaping() {
        assert_eq!(NamingConvention::escape_keyword("value"), "`value`");
        assert_eq!(NamingConvention::escape_keyword("count"), "count");
    }

    #[test]
    fn test_render_c_style_enum() {
        let status = Enumeration::new("sensor_status")
            .with_variant(Variant::new("idle").with_discriminant(0))
            .with_variant(Variant::new("active").with_discriminant(1))
            .with_variant(Variant::new("error").with_discriminant(2));

        let output = Kotlin::render_enum(&status);
        assert!(output.contains("enum class SensorStatus"));
        assert!(output.contains("IDLE(0)"));
        assert!(output.contains("ACTIVE(1)"));
        assert!(output.contains("fromValue(value: Int)"));
    }

    #[test]
    fn test_render_sealed_class_enum() {
        let result_enum = Enumeration::new("api_result")
            .with_variant(Variant::new("success"))
            .with_variant(
                Variant::new("error")
                    .with_field(RecordField::new("code", Type::Primitive(Primitive::I32))),
            );

        let output = Kotlin::render_enum(&result_enum);
        assert!(output.contains("sealed class ApiResult"));
        assert!(output.contains("data object Success"));
        assert!(output.contains("data class Error"));
        assert!(output.contains("val code: Int"));
    }

    #[test]
    fn test_render_record() {
        let reading = Record::new("sensor_reading")
            .with_field(RecordField::new(
                "timestamp",
                Type::Primitive(Primitive::U64),
            ))
            .with_field(RecordField::new(
                "temperature",
                Type::Primitive(Primitive::F64),
            ));

        let output = Kotlin::render_record(&reading);
        assert!(output.contains("data class SensorReading"));
        assert!(output.contains("val timestamp: ULong"));
        assert!(output.contains("val temperature: Double"));
    }

    #[test]
    fn test_render_function() {
        let function = Function::new("get_sensor_value")
            .with_param(Parameter::new("sensor_id", Type::Primitive(Primitive::I32)))
            .with_output(Type::Primitive(Primitive::F64));

        let module = Module::new("test");
        let output = Kotlin::render_function(&function, &module);
        assert!(output.contains("fun getSensorValue"));
        assert!(output.contains("sensorId: Int"));
        assert!(output.contains(": Double"));
    }

    #[test]
    fn test_render_class() {
        let sensor_class = Class::new("sensor")
            .with_constructor(Constructor::new())
            .with_method(
                Method::new("get_reading", Receiver::Ref)
                    .with_output(Type::Primitive(Primitive::F64)),
            );

        let module = Module::new("test");
        let output = Kotlin::render_class(&sensor_class, &module, &KotlinOptions::default());
        assert!(output.contains("class Sensor"));
        assert!(output.contains("private val handle: Long"));
        assert!(output.contains("override fun close()"));
        assert!(output.contains("fun getReading()"));
    }

    #[test]
    fn test_render_string_function() {
        let function = Function::new("fetch_data").with_output(Type::String);

        let module = Module::new("test");
        let output = Kotlin::render_function(&function, &module);
        assert!(output.contains("fun fetchData(): String"));
        assert!(output.contains("Native.riff_fetch_data"));
    }

    #[test]
    fn test_render_native() {
        let module = Module::new("mylib")
            .with_function(
                Function::new("get_version").with_output(Type::Primitive(Primitive::I32)),
            )
            .with_class(
                Class::new("sensor")
                    .with_constructor(Constructor::new())
                    .with_method(Method::new("read", Receiver::Ref)),
            );

        let output = Kotlin::render_native(&module);
        assert!(output.contains("private object Native"));
        assert!(output.contains("System.loadLibrary"));
        assert!(output.contains("@JvmStatic external fun riff_get_version"));
        assert!(output.contains("@JvmStatic external fun riff_sensor_new"));
        assert!(output.contains("@JvmStatic external fun riff_sensor_free"));
        assert!(output.contains("@JvmStatic external fun riff_sensor_read"));
    }

    #[test]
    fn test_blittable_record_wire_encoding() {
        let point = Record::new("point")
            .with_field(RecordField::new("x", Type::Primitive(Primitive::I32)))
            .with_field(RecordField::new("y", Type::Primitive(Primitive::I32)));

        let module = Module::new("test");
        let output = Kotlin::render_record_with_module(&point, &module);

        assert!(output.contains("data class Point"));
        assert!(output.contains("companion object"));
        assert!(output.contains("const val SIZE_BYTES: Int = 8"));
        assert!(output.contains("fun decode(wire: WireBuffer, offset: Int)"));
        assert!(output.contains("wire.readI32(offset + 0)"));
        assert!(output.contains("wire.readI32(offset + 4)"));
        assert!(output.contains(") to SIZE_BYTES"));
        assert!(output.contains("fun wireEncodedSize(): Int = SIZE_BYTES"));
        assert!(output.contains("fun wireEncodeTo(wire: WireWriter)"));
    }

    #[test]
    fn test_non_blittable_record_wire_encoding() {
        let profile = Record::new("user_profile")
            .with_field(RecordField::new("id", Type::Primitive(Primitive::I64)))
            .with_field(RecordField::new("name", Type::String))
            .with_field(RecordField::new(
                "email",
                Type::Option(Box::new(Type::String)),
            ));

        let module = Module::new("test");
        let output = Kotlin::render_record_with_module(&profile, &module);

        assert!(output.contains("data class UserProfile"));
        assert!(output.contains("companion object"));
        assert!(output.contains("fun decode(wire: WireBuffer, offset: Int)"));
        assert!(output.contains("wire.readI64(pos)"));
        assert!(output.contains("wire.readString(pos)"));
        assert!(output.contains("wire.readNullable(pos)"));
        assert!(output.contains("fun wireEncodedSize(): Int"));
        assert!(output.contains("fun wireEncodeTo(wire: WireWriter)"));
    }

    #[test]
    fn test_sealed_enum_wire_codec() {
        let result_enum = Enumeration::new("api_response")
            .with_variant(
                Variant::new("success").with_field(RecordField::new("data", Type::String)),
            )
            .with_variant(
                Variant::new("error")
                    .with_field(RecordField::new("code", Type::Primitive(Primitive::I32))),
            );

        let module = Module::new("test");
        let output = Kotlin::render_enum_with_module(&result_enum, &module);

        assert!(output.contains("sealed class ApiResponse"));
        assert!(output.contains("companion object"));
        assert!(output.contains("fun decode(wire: WireBuffer, offset: Int)"));
        assert!(output.contains("fun wireEncodedSize(): Int"));
        assert!(output.contains("fun wireEncodeTo(wire: WireWriter)"));
        assert!(output.contains("wire.writeI32(0)"));
        assert!(output.contains("wire.writeI32(1)"));
    }

    #[test]
    fn test_wire_function_with_string_return() {
        let func = Function::new("get_greeting")
            .with_param(Parameter::new("name", Type::String))
            .with_output(Type::String)
            .with_wire_encoded();

        let module = Module::new("test");
        let output = Kotlin::render_function(&func, &module);

        assert!(output.contains("fun getGreeting(name: String): String"));
        assert!(output.contains("WireBuffer.fromByteBuffer"));
        assert!(output.contains("wire.readString(0).first"));
    }

    #[test]
    fn test_wire_function_with_result_return() {
        let func = Function::new("try_parse")
            .with_param(Parameter::new("input", Type::String))
            .with_return(ReturnType::Fallible {
                ok: Type::Primitive(Primitive::I64),
                err: Type::String,
            })
            .with_wire_encoded();

        let module = Module::new("test");
        let output = Kotlin::render_function(&func, &module);

        assert!(output.contains("@Throws(FfiException::class)"));
        assert!(output.contains("fun tryParse(input: String): Long"));
        assert!(output.contains("readResult"));
        assert!(output.contains("getOrThrow()"));
    }

    #[test]
    fn test_wire_function_with_vec_return() {
        let func = Function::new("get_numbers")
            .with_output(Type::Vec(Box::new(Type::Primitive(Primitive::I32))))
            .with_wire_encoded();

        let module = Module::new("test");
        let output = Kotlin::render_function(&func, &module);

        assert!(output.contains("fun getNumbers(): IntArray"));
        assert!(output.contains("WireBuffer"));
    }

    #[test]
    fn test_wire_encoded_function_bypasses_support_check() {
        let func = Function::new("complex_return")
            .with_output(Type::Vec(Box::new(Type::Option(Box::new(Type::String)))))
            .with_wire_encoded();

        let module = Module::new("test");
        assert!(Kotlin::is_supported_function(&func, &module));
    }

    #[test]
    fn test_wire_function_option_param_is_encoded() {
        let func = Function::new("count_optional")
            .with_param(Parameter::new("maybe_name", Type::Option(Box::new(Type::String))))
            .with_output(Type::Primitive(Primitive::I32));

        let module = Module::new("test").with_function(func);
        let output = Kotlin::render_module(&module);

        assert!(output.contains("fun countOptional(maybeName: String?): Int"));
        assert!(output.contains("WireWriter("));
        assert!(output.contains("writeU8(1u)"));
        assert!(output.contains("writeU8(0u)"));
        assert!(output.contains("writeString"));
        assert!(output.contains("external fun riff_count_optional(maybeName: ByteBuffer): Int"));
    }

    #[test]
    fn test_wire_function_closure_param_is_wrapped_to_interface() {
        let signature = ClosureSignature::single_param(Type::Primitive(Primitive::I32));
        let func = Function::new("with_callback").with_param(Parameter::new(
            "callback",
            Type::Closure(signature),
        ));

        let module = Module::new("test").with_function(func);
        let output = Kotlin::render_module(&module);

        assert!(output.contains("fun interface I32Callback"));
        assert!(output.contains("I32Callback { p0 -> callback(p0) }"));
        assert!(output.contains("external fun riff_with_callback(callback: I32Callback): Unit"));
    }
}
