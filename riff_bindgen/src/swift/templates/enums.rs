use askama::Template;

use crate::model::{Enumeration, Module, Type};

use super::super::names::NamingConvention;
use super::super::types::TypeMapper;
use super::super::wire;

#[derive(Template)]
#[template(path = "swift/enum_c_style.txt", escape = "none")]
pub struct CStyleEnumTemplate {
    pub class_name: String,
    pub variants: Vec<CStyleVariantView>,
    pub is_error: bool,
}

impl CStyleEnumTemplate {
    pub fn from_enum(enumeration: &Enumeration) -> Self {
        Self {
            class_name: NamingConvention::class_name(&enumeration.name),
            variants: enumeration
                .variants
                .iter()
                .enumerate()
                .map(|(index, variant)| CStyleVariantView {
                    swift_name: NamingConvention::enum_case_name(&variant.name),
                    discriminant: variant.discriminant.unwrap_or(index as i64),
                })
                .collect(),
            is_error: enumeration.is_error,
        }
    }
}

pub struct CStyleVariantView {
    pub swift_name: String,
    pub discriminant: i64,
}

#[derive(Template)]
#[template(path = "swift/enum_data.txt", escape = "none")]
pub struct DataEnumTemplate {
    pub class_name: String,
    pub ffi_type: String,
    pub variants: Vec<DataVariantView>,
    pub is_error: bool,
}

impl DataEnumTemplate {
    pub fn from_enum(enumeration: &Enumeration, module: &Module) -> Self {
        let ffi_module = NamingConvention::ffi_module_name(&module.name);
        Self {
            class_name: NamingConvention::class_name(&enumeration.name),
            ffi_type: format!("{}.{}", ffi_module, enumeration.name),
            is_error: enumeration.is_error,
            variants: enumeration
                .variants
                .iter()
                .map(|variant| {
                    let is_single_tuple =
                        variant.fields.len() == 1 && variant.fields[0].name.starts_with('_');
                    let fields: Vec<EnumFieldView> = variant
                        .fields
                        .iter()
                        .map(|field| {
                            let swift_name = NamingConvention::param_name(&field.name);
                            let c_name = field.name.clone();
                            let wire_decode = Self::enum_field_wire_decode(&field.field_type, &swift_name, module);
                            let wire_size = Self::enum_field_wire_size(&field.field_type, &swift_name, module);
                            let wire_encode = Self::enum_field_wire_encode(&field.field_type, &swift_name, module);
                            let wire_encode_bytes = Self::enum_field_wire_encode_bytes(&field.field_type, &swift_name, module);
                            EnumFieldView {
                                needs_alias: swift_name != c_name,
                                swift_name,
                                c_name,
                                swift_type: TypeMapper::map_type(&field.field_type),
                                wire_decode,
                                wire_size,
                                wire_encode,
                                wire_encode_bytes,
                            }
                        })
                        .collect();
                    let single_wire_decode = if is_single_tuple && !variant.fields.is_empty() {
                        Self::single_tuple_wire_decode(&variant.fields[0].field_type, module)
                    } else {
                        String::new()
                    };
                    let wire_encode_single = if is_single_tuple && !variant.fields.is_empty() {
                        Self::single_tuple_wire_encode(&variant.fields[0].field_type, module)
                    } else {
                        String::new()
                    };
                    let wire_encode_bytes_single = if is_single_tuple && !variant.fields.is_empty() {
                        Self::single_tuple_wire_encode_bytes(&variant.fields[0].field_type, module)
                    } else {
                        String::new()
                    };
                    DataVariantView {
                        swift_name: NamingConvention::enum_case_name(&variant.name),
                        c_name: variant.name.clone(),
                        tag_constant: format!("{}_TAG_{}", enumeration.name, variant.name),
                        is_single_tuple,
                        wire_decode: single_wire_decode,
                        wire_encode_single,
                        wire_encode_bytes_single,
                        fields,
                    }
                })
                .collect(),
        }
    }

    fn single_tuple_wire_decode(ty: &Type, module: &Module) -> String {
        wire::decode_type(ty, module).decode_as_tuple("pos")
    }

    fn enum_field_wire_decode(ty: &Type, name: &str, module: &Module) -> String {
        wire::decode_type(ty, module).decode_to_binding(name, "pos")
    }

    fn enum_field_wire_size(ty: &Type, name: &str, module: &Module) -> String {
        wire::encode_type(ty, name, module).size_expr
    }

    fn enum_field_wire_encode(ty: &Type, name: &str, module: &Module) -> String {
        wire::encode_type(ty, name, module).encode_to_data
    }

    fn single_tuple_wire_encode(ty: &Type, module: &Module) -> String {
        wire::encode_type(ty, "value", module).encode_to_data
    }

    fn enum_field_wire_encode_bytes(ty: &Type, name: &str, module: &Module) -> String {
        wire::encode_type(ty, name, module).encode_to_bytes
    }

    fn single_tuple_wire_encode_bytes(ty: &Type, module: &Module) -> String {
        wire::encode_type(ty, "value", module).encode_to_bytes
    }
}

pub struct EnumFieldView {
    pub swift_name: String,
    pub c_name: String,
    pub swift_type: String,
    pub needs_alias: bool,
    pub wire_decode: String,
    pub wire_size: String,
    pub wire_encode: String,
    pub wire_encode_bytes: String,
}

pub struct DataVariantView {
    pub swift_name: String,
    pub c_name: String,
    pub tag_constant: String,
    pub is_single_tuple: bool,
    pub wire_decode: String,
    pub wire_encode_single: String,
    pub wire_encode_bytes_single: String,
    pub fields: Vec<EnumFieldView>,
}
