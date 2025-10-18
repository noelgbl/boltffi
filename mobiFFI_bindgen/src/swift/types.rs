use crate::model::{Primitive, Type};

use super::NamingConvention;

pub struct TypeMapper;

impl TypeMapper {
    pub fn map_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(primitive) => Self::map_primitive(*primitive),
            Type::String => "String".into(),
            Type::Bytes => "Data".into(),
            Type::Vec(inner) => format!("[{}]", Self::map_type(inner)),
            Type::Option(inner) => format!("{}?", Self::map_type(inner)),
            Type::Result { ok, .. } => Self::map_type(ok),
            Type::Callback(inner) => format!("({}) -> Void", Self::map_type(inner)),
            Type::Object(name) => NamingConvention::class_name(name),
            Type::Record(name) => NamingConvention::class_name(name),
            Type::Enum(name) => NamingConvention::class_name(name),
            Type::Void => "Void".into(),
        }
    }

    pub fn map_primitive(primitive: Primitive) -> String {
        match primitive {
            Primitive::Bool => "Bool",
            Primitive::I8 => "Int8",
            Primitive::U8 => "UInt8",
            Primitive::I16 => "Int16",
            Primitive::U16 => "UInt16",
            Primitive::I32 => "Int32",
            Primitive::U32 => "UInt32",
            Primitive::I64 => "Int64",
            Primitive::U64 => "UInt64",
            Primitive::F32 => "Float",
            Primitive::F64 => "Double",
            Primitive::Usize => "UInt",
            Primitive::Isize => "Int",
        }
        .into()
    }

    pub fn ffi_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(primitive) => Self::ffi_primitive(*primitive),
            Type::String => "UnsafePointer<CChar>".into(),
            Type::Bytes => "UnsafePointer<UInt8>".into(),
            Type::Vec(_) => "UnsafeMutableRawPointer".into(),
            Type::Option(inner) => Self::ffi_type(inner),
            Type::Result { ok, .. } => Self::ffi_type(ok),
            Type::Callback(inner) => {
                format!(
                    "@convention(c) (UnsafeMutableRawPointer?, {}) -> Void",
                    Self::ffi_type(inner)
                )
            }
            Type::Object(_) => "OpaquePointer".into(),
            Type::Record(name) => NamingConvention::class_name(name),
            Type::Enum(_) => "Int32".into(),
            Type::Void => "Void".into(),
        }
    }

    fn ffi_primitive(primitive: Primitive) -> String {
        match primitive {
            Primitive::Bool => "Bool",
            Primitive::I8 => "Int8",
            Primitive::U8 => "UInt8",
            Primitive::I16 => "Int16",
            Primitive::U16 => "UInt16",
            Primitive::I32 => "Int32",
            Primitive::U32 => "UInt32",
            Primitive::I64 => "Int64",
            Primitive::U64 => "UInt64",
            Primitive::F32 => "Float",
            Primitive::F64 => "Double",
            Primitive::Usize => "UInt",
            Primitive::Isize => "Int",
        }
        .into()
    }

    pub fn default_value(ty: &Type) -> String {
        match ty {
            Type::Primitive(primitive) => Self::primitive_default(*primitive),
            Type::String => "\"\"".into(),
            Type::Bytes => "Data()".into(),
            Type::Vec(_) => "[]".into(),
            Type::Option(_) => "nil".into(),
            Type::Void => "()".into(),
            _ => "/* default */".into(),
        }
    }

    fn primitive_default(primitive: Primitive) -> String {
        match primitive {
            Primitive::Bool => "false",
            Primitive::F32 | Primitive::F64 => "0.0",
            _ => "0",
        }
        .into()
    }

    pub fn needs_conversion(ty: &Type) -> bool {
        matches!(
            ty,
            Type::String | Type::Bytes | Type::Vec(_) | Type::Option(_) | Type::Object(_)
        )
    }
}
