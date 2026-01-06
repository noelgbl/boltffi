use super::{Primitive, Type};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionAbi {
    Packed { primitive: Primitive },
    Primitive { primitive: Primitive },
    String,
    Record { struct_size: usize },
    Enum,
    DataEnum { struct_size: usize },
    VecPrimitive { primitive: Primitive },
    VecRecord { struct_size: usize },
    VecString,
    VecEnum,
    VecDataEnum { struct_size: usize },
}

impl OptionAbi {
    pub fn from_type<F, E>(inner: &Type, struct_size: F, is_data_enum: E) -> Self
    where
        F: Fn(&str) -> usize,
        E: Fn(&str) -> bool,
    {
        match inner {
            Type::Primitive(p) => Self::from_primitive(*p),
            Type::String => Self::String,
            Type::Record(name) => Self::Record {
                struct_size: struct_size(name),
            },
            Type::Enum(name) => {
                if is_data_enum(name) {
                    Self::DataEnum {
                        struct_size: struct_size(name),
                    }
                } else {
                    Self::Enum
                }
            }
            Type::Vec(vec_inner) => match vec_inner.as_ref() {
                Type::Primitive(p) => Self::VecPrimitive { primitive: *p },
                Type::Record(name) => Self::VecRecord {
                    struct_size: struct_size(name),
                },
                Type::String => Self::VecString,
                Type::Enum(name) => {
                    if is_data_enum(name) {
                        Self::VecDataEnum {
                            struct_size: struct_size(name),
                        }
                    } else {
                        Self::VecEnum
                    }
                }
                _ => Self::Packed {
                    primitive: Primitive::I32,
                },
            },
            _ => Self::Packed {
                primitive: Primitive::I32,
            },
        }
    }

    fn from_primitive(primitive: Primitive) -> Self {
        match primitive {
            Primitive::Bool
            | Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::F32 => Self::Packed { primitive },
            Primitive::I64
            | Primitive::U64
            | Primitive::F64
            | Primitive::Isize
            | Primitive::Usize => Self::Primitive { primitive },
        }
    }

    pub fn is_packed(&self) -> bool {
        matches!(self, Self::Packed { .. })
    }

    pub fn is_large_primitive(&self) -> bool {
        matches!(self, Self::Primitive { .. })
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String)
    }

    pub fn is_record(&self) -> bool {
        matches!(self, Self::Record { .. })
    }

    pub fn is_enum(&self) -> bool {
        matches!(self, Self::Enum)
    }

    pub fn is_data_enum(&self) -> bool {
        matches!(self, Self::DataEnum { .. })
    }

    pub fn is_vec_primitive(&self) -> bool {
        matches!(self, Self::VecPrimitive { .. })
    }

    pub fn is_vec_record(&self) -> bool {
        matches!(self, Self::VecRecord { .. })
    }

    pub fn is_vec_string(&self) -> bool {
        matches!(self, Self::VecString)
    }

    pub fn is_vec_enum(&self) -> bool {
        matches!(self, Self::VecEnum)
    }

    pub fn is_vec_data_enum(&self) -> bool {
        matches!(self, Self::VecDataEnum { .. })
    }

    pub fn is_vec(&self) -> bool {
        matches!(
            self,
            Self::VecPrimitive { .. }
                | Self::VecRecord { .. }
                | Self::VecString
                | Self::VecEnum
                | Self::VecDataEnum { .. }
        )
    }

    pub fn primitive(&self) -> Option<Primitive> {
        match self {
            Self::Packed { primitive } | Self::Primitive { primitive } => Some(*primitive),
            _ => None,
        }
    }

    pub fn struct_size(&self) -> usize {
        match self {
            Self::Record { struct_size }
            | Self::DataEnum { struct_size }
            | Self::VecRecord { struct_size }
            | Self::VecDataEnum { struct_size } => *struct_size,
            _ => 0,
        }
    }

    pub fn vec_primitive(&self) -> Option<Primitive> {
        match self {
            Self::VecPrimitive { primitive } => Some(*primitive),
            _ => None,
        }
    }
}
