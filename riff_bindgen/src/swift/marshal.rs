use crate::model::{Module, Primitive, ReturnType, Type};

use super::names::NamingConvention;
use super::primitives;
use super::wire;

#[derive(Debug, Clone, PartialEq)]
pub enum SwiftType {
    Void,
    Primitive(Primitive),
    String,
    Bytes,
    Slice {
        inner: Box<SwiftType>,
        mutable: bool,
    },
    Vec(Box<SwiftType>),
    Option(Box<SwiftType>),
    Result {
        ok: Box<SwiftType>,
    },
    Enum(String),
    Record(String),
    Object(String),
    BoxedTrait(String),
    Closure {
        params: Vec<SwiftType>,
        returns: Box<SwiftType>,
    },
}

impl SwiftType {
    pub fn from_model(ty: &Type) -> Self {
        match ty {
            Type::Void => Self::Void,
            Type::Primitive(p) => Self::Primitive(*p),
            Type::String => Self::String,
            Type::Bytes => Self::Bytes,
            Type::Slice(inner) => Self::Slice {
                inner: Box::new(Self::from_model(inner)),
                mutable: false,
            },
            Type::MutSlice(inner) => Self::Slice {
                inner: Box::new(Self::from_model(inner)),
                mutable: true,
            },
            Type::Vec(inner) => Self::Vec(Box::new(Self::from_model(inner))),
            Type::Option(inner) => Self::Option(Box::new(Self::from_model(inner))),
            Type::Result { ok, .. } => Self::Result {
                ok: Box::new(Self::from_model(ok)),
            },
            Type::Enum(name) => Self::Enum(name.clone()),
            Type::Record(name) => Self::Record(name.clone()),
            Type::Object(name) => Self::Object(name.clone()),
            Type::BoxedTrait(name) => Self::BoxedTrait(name.clone()),
            Type::Closure(sig) => Self::Closure {
                params: sig.params.iter().map(|p| Self::from_model(p)).collect(),
                returns: Box::new(Self::from_model(&sig.returns)),
            },
        }
    }

    pub fn swift_type(&self) -> String {
        match self {
            Self::Void => "Void".into(),
            Self::Primitive(p) => primitives::info(*p).swift_type.into(),
            Self::String => "String".into(),
            Self::Bytes => "Data".into(),
            Self::Slice { inner, .. } | Self::Vec(inner) => format!("[{}]", inner.swift_type()),
            Self::Option(inner) => format!("{}?", inner.swift_type()),
            Self::Result { ok } => ok.swift_type(),
            Self::Enum(name) | Self::Record(name) | Self::Object(name) => {
                NamingConvention::class_name(name)
            }
            Self::BoxedTrait(name) => format!("{}Protocol", NamingConvention::class_name(name)),
            Self::Closure { params, returns } => {
                let params_str = params
                    .iter()
                    .map(|p| p.swift_type())
                    .collect::<Vec<_>>()
                    .join(", ");
                let ret = if matches!(returns.as_ref(), SwiftType::Void) {
                    "Void".to_string()
                } else {
                    returns.swift_type()
                };
                format!("({}) -> {}", params_str, ret)
            }
        }
    }

    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self, Self::Primitive(_))
    }
}

#[derive(Debug, Clone)]
pub struct ParamConversion {
    pub wrapper_pre: Option<String>,
    pub wrapper_post: Option<String>,
    pub ffi_args: Vec<String>,
}

impl ParamConversion {
    pub fn from_param(name: &str, ty: &Type, _module: &Module) -> Self {
        let swift_ty = SwiftType::from_model(ty);
        let swift_name = NamingConvention::param_name(name);

        let (wrapper_pre, ffi_args, wrapper_post) = match &swift_ty {
            SwiftType::String => (
                Some(format!("{}.withCString {{ {}Ptr in", swift_name, swift_name)),
                vec![
                    format!("UnsafeRawPointer({}Ptr).assumingMemoryBound(to: UInt8.self)", swift_name),
                    format!("UInt({}.utf8.count)", swift_name),
                ],
                Some("}".into()),
            ),
            SwiftType::Bytes => (
                Some(format!("{}.withUnsafeBytes {{ {}Ptr in", swift_name, swift_name)),
                vec![
                    format!("{}Ptr.baseAddress!.assumingMemoryBound(to: UInt8.self)", swift_name),
                    format!("UInt({}.count)", swift_name),
                ],
                Some("}".into()),
            ),
            SwiftType::Slice { mutable: false, .. } => (
                Some(format!("{}.withUnsafeBufferPointer {{ {}Ptr in", swift_name, swift_name)),
                vec![
                    format!("{}Ptr.baseAddress", swift_name),
                    format!("UInt({}Ptr.count)", swift_name),
                ],
                Some("}".into()),
            ),
            SwiftType::Vec(inner) if inner.is_primitive() => (
                Some(format!("{}.withUnsafeBufferPointer {{ {}Ptr in", swift_name, swift_name)),
                vec![
                    format!("{}Ptr.baseAddress", swift_name),
                    format!("UInt({}Ptr.count)", swift_name),
                ],
                Some("}".into()),
            ),
            SwiftType::Vec(_) => (
                Some(format!("withWireEncodedArray({}) {{ {}Ptr in", swift_name, swift_name)),
                vec![
                    format!("{}Ptr.baseAddress!.assumingMemoryBound(to: UInt8.self)", swift_name),
                    format!("UInt({}Ptr.count)", swift_name),
                ],
                Some("}".into()),
            ),
            SwiftType::Slice { mutable: true, .. } => (
                Some(format!("{}.withUnsafeMutableBufferPointer {{ {}Ptr in", swift_name, swift_name)),
                vec![
                    format!("{}Ptr.baseAddress", swift_name),
                    format!("UInt({}Ptr.count)", swift_name),
                ],
                Some("}".into()),
            ),
            SwiftType::Enum(_) => (
                Some(format!("{}.wireEncode().withUnsafeBytes {{ {}Ptr in", swift_name, swift_name)),
                vec![
                    format!("{}Ptr.baseAddress!.assumingMemoryBound(to: UInt8.self)", swift_name),
                    format!("UInt({}Ptr.count)", swift_name),
                ],
                Some("}".into()),
            ),
            SwiftType::BoxedTrait(trait_name) => (
                None,
                vec![format!("{}Bridge.create({})", NamingConvention::class_name(trait_name), swift_name)],
                None,
            ),
            SwiftType::Record(_) => (
                Some(format!("{}.wireEncode().withUnsafeBytes {{ {}Ptr in", swift_name, swift_name)),
                vec![
                    format!("{}Ptr.baseAddress!.assumingMemoryBound(to: UInt8.self)", swift_name),
                    format!("UInt({}Ptr.count)", swift_name),
                ],
                Some("}".into()),
            ),
            SwiftType::Option(inner)
                if matches!(inner.as_ref(), SwiftType::Record(_) | SwiftType::Enum(_) | SwiftType::Vec(_)) =>
            {
                (
                    Some(format!("withWireEncodedOptional({}) {{ {}Ptr in", swift_name, swift_name)),
                    vec![
                        format!("{}Ptr?.baseAddress?.assumingMemoryBound(to: UInt8.self)", swift_name),
                        format!("UInt({}Ptr?.count ?? 0)", swift_name),
                    ],
                    Some("}".into()),
                )
            }
            _ => (None, vec![swift_name.clone()], None),
        };

        Self { wrapper_pre, wrapper_post, ffi_args }
    }

    pub fn needs_wrapper(&self) -> bool {
        self.wrapper_pre.is_some()
    }
}

pub struct SyncCallBuilder {
    params: Vec<ParamConversion>,
    include_handle: bool,
}

impl SyncCallBuilder {
    pub fn new(include_handle: bool) -> Self {
        Self {
            params: Vec::new(),
            include_handle,
        }
    }

    pub fn with_params<'a>(mut self, params: impl Iterator<Item = (&'a str, &'a Type)>, module: &Module) -> Self {
        self.params = params
            .map(|(n, t)| ParamConversion::from_param(n, t, module))
            .collect();
        self
    }

    pub fn build_wrappers_open(&self) -> String {
        self.params
            .iter()
            .filter_map(|p| p.wrapper_pre.as_ref())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn build_wrappers_close(&self) -> String {
        self.params
            .iter()
            .filter_map(|p| p.wrapper_post.as_ref())
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn build_ffi_args(&self) -> String {
        [if self.include_handle {
            Some("handle")
        } else {
            None
        }]
        .into_iter()
        .flatten()
        .map(String::from)
        .chain(self.params.iter().flat_map(|p| p.ffi_args.clone()))
        .collect::<Vec<_>>()
        .join(", ")
    }
}

#[derive(Debug, Clone)]
pub enum ReturnAbi {
    Unit,
    Direct { swift_type: String, conversion: Option<String> },
    WireEncoded { swift_type: String, decode_expr: String, throws: bool },
}

impl ReturnAbi {
    pub fn from_return_type(returns: &ReturnType, module: &Module) -> Self {
        match returns {
            ReturnType::Void => Self::Unit,
            ReturnType::Value(ty) => Self::from_value_type(ty, module),
            ReturnType::Fallible { ok, err } => Self::from_fallible(ok, err, module),
        }
    }

    fn from_value_type(ty: &Type, module: &Module) -> Self {
        match ty {
            Type::Void => Self::Unit,
            Type::Primitive(_) => Self::Direct {
                swift_type: SwiftType::from_model(ty).swift_type(),
                conversion: None,
            },
            Type::String | Type::Record(_) | Type::Enum(_) | Type::Vec(_) | Type::Option(_) => {
                Self::WireEncoded {
                    swift_type: SwiftType::from_model(ty).swift_type(),
                    decode_expr: wire::decode_value_at_offset(ty, module, "0"),
                    throws: false,
                }
            }
            _ => Self::Direct {
                swift_type: SwiftType::from_model(ty).swift_type(),
                conversion: None,
            },
        }
    }

    fn from_fallible(ok: &Type, err: &Type, module: &Module) -> Self {
        let ok_swift = SwiftType::from_model(ok).swift_type();
        let err_swift = Self::error_type_name(err, module);
        let ok_decode = Self::ok_decode_expr(ok, module);
        
        Self::WireEncoded {
            swift_type: if ok.is_void() { "Void".into() } else { ok_swift },
            decode_expr: format!(
                "try wire.readResultOrThrow(at: 0, ok: {{ {} }}, err: {{ {} }})",
                ok_decode,
                Self::error_decode_expr(err, &err_swift)
            ),
            throws: true,
        }
    }

    fn ok_decode_expr(ty: &Type, module: &Module) -> String {
        match ty {
            Type::Void => "()".into(),
            Type::Primitive(Primitive::Usize) => "UInt(wire.readU64(at: $0))".into(),
            Type::Primitive(Primitive::Isize) => "Int(wire.readI64(at: $0))".into(),
            _ => wire::decode_type(ty, module).value_only(),
        }
    }

    fn error_type_name(err: &Type, module: &Module) -> String {
        match err {
            Type::String => "FfiError".into(),
            Type::Enum(name) => {
                if module.enums.iter().any(|e| &e.name == name && e.is_error) {
                    NamingConvention::class_name(name)
                } else {
                    "FfiError".into()
                }
            }
            _ => "FfiError".into(),
        }
    }

    fn error_decode_expr(err: &Type, err_swift: &str) -> String {
        match err {
            Type::String => "FfiError(message: wire.readString(at: $0).value)".into(),
            Type::Enum(_) => format!("{}.decode(wireBuffer: wire, at: $0).value", err_swift),
            _ => "FfiError(message: \"unknown error\")".into(),
        }
    }

    pub fn is_unit(&self) -> bool {
        matches!(self, Self::Unit)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct { .. })
    }

    pub fn is_wire_encoded(&self) -> bool {
        matches!(self, Self::WireEncoded { .. })
    }

    pub fn throws(&self) -> bool {
        matches!(self, Self::WireEncoded { throws: true, .. })
    }

    pub fn decode_expr(&self) -> &str {
        match self {
            Self::WireEncoded { decode_expr, .. } => decode_expr,
            _ => "",
        }
    }

    pub fn direct_call_expr(&self, ffi_call: &str) -> String {
        match self {
            Self::Direct { conversion: Some(conv), .. } => conv.replace("$0", ffi_call),
            Self::Direct { conversion: None, .. } => ffi_call.to_string(),
            _ => ffi_call.to_string(),
        }
    }
}
