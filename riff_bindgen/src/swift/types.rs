use crate::model::Type;

use super::NamingConvention;
use super::primitives;

pub struct TypeMapper;

impl TypeMapper {
    pub fn map_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => primitives::info(*p).swift_type.into(),
            Type::String => "String".into(),
            Type::Bytes => "Data".into(),
            Type::Slice(inner) => format!("[{}]", Self::map_type(inner)),
            Type::MutSlice(inner) => format!("[{}]", Self::map_type(inner)),
            Type::Vec(inner) => format!("[{}]", Self::map_type(inner)),
            Type::Option(inner) => format!("{}?", Self::map_type(inner)),
            Type::Result { ok, err } => {
                format!("Result<{}, {}>", Self::map_type(ok), Self::map_type(err))
            }
            Type::Closure(sig) => {
                let params = sig
                    .params
                    .iter()
                    .map(|p| Self::map_type(p))
                    .collect::<Vec<_>>()
                    .join(", ");
                let ret = if sig.returns.is_void() {
                    "Void".to_string()
                } else {
                    Self::map_type(&sig.returns)
                };
                format!("({}) -> {}", params, ret)
            }
            Type::Object(name) => NamingConvention::class_name(name),
            Type::Record(name) => NamingConvention::class_name(name),
            Type::Enum(name) => NamingConvention::class_name(name),
            Type::BoxedTrait(name) => format!("{}Protocol", NamingConvention::class_name(name)),
            Type::Void => "Void".into(),
        }
    }

    pub fn ffi_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => primitives::info(*p).swift_type.into(),
            Type::String => "UnsafePointer<CChar>".into(),
            Type::Bytes => "UnsafePointer<UInt8>".into(),
            Type::Slice(inner) => format!("UnsafePointer<{}>", Self::ffi_type(inner)),
            Type::MutSlice(inner) => format!("UnsafeMutablePointer<{}>", Self::ffi_type(inner)),
            Type::Vec(_) => "UnsafeMutableRawPointer".into(),
            Type::Option(inner) => Self::ffi_type(inner),
            Type::Result { ok, .. } => Self::ffi_type(ok),
            Type::Closure(sig) => {
                let params = std::iter::once("UnsafeMutableRawPointer?".to_string())
                    .chain(sig.params.iter().map(|p| Self::ffi_type(p)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let ret = if sig.returns.is_void() {
                    "Void".to_string()
                } else {
                    Self::ffi_type(&sig.returns)
                };
                format!("@convention(c) ({}) -> {}", params, ret)
            }
            Type::Object(_) => "OpaquePointer".into(),
            Type::Record(_) => "UnsafePointer<UInt8>?, UInt".into(),
            Type::Enum(_) => "Int32".into(),
            Type::BoxedTrait(_) => "OpaquePointer".into(),
            Type::Void => "Void".into(),
        }
    }
}
