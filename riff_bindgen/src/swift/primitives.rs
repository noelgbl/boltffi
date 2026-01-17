use crate::model::Primitive;

#[derive(Debug, Clone, Copy)]
pub struct SwiftPrimitiveInfo {
    pub swift_type: &'static str,
}

pub const fn info(p: Primitive) -> SwiftPrimitiveInfo {
    match p {
        Primitive::Bool => SwiftPrimitiveInfo { swift_type: "Bool" },
        Primitive::I8 => SwiftPrimitiveInfo { swift_type: "Int8" },
        Primitive::U8 => SwiftPrimitiveInfo {
            swift_type: "UInt8",
        },
        Primitive::I16 => SwiftPrimitiveInfo {
            swift_type: "Int16",
        },
        Primitive::U16 => SwiftPrimitiveInfo {
            swift_type: "UInt16",
        },
        Primitive::I32 => SwiftPrimitiveInfo {
            swift_type: "Int32",
        },
        Primitive::U32 => SwiftPrimitiveInfo {
            swift_type: "UInt32",
        },
        Primitive::I64 => SwiftPrimitiveInfo {
            swift_type: "Int64",
        },
        Primitive::U64 => SwiftPrimitiveInfo {
            swift_type: "UInt64",
        },
        Primitive::Isize => SwiftPrimitiveInfo { swift_type: "Int" },
        Primitive::Usize => SwiftPrimitiveInfo { swift_type: "UInt" },
        Primitive::F32 => SwiftPrimitiveInfo {
            swift_type: "Float",
        },
        Primitive::F64 => SwiftPrimitiveInfo {
            swift_type: "Double",
        },
    }
}
