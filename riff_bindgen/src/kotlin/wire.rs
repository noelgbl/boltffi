use super::names::NamingConvention;
use crate::model::{Module, Primitive, Type};

const OFFSET_PLACEHOLDER: &str = "OFFSET";

#[derive(Debug, Clone)]
pub struct KotlinCodec {
    pub reader_expr: String,
    pub size_kind: SizeKind,
}

#[derive(Debug, Clone)]
pub enum SizeKind {
    Fixed(usize),
    Variable,
}

impl KotlinCodec {
    pub fn fixed(reader_expr: impl Into<String>, size: usize) -> Self {
        Self {
            reader_expr: reader_expr.into(),
            size_kind: SizeKind::Fixed(size),
        }
    }

    pub fn variable(reader_expr: impl Into<String>) -> Self {
        Self {
            reader_expr: reader_expr.into(),
            size_kind: SizeKind::Variable,
        }
    }

    pub fn as_lambda_reader(&self) -> String {
        let expr = self.reader_expr.replace(OFFSET_PLACEHOLDER, "it");
        match &self.size_kind {
            SizeKind::Fixed(size) => format!("{{ {} to {} }}", expr, size),
            SizeKind::Variable => format!("{{ {} }}", expr),
        }
    }

    pub fn value_only(&self) -> String {
        let expr = self.reader_expr.replace(OFFSET_PLACEHOLDER, "it");
        match &self.size_kind {
            SizeKind::Fixed(_) => expr,
            SizeKind::Variable => format!("{}.first", expr),
        }
    }

    pub fn value_at(&self, offset: &str) -> String {
        let expr = self.reader_expr.replace(OFFSET_PLACEHOLDER, offset);
        match &self.size_kind {
            SizeKind::Fixed(_) => expr,
            SizeKind::Variable => format!("{}.first", expr),
        }
    }

    pub fn lambda_body_at(&self, offset_var: &str) -> String {
        let expr = self.reader_expr.replace(OFFSET_PLACEHOLDER, offset_var);
        match &self.size_kind {
            SizeKind::Fixed(size) => format!("{} to {}", expr, size),
            SizeKind::Variable => expr,
        }
    }

    pub fn decode_to_binding(&self, name: &str, offset_var: &str) -> String {
        let expr = self.reader_expr.replace(OFFSET_PLACEHOLDER, offset_var);
        match &self.size_kind {
            SizeKind::Fixed(size) => {
                format!("val {} = {}; {} += {}", name, expr, offset_var, size)
            }
            SizeKind::Variable => {
                format!(
                    "val ({}, {}Size) = {}; {} += {}Size",
                    name, name, expr, offset_var, name
                )
            }
        }
    }
}

pub fn decode_type(ty: &Type, module: &Module) -> KotlinCodec {
    match ty {
        Type::Primitive(p) => decode_primitive(*p),
        Type::Void => KotlinCodec::fixed("Unit", 0),
        Type::String => KotlinCodec::variable(format!("wire.readString({})", OFFSET_PLACEHOLDER)),
        Type::Record(name) => decode_record(name, module),
        Type::Enum(name) => decode_enum(name, module),
        Type::Vec(inner) => decode_vec(inner, module),
        Type::Option(inner) => decode_option(inner, module),
        Type::Result { ok, err } => decode_result(ok, err, module),
        Type::Bytes => KotlinCodec::variable(format!("wire.readBytes({})", OFFSET_PLACEHOLDER)),
        other => panic!("Kotlin wire decode not supported for type: {:?}", other),
    }
}

fn decode_primitive(p: Primitive) -> KotlinCodec {
    let (read_fn, size) = primitive_wire_info(p);
    KotlinCodec::fixed(format!("wire.{}({})", read_fn, OFFSET_PLACEHOLDER), size)
}

fn decode_record(name: &str, _module: &Module) -> KotlinCodec {
    let class_name = NamingConvention::class_name(name);
    KotlinCodec::variable(format!(
        "{}.decode(wire, {})",
        class_name, OFFSET_PLACEHOLDER
    ))
}

fn decode_enum(name: &str, module: &Module) -> KotlinCodec {
    let class_name = NamingConvention::class_name(name);
    let is_data = module.is_data_enum(name);

    if is_data {
        KotlinCodec::variable(format!(
            "{}.decode(wire, {})",
            class_name, OFFSET_PLACEHOLDER
        ))
    } else {
        KotlinCodec::fixed(
            format!(
                "{}.fromValue(wire.readI32({}))",
                class_name, OFFSET_PLACEHOLDER
            ),
            4,
        )
    }
}

fn decode_vec(inner: &Type, module: &Module) -> KotlinCodec {
    if let Type::Primitive(p) = inner {
        let reader = match p {
            Primitive::U8 | Primitive::I8 => "readByteArray",
            Primitive::I16 | Primitive::U16 => "readShortArray",
            Primitive::I32 | Primitive::U32 => "readIntArray",
            Primitive::I64 | Primitive::U64 | Primitive::Isize | Primitive::Usize => {
                "readLongArray"
            }
            Primitive::F32 => "readFloatArray",
            Primitive::F64 => "readDoubleArray",
            Primitive::Bool => "readBooleanArray",
        };
        return KotlinCodec::variable(format!("wire.{}({})", reader, OFFSET_PLACEHOLDER));
    }

    let inner_codec = decode_type(inner, module);
    KotlinCodec::variable(format!(
        "wire.readList({}) {}",
        OFFSET_PLACEHOLDER,
        inner_codec.as_lambda_reader()
    ))
}

fn decode_option(inner: &Type, module: &Module) -> KotlinCodec {
    let inner_codec = decode_type(inner, module);
    KotlinCodec::variable(format!(
        "wire.readNullable({}) {}",
        OFFSET_PLACEHOLDER,
        inner_codec.as_lambda_reader()
    ))
}

fn decode_result(ok: &Type, err: &Type, module: &Module) -> KotlinCodec {
    let ok_codec = decode_type(ok, module);
    let err_codec = decode_type(err, module);
    KotlinCodec::variable(format!(
        "wire.readResult({}, {}, {})",
        OFFSET_PLACEHOLDER,
        ok_codec.as_lambda_reader(),
        err_codec.as_lambda_reader()
    ))
}

fn primitive_wire_info(p: Primitive) -> (&'static str, usize) {
    match p {
        Primitive::Bool => ("readBool", 1),
        Primitive::I8 => ("readI8", 1),
        Primitive::U8 => ("readU8", 1),
        Primitive::I16 => ("readI16", 2),
        Primitive::U16 => ("readU16", 2),
        Primitive::I32 => ("readI32", 4),
        Primitive::U32 => ("readU32", 4),
        Primitive::I64 => ("readI64", 8),
        Primitive::U64 => ("readU64", 8),
        Primitive::F32 => ("readF32", 4),
        Primitive::F64 => ("readF64", 8),
        Primitive::Isize => ("readI64", 8),
        Primitive::Usize => ("readU64", 8),
    }
}

pub struct KotlinEncoder {
    pub size_expr: String,
    pub encode_expr: String,
}

pub fn encode_type(ty: &Type, name: &str, module: &Module) -> KotlinEncoder {
    match ty {
        Type::Void => KotlinEncoder {
            size_expr: "0".into(),
            encode_expr: String::new(),
        },
        Type::Primitive(p) => encode_primitive(*p, name),
        Type::String => encode_string(name),
        Type::Record(record_name) => encode_record(record_name, name, module),
        Type::Enum(enum_name) => encode_enum(enum_name, name, module),
        Type::Vec(inner) => encode_vec(inner, name, module),
        Type::Option(inner) => encode_option(inner, name, module),
        Type::Result { ok, err } => encode_result(ok, err, name, module),
        Type::Bytes => encode_bytes(name),
        other => panic!("Kotlin wire encode not supported for type: {:?}", other),
    }
}

fn encode_primitive(p: Primitive, name: &str) -> KotlinEncoder {
    let (write_fn, size) = primitive_encode_info(p);
    KotlinEncoder {
        size_expr: size.to_string(),
        encode_expr: format!("wire.{}({})", write_fn, name),
    }
}

fn encode_string(name: &str) -> KotlinEncoder {
    KotlinEncoder {
        size_expr: format!("(4 + Utf8Codec.maxBytes({}))", name),
        encode_expr: format!("wire.writeString({})", name),
    }
}

fn encode_bytes(name: &str) -> KotlinEncoder {
    KotlinEncoder {
        size_expr: format!("(4 + {}.size)", name),
        encode_expr: format!("wire.writeBytes({})", name),
    }
}

fn encode_record(_record_name: &str, field_name: &str, _module: &Module) -> KotlinEncoder {
    KotlinEncoder {
        size_expr: format!("{}.wireEncodedSize()", field_name),
        encode_expr: format!("{}.wireEncodeTo(wire)", field_name),
    }
}

fn encode_enum(enum_name: &str, field_name: &str, module: &Module) -> KotlinEncoder {
    let is_data = module.is_data_enum(enum_name);

    if is_data {
        KotlinEncoder {
            size_expr: format!("{}.wireEncodedSize()", field_name),
            encode_expr: format!("{}.wireEncodeTo(wire)", field_name),
        }
    } else {
        KotlinEncoder {
            size_expr: "4".into(),
            encode_expr: format!("wire.writeI32({}.value)", field_name),
        }
    }
}

fn encode_vec(inner: &Type, name: &str, module: &Module) -> KotlinEncoder {
    let inner_encoder = encode_type(inner, "item", module);

    let record_struct_size = inner
        .record_name()
        .and_then(|record_name| module.records.iter().find(|record| record.name == record_name))
        .filter(|record| record.is_blittable())
        .map(|record| record.struct_size().as_usize());

    let size_expr = match inner {
        Type::Primitive(p) => format!("(4 + {}.size * {})", name, p.size_bytes()),
        Type::Record(_) if record_struct_size.is_some() => {
            format!("(4 + {}.size * {})", name, record_struct_size.unwrap())
        }
        _ => {
            format!(
                "(4 + {}.sumOf {{ item -> {} }})",
                name, inner_encoder.size_expr
            )
        }
    };

    let encode_expr = match inner {
        Type::Primitive(_) => format!("wire.writePrimitiveList({})", name),
        Type::Record(record_name) if record_struct_size.is_some() => format!(
            "wire.writeU32({}.size.toUInt()); {}Writer.writeAllToWire(wire, {})",
            name,
            NamingConvention::class_name(record_name),
            name
        ),
        _ => {
            format!(
                "wire.writeU32({}.size.toUInt()); {}.forEach {{ item -> {} }}",
                name, name, inner_encoder.encode_expr
            )
        }
    };

    KotlinEncoder {
        size_expr,
        encode_expr,
    }
}

fn encode_option(inner: &Type, name: &str, module: &Module) -> KotlinEncoder {
    let inner_encoder = encode_type(inner, "v", module);

    KotlinEncoder {
        size_expr: format!(
            "({}?.let {{ v -> 1 + {} }} ?: 1)",
            name, inner_encoder.size_expr
        ),
        encode_expr: format!(
            "{}?.let {{ v -> wire.writeU8(1u); {} }} ?: wire.writeU8(0u)",
            name, inner_encoder.encode_expr
        ),
    }
}

fn encode_result(ok: &Type, err: &Type, name: &str, module: &Module) -> KotlinEncoder {
    let ok_encoder = encode_type(ok, "okVal", module);
    let err_type = super::types::TypeMapper::map_type(err);
    let err_encoder = encode_type(err, "e", module);

    KotlinEncoder {
        size_expr: format!(
            "{}.fold({{ okVal -> 1 + {} }}, {{ t -> val e = t as {}; 1 + {} }})",
            name, ok_encoder.size_expr, err_type, err_encoder.size_expr
        ),
        encode_expr: format!(
            "{}.fold({{ okVal -> wire.writeU8(0u); {} }}, {{ t -> val e = t as {}; wire.writeU8(1u); {} }})",
            name, ok_encoder.encode_expr, err_type, err_encoder.encode_expr
        ),
    }
}

fn primitive_encode_info(p: Primitive) -> (&'static str, usize) {
    match p {
        Primitive::Bool => ("writeBool", 1),
        Primitive::I8 => ("writeI8", 1),
        Primitive::U8 => ("writeU8", 1),
        Primitive::I16 => ("writeI16", 2),
        Primitive::U16 => ("writeU16", 2),
        Primitive::I32 => ("writeI32", 4),
        Primitive::U32 => ("writeU32", 4),
        Primitive::I64 => ("writeI64", 8),
        Primitive::U64 => ("writeU64", 8),
        Primitive::F32 => ("writeF32", 4),
        Primitive::F64 => ("writeF64", 8),
        Primitive::Isize => ("writeI64", 8),
        Primitive::Usize => ("writeU64", 8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_module() -> Module {
        Module::new("test")
    }

    #[test]
    fn test_decode_primitives() {
        let module = empty_module();

        let codec = decode_type(&Type::Primitive(Primitive::I32), &module);
        assert!(codec.reader_expr.contains("readI32"));
        assert!(matches!(codec.size_kind, SizeKind::Fixed(4)));

        let codec = decode_type(&Type::Primitive(Primitive::Bool), &module);
        assert!(codec.reader_expr.contains("readBool"));
        assert!(matches!(codec.size_kind, SizeKind::Fixed(1)));
    }

    #[test]
    fn test_decode_string() {
        let module = empty_module();
        let codec = decode_type(&Type::String, &module);
        assert!(codec.reader_expr.contains("readString"));
        assert!(matches!(codec.size_kind, SizeKind::Variable));
    }

    #[test]
    fn test_decode_vec_nested() {
        let module = empty_module();
        let nested = Type::Vec(Box::new(Type::Option(Box::new(Type::String))));
        let codec = decode_type(&nested, &module);
        assert!(codec.reader_expr.contains("readList"));
        assert!(codec.reader_expr.contains("readNullable"));
        assert!(codec.reader_expr.contains("readString"));
    }

    #[test]
    fn test_decode_option() {
        let module = empty_module();
        let opt = Type::Option(Box::new(Type::Primitive(Primitive::I64)));
        let codec = decode_type(&opt, &module);
        assert!(codec.reader_expr.contains("readNullable"));
        assert!(codec.reader_expr.contains("readI64"));
    }

    #[test]
    fn test_encode_primitives() {
        let module = empty_module();

        let encoder = encode_type(&Type::Primitive(Primitive::I32), "value", &module);
        assert_eq!(encoder.size_expr, "4");
        assert!(encoder.encode_expr.contains("writeI32"));
    }

    #[test]
    fn test_encode_vec_string() {
        let module = empty_module();
        let vec = Type::Vec(Box::new(Type::String));
        let encoder = encode_type(&vec, "items", &module);
        assert!(encoder.size_expr.contains("sumOf"));
        assert!(encoder.encode_expr.contains("forEach"));
    }
}
