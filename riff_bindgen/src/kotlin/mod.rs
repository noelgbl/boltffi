mod jni;
mod layout;
mod marshal;
mod names;
mod templates;
mod types;

use askama::Template;

pub use jni::JniGenerator;
pub use marshal::{JniParamInfo, JniReturnKind, ParamConversion, ReturnKind};
pub use names::NamingConvention;
pub use templates::{
    CStyleEnumTemplate, ClassTemplate, DataEnumCodecTemplate, FunctionTemplate, NativeTemplate,
    PreambleTemplate, RecordReaderTemplate, RecordTemplate, RecordWriterTemplate,
    SealedEnumTemplate,
};
pub use types::TypeMapper;

use crate::model::{Class, Enumeration, Function, Module, Record, Type};

pub struct Kotlin;

impl Kotlin {
    pub fn render_module(module: &Module) -> String {
        Self::render_module_with_package(module, &module.name)
    }

    pub fn render_module_with_package(module: &Module, package_name: &str) -> String {
        let mut sections = Vec::new();

        sections.push(Self::render_preamble_with_package(package_name));

        module.enums.iter().for_each(|enumeration| {
            sections.push(Self::render_enum(enumeration));
            if enumeration.is_data_enum() || enumeration.is_error {
                sections.push(Self::render_data_enum_codec(enumeration));
            }
        });

        let blittable_vec_return_records = Self::find_blittable_vec_return_records(module);
        let blittable_vec_param_records = Self::find_blittable_vec_param_records(module);

        module.records.iter().for_each(|record| {
            sections.push(Self::render_record(record));
            if blittable_vec_return_records.contains(&record.name.as_str()) {
                sections.push(Self::render_record_reader(record));
            }
            if blittable_vec_param_records.contains(&record.name.as_str()) {
                sections.push(Self::render_record_writer(record));
            }
        });

        module
            .functions
            .iter()
            .filter(|func| Self::is_supported_function(func, module))
            .for_each(|function| sections.push(Self::render_function(function, module)));

        module
            .classes
            .iter()
            .for_each(|class| sections.push(Self::render_class(class)));

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

    pub fn render_preamble_with_package(package_name: &str) -> String {
        PreambleTemplate::with_package(package_name)
            .render()
            .expect("preamble template failed")
    }

    pub fn render_enum(enumeration: &Enumeration) -> String {
        if enumeration.is_c_style() && !enumeration.is_error {
            CStyleEnumTemplate::from_enum(enumeration)
                .render()
                .expect("c-style enum template failed")
        } else {
            SealedEnumTemplate::from_enum(enumeration)
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
        RecordTemplate::from_record(record)
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

    pub fn render_function(function: &Function, module: &Module) -> String {
        FunctionTemplate::from_function(function, module)
            .render()
            .expect("function template failed")
    }

    pub fn render_class(class: &Class) -> String {
        ClassTemplate::from_class(class)
            .render()
            .expect("class template failed")
    }

    pub fn render_native(module: &Module) -> String {
        NativeTemplate::from_module(module)
            .render()
            .expect("native template failed")
    }

    fn find_blittable_vec_return_records(module: &Module) -> std::collections::HashSet<&str> {
        module
            .functions
            .iter()
            .filter_map(|func| {
                if let Some(Type::Vec(inner)) = &func.output {
                    if let Type::Record(record_name) = inner.as_ref() {
                        let is_blittable = module
                            .records
                            .iter()
                            .find(|record| &record.name == record_name)
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

    fn is_supported_function(func: &Function, module: &Module) -> bool {
        if func.is_async {
            return false;
        }

        let supported_output = match &func.output {
            None => true,
            Some(Type::Primitive(_)) => true,
            Some(Type::String) => true,
            Some(Type::Enum(_)) => true,
            Some(Type::Vec(inner)) => match inner.as_ref() {
                Type::Primitive(_) => true,
                Type::Record(record_name) => Self::is_record_blittable(record_name, module),
                _ => false,
            },
            Some(Type::Option(inner)) => Self::is_supported_option_inner(inner, module),
            Some(Type::Result { ok, .. }) => Self::is_supported_result_ok(ok, module),
            _ => false,
        };

        let supported_inputs = func.inputs.iter().all(|param| match &param.param_type {
            Type::Primitive(_) | Type::String | Type::Enum(_) => true,
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
                Type::Primitive(_) => true,
                Type::Record(name) => Self::is_record_blittable(name, module),
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
            "List<Double>"
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

        let output = Kotlin::render_class(&sensor_class);
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
}
