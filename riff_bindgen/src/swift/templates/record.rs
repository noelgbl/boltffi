use askama::Template;

use crate::model::{Module, Record, Type};

use super::super::names::NamingConvention;
use super::super::types::TypeMapper;
use super::super::wire;

#[derive(Template)]
#[template(path = "swift/record.txt", escape = "none")]
pub struct RecordTemplate {
    pub class_name: String,
    pub fields: Vec<FieldView>,
    pub is_blittable: bool,
}

impl RecordTemplate {
    pub fn from_record(record: &Record, module: &Module) -> Self {
        let fields: Vec<FieldView> = record
            .fields
            .iter()
            .enumerate()
            .map(|(idx, field)| Self::make_field(field, idx, module))
            .collect();
        let is_blittable = record.fields.iter().all(|f| Self::is_type_blittable(&f.field_type));
        Self {
            class_name: NamingConvention::class_name(&record.name),
            fields,
            is_blittable,
        }
    }

    fn is_type_blittable(ty: &Type) -> bool {
        matches!(ty, Type::Primitive(_))
    }

    fn make_field(field: &crate::model::RecordField, _idx: usize, module: &Module) -> FieldView {
        let swift_name = NamingConvention::property_name(&field.name);
        let encoder = wire::encode_type(&field.field_type, &swift_name, module);
        
        FieldView {
            swift_name: swift_name.clone(),
            swift_type: TypeMapper::map_type(&field.field_type),
            wire_size_expr: encoder.size_expr,
            wire_decode_inline: Self::make_decode_inline(&field.field_type, module),
            wire_encode: encoder.encode_to_data,
            wire_encode_bytes: encoder.encode_to_bytes,
        }
    }

    fn make_decode_inline(ty: &Type, module: &Module) -> String {
        let codec = wire::decode_type(ty, module);
        let reader = codec.reader_expr.replace("OFFSET", "pos");
        match &codec.size_kind {
            wire::SizeKind::Fixed(size) => {
                format!("{{ let v = {}; pos += {}; return v }}()", reader, size)
            }
            wire::SizeKind::Variable => {
                format!("{{ let (v, s) = {}; pos += s; return v }}()", reader)
            }
        }
    }
}

pub struct FieldView {
    pub swift_name: String,
    pub swift_type: String,
    pub wire_size_expr: String,
    pub wire_decode_inline: String,
    pub wire_encode: String,
    pub wire_encode_bytes: String,
}
