use riff_ffi_rules::naming::{GlobalSymbol, Name, VtableField};

use crate::ir::codec::CodecPlan;
use crate::ir::ids::{CallbackId, ClassId, ParamName};

#[derive(Debug, Clone)]
pub struct CallPlan {
    pub target: CallTarget,
    pub params: Vec<ParamPlan>,
    pub kind: CallPlanKind,
}

#[derive(Debug, Clone)]
pub enum CallTarget {
    GlobalSymbol(Name<GlobalSymbol>),
    VtableField(Name<VtableField>),
}

#[derive(Debug, Clone)]
pub enum CallPlanKind {
    Sync { returns: ReturnPlan },
    Async { async_plan: AsyncPlan },
}

#[derive(Debug, Clone)]
pub struct AsyncPlan {
    pub completion_callback: CompletionCallback,
    pub result: AsyncResult,
}

#[derive(Debug, Clone)]
pub enum AsyncResult {
    Void,
    Value(ReturnValuePlan),
    Fallible {
        ok: ReturnValuePlan,
        err_codec: CodecPlan,
    },
}

#[derive(Debug, Clone)]
pub struct CompletionCallback {
    pub param_name: ParamName,
    pub ffi_type: AbiType,
}

#[derive(Debug, Clone)]
pub struct ParamPlan {
    pub name: ParamName,
    pub strategy: ParamStrategy,
}

#[derive(Debug, Clone)]
pub enum ParamStrategy {
    Direct(DirectPlan),
    Buffer {
        element_abi: AbiType,
        mutability: Mutability,
    },
    String {
        mutability: Mutability,
    },
    Encoded {
        codec: CodecPlan,
        mutability: Mutability,
    },
    Handle {
        class_id: ClassId,
        nullable: bool,
    },
    Callback {
        callback_id: CallbackId,
        style: CallbackStyle,
        nullable: bool,
    },
}

#[derive(Debug, Clone)]
pub struct DirectPlan {
    pub abi_type: AbiType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiType {
    Void,
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    ISize,
    USize,
    F32,
    F64,
    Pointer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutability {
    Shared,
    Mutable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackStyle {
    ImplTrait,
    BoxedDyn,
}

#[derive(Debug, Clone)]
pub enum ReturnValuePlan {
    Void,
    Direct(DirectPlan),
    Encoded {
        codec: CodecPlan,
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
pub enum ReturnPlan {
    Value(ReturnValuePlan),
    Fallible {
        ok: ReturnValuePlan,
        err_codec: CodecPlan,
    },
}
