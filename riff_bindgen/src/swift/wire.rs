use super::names::NamingConvention;
use crate::model::{Module, Primitive, Type};

const OFFSET_PLACEHOLDER: &str = "OFFSET";

#[derive(Debug, Clone)]
pub struct TypeCodec {
    pub reader_expr: String,
    pub size_kind: SizeKind,
}

#[derive(Debug, Clone)]
pub enum SizeKind {
    Fixed(usize),
    Variable,
}

impl TypeCodec {
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

    pub fn as_tuple_reader(&self) -> String {
        let expr = self.reader_expr.replace(OFFSET_PLACEHOLDER, "$0");
        match &self.size_kind {
            SizeKind::Fixed(size) => format!("({}, {})", expr, size),
            SizeKind::Variable => expr,
        }
    }

    pub fn value_only(&self) -> String {
        let expr = self.reader_expr.replace(OFFSET_PLACEHOLDER, "$0");
        match &self.size_kind {
            SizeKind::Fixed(_) => expr,
            SizeKind::Variable => format!("{}.value", expr),
        }
    }

    pub fn value_at(&self, offset: &str) -> String {
        let expr = self.reader_expr.replace(OFFSET_PLACEHOLDER, offset);
        match &self.size_kind {
            SizeKind::Fixed(_) => expr,
            SizeKind::Variable => format!("{}.value", expr),
        }
    }

    pub fn decode_to_binding(&self, name: &str, offset_var: &str) -> String {
        let expr = self.reader_expr.replace(OFFSET_PLACEHOLDER, offset_var);
        match &self.size_kind {
            SizeKind::Fixed(size) => format!("let {} = {}; {} += {}", name, expr, offset_var, size),
            SizeKind::Variable => format!(
                "let ({}, {}Size) = {}; {} += {}Size",
                name, name, expr, offset_var, name
            ),
        }
    }

    pub fn decode_as_tuple(&self, offset_var: &str) -> String {
        let expr = self.reader_expr.replace(OFFSET_PLACEHOLDER, offset_var);
        match &self.size_kind {
            SizeKind::Fixed(size) => format!("({}, {})", expr, size),
            SizeKind::Variable => expr,
        }
    }

    pub fn as_stream_item_closure(&self, offset_var: &str) -> String {
        let expr = self.reader_expr.replace(OFFSET_PLACEHOLDER, offset_var);
        match &self.size_kind {
            SizeKind::Fixed(size) => format!(
                "{{ let v = {}; {} += {}; return v }}()",
                expr, offset_var, size
            ),
            SizeKind::Variable => format!(
                "{{ let (v, s) = {}; {} += s; return v }}()",
                expr, offset_var
            ),
        }
    }
}

pub fn decode_type(ty: &Type, module: &Module) -> TypeCodec {
    match ty {
        Type::Primitive(p) => decode_primitive(*p),
        Type::String => TypeCodec::variable(format!("wire.readString(at: {})", OFFSET_PLACEHOLDER)),
        Type::Record(name) => decode_record(name, module),
        Type::Enum(name) => decode_enum(name, module),
        Type::Vec(inner) => decode_vec(inner, module),
        Type::Option(inner) => decode_option(inner, module),
        Type::Result { ok, err } => decode_result(ok, err, module),
        Type::Bytes => TypeCodec::variable(format!(
            "wire.readBytesWithSize(at: {})",
            OFFSET_PLACEHOLDER
        )),
        other => panic!("wire decode not supported for type: {:?}", other),
    }
}

fn decode_primitive(p: Primitive) -> TypeCodec {
    let (read_fn, size) = primitive_wire_info(p);
    TypeCodec::fixed(
        format!("wire.{}(at: {})", read_fn, OFFSET_PLACEHOLDER),
        size,
    )
}

fn decode_record(name: &str, module: &Module) -> TypeCodec {
    let class_name = NamingConvention::class_name(name);
    let is_blittable = module
        .records
        .iter()
        .find(|r| &r.name == name)
        .map(|r| r.is_blittable())
        .unwrap_or(false);

    if is_blittable {
        let size = module
            .records
            .iter()
            .find(|r| &r.name == name)
            .map(|r| r.struct_size().as_usize())
            .unwrap_or(0);
        TypeCodec::fixed(
            format!(
                "wire.readBlittable(at: {}, as: {}.self)",
                OFFSET_PLACEHOLDER, class_name
            ),
            size,
        )
    } else {
        TypeCodec::variable(format!(
            "{}.decode(wireBuffer: wire, at: {})",
            class_name, OFFSET_PLACEHOLDER
        ))
    }
}

fn decode_enum(name: &str, module: &Module) -> TypeCodec {
    let class_name = NamingConvention::class_name(name);
    let is_data = module.is_data_enum(name);

    if is_data {
        TypeCodec::variable(format!(
            "{}.decode(wireBuffer: wire, at: {})",
            class_name, OFFSET_PLACEHOLDER
        ))
    } else {
        TypeCodec::fixed(
            format!(
                "{}(fromC: wire.readI32(at: {}))",
                class_name, OFFSET_PLACEHOLDER
            ),
            4,
        )
    }
}

fn decode_vec(inner: &Type, module: &Module) -> TypeCodec {
    if let Type::Primitive(Primitive::U8) = inner {
        return TypeCodec::variable(format!(
            "wire.readBytesWithSize(at: {})",
            OFFSET_PLACEHOLDER
        ));
    }

    if let Type::Record(name) = inner {
        let is_blittable = module
            .records
            .iter()
            .find(|r| &r.name == name)
            .map(|r| r.is_blittable())
            .unwrap_or(false);
        if is_blittable {
            let class_name = NamingConvention::class_name(name);
            return TypeCodec::variable(format!(
                "wire.readBlittableArrayWithSize(at: {}, as: {}.self)",
                OFFSET_PLACEHOLDER, class_name
            ));
        }
    }

    let inner_codec = decode_type(inner, module);
    TypeCodec::variable(format!(
        "wire.readArray(at: {}, reader: {{ {} }})",
        OFFSET_PLACEHOLDER,
        inner_codec.as_tuple_reader()
    ))
}

fn decode_option(inner: &Type, module: &Module) -> TypeCodec {
    let inner_codec = decode_type(inner, module);
    TypeCodec::variable(format!(
        "wire.readOptional(at: {}, reader: {{ {} }})",
        OFFSET_PLACEHOLDER,
        inner_codec.as_tuple_reader()
    ))
}

fn decode_result(ok: &Type, err: &Type, module: &Module) -> TypeCodec {
    let ok_codec = decode_type(ok, module);
    let err_codec = decode_type(err, module);
    TypeCodec::variable(format!(
        "wire.readResult(at: {}, okReader: {{ {} }}, errReader: {{ {} }})",
        OFFSET_PLACEHOLDER,
        ok_codec.as_tuple_reader(),
        err_codec.as_tuple_reader()
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

pub fn decode_value_at_offset(ty: &Type, module: &Module, offset: &str) -> String {
    decode_type(ty, module).value_at(offset)
}

pub struct TypeEncoder {
    pub size_expr: String,
    pub encode_to_data: String,
    pub encode_to_bytes: String,
}

pub fn encode_type(ty: &Type, name: &str, module: &Module) -> TypeEncoder {
    match ty {
        Type::Primitive(p) => encode_primitive(*p, name),
        Type::String => encode_string(name),
        Type::Record(name_str) => encode_record(name_str, name, module),
        Type::Enum(enum_name) => encode_enum(enum_name, name, module),
        Type::Vec(inner) => encode_vec(inner, name, module),
        Type::Option(inner) => encode_option(inner, name, module),
        Type::Result { ok, err } => encode_result(ok, err, name, module),
        Type::Bytes => encode_bytes(name),
        other => panic!("wire encode not supported for type: {:?}", other),
    }
}

fn encode_bytes(name: &str) -> TypeEncoder {
    TypeEncoder {
        size_expr: format!("(4 + {}.count)", name),
        encode_to_data: format!("data.appendBytes({})", name),
        encode_to_bytes: format!("bytes.appendBytes({})", name),
    }
}

fn encode_primitive(p: Primitive, name: &str) -> TypeEncoder {
    let (append_fn, size) = primitive_encode_info(p);
    TypeEncoder {
        size_expr: size.to_string(),
        encode_to_data: format!("data.{}({})", append_fn, name),
        encode_to_bytes: format!("bytes.{}({})", append_fn, name),
    }
}

fn encode_string(name: &str) -> TypeEncoder {
    TypeEncoder {
        size_expr: format!("(4 + {}.utf8.count)", name),
        encode_to_data: format!("data.appendString({})", name),
        encode_to_bytes: format!("bytes.appendString({})", name),
    }
}

fn encode_record(record_name: &str, field_name: &str, module: &Module) -> TypeEncoder {
    let is_blittable = module
        .records
        .iter()
        .find(|r| &r.name == record_name)
        .map(|r| r.is_blittable())
        .unwrap_or(false);

    if is_blittable {
        let size = module
            .records
            .iter()
            .find(|r| &r.name == record_name)
            .map(|r| r.struct_size().as_usize())
            .unwrap_or(0);
        TypeEncoder {
            size_expr: size.to_string(),
            encode_to_data: format!(
                "withUnsafeBytes(of: {}) {{ data.append(contentsOf: $0) }}",
                field_name
            ),
            encode_to_bytes: format!(
                "withUnsafeBytes(of: {}) {{ bytes.append(contentsOf: $0) }}",
                field_name
            ),
        }
    } else {
        TypeEncoder {
            size_expr: format!("{}.wireEncodedSize()", field_name),
            encode_to_data: format!("{}.wireEncodeTo(&data)", field_name),
            encode_to_bytes: format!("{}.wireEncodeToBytes(&bytes)", field_name),
        }
    }
}

fn encode_enum(enum_name: &str, field_name: &str, module: &Module) -> TypeEncoder {
    let is_data = module.is_data_enum(enum_name);

    if is_data {
        TypeEncoder {
            size_expr: format!("{}.wireEncodedSize()", field_name),
            encode_to_data: format!("{}.wireEncodeTo(&data)", field_name),
            encode_to_bytes: format!("{}.wireEncodeToBytes(&bytes)", field_name),
        }
    } else {
        TypeEncoder {
            size_expr: "4".into(),
            encode_to_data: format!("data.appendI32({}.rawValue)", field_name),
            encode_to_bytes: format!("bytes.appendI32({}.rawValue)", field_name),
        }
    }
}

fn encode_vec(inner: &Type, name: &str, module: &Module) -> TypeEncoder {
    let inner_encoder = encode_type(inner, "ITEM", module);

    let is_blittable_record = matches!(inner, Type::Record(rec_name) if module
        .records
        .iter()
        .find(|r| &r.name == rec_name)
        .map(|r| r.is_blittable())
        .unwrap_or(false));

    let size_expr = match inner {
        Type::Primitive(p) => format!("(4 + {}.count * {})", name, p.size_bytes()),
        Type::Record(_) if is_blittable_record => {
            format!(
                "(4 + {}.count * {})",
                name,
                inner_encoder.size_expr.replace("ITEM", "")
            )
        }
        _ => {
            let inner_size = inner_encoder.size_expr.replace("ITEM", "$1");
            format!("(4 + {}.reduce(0) {{ $0 + {} }})", name, inner_size)
        }
    };

    let encode_to_data = match inner {
        Type::Primitive(_) => format!("data.appendArray({})", name),
        Type::Record(_) if is_blittable_record => format!("data.appendBlittableArray({})", name),
        _ => {
            let inner_encode = inner_encoder.encode_to_data.replace("ITEM", "item");
            format!(
                "data.appendU32(UInt32({}.count)); for item in {} {{ {} }}",
                name, name, inner_encode
            )
        }
    };

    let encode_to_bytes = match inner {
        Type::Primitive(_) => format!("bytes.appendArray({})", name),
        Type::Record(_) if is_blittable_record => format!("bytes.appendBlittableArray({})", name),
        _ => {
            let inner_encode = inner_encoder.encode_to_bytes.replace("ITEM", "item");
            format!(
                "bytes.appendU32(UInt32({}.count)); for item in {} {{ {} }}",
                name, name, inner_encode
            )
        }
    };

    TypeEncoder {
        size_expr,
        encode_to_data,
        encode_to_bytes,
    }
}

fn encode_option(inner: &Type, name: &str, module: &Module) -> TypeEncoder {
    let inner_encoder = encode_type(inner, "v", module);

    TypeEncoder {
        size_expr: format!(
            "({}.map {{ v in 1 + {} }} ?? 1)",
            name, inner_encoder.size_expr
        ),
        encode_to_data: format!(
            "if let v = {} {{ data.appendU8(1); {} }} else {{ data.appendU8(0) }}",
            name, inner_encoder.encode_to_data
        ),
        encode_to_bytes: format!(
            "if let v = {} {{ bytes.appendU8(1); {} }} else {{ bytes.appendU8(0) }}",
            name, inner_encoder.encode_to_bytes
        ),
    }
}

fn encode_result(ok: &Type, err: &Type, name: &str, module: &Module) -> TypeEncoder {
    let ok_encoder = encode_type(ok, "okVal", module);
    let err_encoder = encode_type(err, "errVal", module);

    TypeEncoder {
        size_expr: format!(
            "({{ switch {} {{ case .success(let okVal): return 1 + {}; case .failure(let errVal): return 1 + {} }} }}())",
            name, ok_encoder.size_expr, err_encoder.size_expr
        ),
        encode_to_data: format!(
            "switch {} {{ case .success(let okVal): data.appendU8(0); {}; case .failure(let errVal): data.appendU8(1); {} }}",
            name, ok_encoder.encode_to_data, err_encoder.encode_to_data
        ),
        encode_to_bytes: format!(
            "switch {} {{ case .success(let okVal): bytes.appendU8(0); {}; case .failure(let errVal): bytes.appendU8(1); {} }}",
            name, ok_encoder.encode_to_bytes, err_encoder.encode_to_bytes
        ),
    }
}

fn primitive_encode_info(p: Primitive) -> (&'static str, usize) {
    match p {
        Primitive::Bool => ("appendBool", 1),
        Primitive::I8 => ("appendI8", 1),
        Primitive::U8 => ("appendU8", 1),
        Primitive::I16 => ("appendI16", 2),
        Primitive::U16 => ("appendU16", 2),
        Primitive::I32 => ("appendI32", 4),
        Primitive::U32 => ("appendU32", 4),
        Primitive::I64 => ("appendI64", 8),
        Primitive::U64 => ("appendU64", 8),
        Primitive::F32 => ("appendF32", 4),
        Primitive::F64 => ("appendF64", 8),
        Primitive::Isize => ("appendI64", 8),
        Primitive::Usize => ("appendU64", 8),
    }
}
