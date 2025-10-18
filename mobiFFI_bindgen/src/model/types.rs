use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Primitive {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    Usize,
    Isize,
}

impl Primitive {
    pub fn c_type_name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::I8 => "int8_t",
            Self::U8 => "uint8_t",
            Self::I16 => "int16_t",
            Self::U16 => "uint16_t",
            Self::I32 => "int32_t",
            Self::U32 => "uint32_t",
            Self::I64 => "int64_t",
            Self::U64 => "uint64_t",
            Self::F32 => "float",
            Self::F64 => "double",
            Self::Usize => "uintptr_t",
            Self::Isize => "intptr_t",
        }
    }

    pub fn is_integer(self) -> bool {
        !matches!(self, Self::F32 | Self::F64 | Self::Bool)
    }

    pub fn is_floating_point(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub fn is_signed(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::Isize)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Type {
    Primitive(Primitive),
    String,
    Bytes,
    Vec(Box<Type>),
    Option(Box<Type>),
    Result { ok: Box<Type>, err: Box<Type> },
    Callback(Box<Type>),
    Object(String),
    Record(String),
    Enum(String),
    Void,
}

impl Type {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self, Self::Primitive(_))
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, Self::Option(_))
    }

    pub fn is_result(&self) -> bool {
        matches!(self, Self::Result { .. })
    }

    pub fn inner_type(&self) -> Option<&Type> {
        match self {
            Self::Vec(inner) | Self::Option(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn result_types(&self) -> Option<(&Type, &Type)> {
        match self {
            Self::Result { ok, err } => Some((ok, err)),
            _ => None,
        }
    }

    pub fn named_type(&self) -> Option<&str> {
        match self {
            Self::Object(name) | Self::Record(name) | Self::Enum(name) => Some(name),
            _ => None,
        }
    }

    pub fn vec(element: Type) -> Self {
        Self::Vec(Box::new(element))
    }

    pub fn option(inner: Type) -> Self {
        Self::Option(Box::new(inner))
    }

    pub fn result(ok: Type, err: Type) -> Self {
        Self::Result {
            ok: Box::new(ok),
            err: Box::new(err),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Receiver {
    None,
    Ref,
    RefMut,
}

impl Receiver {
    pub fn is_static(self) -> bool {
        matches!(self, Self::None)
    }

    pub fn is_mutable(self) -> bool {
        matches!(self, Self::RefMut)
    }

    pub fn takes_self(self) -> bool {
        !self.is_static()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Deprecation {
    pub message: Option<String>,
    pub since: Option<String>,
}

impl Deprecation {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: Some(message.into()),
            since: None,
        }
    }

    pub fn with_since(mut self, version: impl Into<String>) -> Self {
        self.since = Some(version.into());
        self
    }
}
