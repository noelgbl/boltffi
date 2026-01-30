use riff_ffi_rules::naming::{CreateFn, GlobalSymbol, Name, RegisterFn, VtableField, VtableType};

use crate::ir::contract::PackageInfo;
use crate::ir::definitions::StreamMode;
use crate::ir::ids::{
    CallbackId, ClassId, EnumId, FieldName, FunctionId, MethodId, ParamName, RecordId, StreamId,
    VariantName,
};
use crate::ir::ops::{ReadSeq, WriteSeq};
use crate::ir::plan::{AbiType, CallbackStyle, Mutability};
use crate::ir::types::TypeExpr;

/// The resolved FFI boundary for the whole crate.
///
/// Each function and method is an [`AbiCall`] with a concrete parameter strategy
/// (wire-encoded buffer vs direct primitive), read/write op sequences for its
/// return type, and for async methods, the polling and completion setup. Backends
/// must read this and transform ops into syntax.
#[derive(Debug, Clone)]
pub struct AbiContract {
    pub package: PackageInfo,
    pub calls: Vec<AbiCall>,
    pub callbacks: Vec<AbiCallbackInvocation>,
    pub streams: Vec<AbiStream>,
    pub records: Vec<AbiRecord>,
    pub enums: Vec<AbiEnum>,
    pub free_buf: Name<GlobalSymbol>,
    pub atomic_cas: Name<GlobalSymbol>,
}

#[derive(Debug, Clone)]
pub struct AbiRecord {
    pub id: RecordId,
    pub decode_ops: ReadSeq,
    pub encode_ops: WriteSeq,
    pub is_blittable: bool,
    pub size: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct AbiEnum {
    pub id: EnumId,
    pub decode_ops: ReadSeq,
    pub encode_ops: WriteSeq,
    pub is_c_style: bool,
    pub variants: Vec<AbiEnumVariant>,
}

#[derive(Debug, Clone)]
pub struct AbiEnumVariant {
    pub name: VariantName,
    pub discriminant: i64,
    pub payload: AbiEnumPayload,
}

#[derive(Debug, Clone)]
pub enum AbiEnumPayload {
    Unit,
    Tuple(Vec<AbiEnumField>),
    Struct(Vec<AbiEnumField>),
}

#[derive(Debug, Clone)]
pub struct AbiEnumField {
    pub name: FieldName,
    pub type_expr: TypeExpr,
    pub decode: ReadSeq,
    pub encode: WriteSeq,
}

#[derive(Debug, Clone)]
pub enum StreamItemTransport {
    WireEncoded { decode_ops: ReadSeq },
}

#[derive(Debug, Clone)]
pub struct AbiStream {
    pub class_id: ClassId,
    pub stream_id: StreamId,
    pub mode: StreamMode,
    pub item: StreamItemTransport,
    pub subscribe: Name<GlobalSymbol>,
    pub poll: Name<GlobalSymbol>,
    pub pop_batch: Name<GlobalSymbol>,
    pub wait: Name<GlobalSymbol>,
    pub unsubscribe: Name<GlobalSymbol>,
    pub free: Name<GlobalSymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallId {
    Function(FunctionId),
    Method {
        class_id: ClassId,
        method_id: MethodId,
    },
    Constructor {
        class_id: ClassId,
        index: usize,
    },
}

#[derive(Debug, Clone)]
pub struct AbiCall {
    pub id: CallId,
    pub symbol: Name<GlobalSymbol>,
    pub mode: CallMode,
    pub params: Vec<AbiParam>,
    pub return_: ReturnTransport,
    pub error: ErrorTransport,
}

#[derive(Debug, Clone)]
pub enum CallMode {
    Sync,
    Async(Box<AsyncCall>),
}

#[derive(Debug, Clone)]
pub struct AsyncCall {
    pub poll: Name<GlobalSymbol>,
    pub complete: Name<GlobalSymbol>,
    pub cancel: Name<GlobalSymbol>,
    pub free: Name<GlobalSymbol>,
    pub result: AsyncResultTransport,
    pub error: ErrorTransport,
}

#[derive(Debug, Clone)]
pub enum AsyncResultTransport {
    Void,
    Direct(AbiType),
    Encoded {
        decode_ops: ReadSeq,
        encode_ops: WriteSeq,
    },
    Handle {
        class_id: ClassId,
        nullable: bool,
    },
    Callback {
        callback_id: CallbackId,
        nullable: bool,
    },
}

#[derive(Debug, Clone)]
pub struct AbiParam {
    pub name: ParamName,
    pub ffi_type: AbiType,
    pub role: ParamRole,
}

#[derive(Debug, Clone)]
pub enum ParamRole {
    InDirect,
    InBuffer {
        len_param: ParamName,
        mutability: Mutability,
        element_abi: AbiType,
    },
    InString {
        len_param: ParamName,
    },
    InEncoded {
        len_param: ParamName,
        decode_ops: ReadSeq,
        encode_ops: WriteSeq,
    },
    InHandle {
        class_id: ClassId,
        nullable: bool,
    },
    InCallback {
        callback_id: CallbackId,
        nullable: bool,
        style: CallbackStyle,
    },
    OutDirect,
    OutBuffer {
        len_param: ParamName,
        decode_ops: ReadSeq,
    },
    SyntheticLen {
        for_param: ParamName,
    },
    OutLen {
        for_param: ParamName,
    },
    StatusOut,
}

#[derive(Debug, Clone)]
pub enum ReturnTransport {
    Void,
    Direct(AbiType),
    Encoded {
        decode_ops: ReadSeq,
        encode_ops: WriteSeq,
    },
    Handle {
        class_id: ClassId,
        nullable: bool,
    },
    Callback {
        callback_id: CallbackId,
        nullable: bool,
    },
}

#[derive(Debug, Clone)]
pub enum ErrorTransport {
    None,
    StatusCode,
    Encoded { decode_ops: ReadSeq },
}

#[derive(Debug, Clone)]
pub struct AbiCallbackInvocation {
    pub callback_id: CallbackId,
    pub vtable_type: Name<VtableType>,
    pub register_fn: Name<RegisterFn>,
    pub create_fn: Name<CreateFn>,
    pub methods: Vec<AbiCallbackMethod>,
}

#[derive(Debug, Clone)]
pub struct AbiCallbackMethod {
    pub id: MethodId,
    pub vtable_field: Name<VtableField>,
    pub is_async: bool,
    pub params: Vec<AbiParam>,
    pub return_: ReturnTransport,
    pub error: ErrorTransport,
}
