use crate::ir::ids::CallbackId;

#[derive(Clone)]
pub struct JniModule {
    pub prefix: String,
    pub jni_prefix: String,
    pub package_path: String,
    pub module_name: String,
    pub class_name: String,
    pub has_async: bool,
    pub has_async_callbacks: bool,
    pub functions: Vec<JniFunction>,
    pub wire_functions: Vec<JniWireFunction>,
    pub async_functions: Vec<JniAsyncFunction>,
    pub classes: Vec<JniClass>,
    pub callback_traits: Vec<JniCallbackTrait>,
    pub async_callback_invokers: Vec<JniAsyncCallbackInvoker>,
    pub closure_trampolines: Vec<JniClosureTrampoline>,
}

#[derive(Clone)]
pub struct JniFunction {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_return: String,
    pub jni_params: String,
    pub return_kind: JniReturnKind,
    pub params: Vec<JniParam>,
    pub is_vec: bool,
    pub is_vec_record: bool,
    pub is_data_enum_return: bool,
    pub data_enum_return_name: String,
    pub data_enum_return_size: usize,
    pub vec_buf_type: String,
    pub vec_free_fn: String,
    pub vec_c_type: String,
    pub vec_jni_array_type: String,
    pub vec_new_array_fn: String,
    pub vec_set_array_fn: String,
    pub vec_struct_size: usize,
    pub option_vec_buf_type: String,
    pub option_vec_free_fn: String,
    pub option_vec_c_type: String,
    pub option_vec_jni_array_type: String,
    pub option_vec_new_array_fn: String,
    pub option_vec_struct_size: usize,
    pub option: Option<JniOptionView>,
    pub result: Option<JniResultView>,
}

#[derive(Clone)]
pub struct JniClass {
    pub ffi_prefix: String,
    pub jni_ffi_prefix: String,
    pub jni_prefix: String,
    pub ctors: Vec<JniWireCtor>,
    pub wire_methods: Vec<JniWireMethod>,
    pub async_methods: Vec<JniAsyncFunction>,
}

#[derive(Clone)]
pub struct JniAsyncFunction {
    pub ffi_name: String,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub jni_create_name: String,
    pub jni_params: String,
    pub jni_poll_name: String,
    pub jni_complete_name: String,
    pub jni_cancel_name: String,
    pub jni_free_name: String,
    pub jni_complete_return: String,
    pub jni_complete_c_type: String,
    pub complete_is_void: bool,
    pub complete_is_string: bool,
    pub complete_is_vec: bool,
    pub complete_is_record: bool,
    pub complete_is_result: bool,
    pub result_ok_is_void: bool,
    pub result_ok_is_string: bool,
    pub result_err_is_string: bool,
    pub result_ok_c_type: String,
    pub result_ok_jni_type: String,
    pub vec_jni_element_type: String,
    pub vec_jni_array_type: String,
    pub vec_new_array_fn: String,
    pub vec_set_array_fn: String,
    pub params: Vec<JniParam>,
}

#[derive(Clone)]
pub struct JniCallbackTrait {
    pub trait_name: String,
    pub vtable_type: String,
    pub register_fn: String,
    pub callbacks_class: String,
    pub sync_methods: Vec<JniCallbackMethod>,
    pub async_methods: Vec<JniAsyncCallbackMethod>,
}

#[derive(Clone)]
pub struct JniAsyncCallbackMethod {
    pub ffi_name: String,
    pub jni_method_name: String,
    pub jni_signature: String,
    pub c_params: Vec<JniCallbackCParam>,
    pub setup_lines: Vec<String>,
    pub jni_args: Vec<String>,
    pub has_return: bool,
    pub return_c_type: String,
    pub invoker_jni_name: String,
    pub invoker_native_name: String,
}

#[derive(Clone)]
pub struct JniCallbackMethod {
    pub ffi_name: String,
    pub jni_method_name: String,
    pub jni_signature: String,
    pub jni_return_type: String,
    pub jni_call_type: String,
    pub c_return_type: String,
    pub has_return: bool,
    pub c_params: Vec<JniCallbackCParam>,
    pub setup_lines: Vec<String>,
    pub jni_args: Vec<String>,
}

#[derive(Clone)]
pub struct JniCallbackCParam {
    pub name: String,
    pub c_type: String,
}

#[derive(Clone)]
pub struct JniParam {
    pub name: String,
    pub ffi_arg: String,
    pub jni_decl: String,
    pub is_string: bool,
    pub is_primitive_array: bool,
    pub is_wire_param: bool,
    pub is_data_enum: bool,
    pub is_record_buffer: bool,
    pub is_closure: bool,
    pub record_struct_size: usize,
    pub array_c_type: String,
    pub array_release_mode: String,
}

impl JniParam {
    pub fn jni_param_decl(&self) -> &str {
        &self.jni_decl
    }

    pub fn ffi_arg(&self) -> &str {
        &self.ffi_arg
    }

    pub fn is_string(&self) -> bool {
        self.is_string
    }

    pub fn is_primitive_array(&self) -> bool {
        self.is_primitive_array
    }

    pub fn is_wire_param(&self) -> bool {
        self.is_wire_param
    }

    pub fn is_data_enum(&self) -> bool {
        self.is_data_enum
    }

    pub fn is_record_buffer(&self) -> bool {
        self.is_record_buffer
    }

    pub fn is_closure(&self) -> bool {
        self.is_closure
    }

    pub fn record_struct_size(&self) -> usize {
        self.record_struct_size
    }

    pub fn array_c_type(&self) -> &str {
        &self.array_c_type
    }

    pub fn array_release_mode(&self) -> &str {
        &self.array_release_mode
    }
}

#[derive(Clone)]
pub struct JniClosureTrampoline {
    pub trampoline_name: String,
    pub signature_id: String,
    pub c_params: String,
    pub jni_signature: String,
    pub jni_call_args: String,
    pub invoke_method_name: String,
    pub record_params: Vec<JniClosureRecordParam>,
}

#[derive(Clone)]
pub struct JniClosureRecordParam {
    pub index: usize,
    pub c_type: String,
    pub size: String,
}

#[derive(Clone)]
pub struct JniAsyncCallbackInvoker {
    pub suffix: String,
    pub jni_fn_name: String,
    pub c_result_type: String,
    pub jni_result_type: String,
    pub has_result: bool,
}

#[derive(Clone)]
pub struct JniOptionView {
    pub ffi_type: String,
    pub struct_size: usize,
    pub is_vec: bool,
    pub is_data_enum: bool,
    pub inner_kind: JniOptionInnerKind,
}

#[derive(Clone)]
pub enum JniOptionInnerKind {
    Primitive32,
    PrimitiveLarge,
    String,
    Record,
    Enum,
    VecPrimitive,
    VecRecord,
    VecString,
    VecEnum,
}

impl JniOptionView {
    pub fn is_vec_record(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::VecRecord)
    }

    pub fn is_vec_primitive(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::VecPrimitive)
    }

    pub fn is_vec_string(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::VecString)
    }

    pub fn is_vec_enum(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::VecEnum)
    }

    pub fn is_packed(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::Primitive32)
    }

    pub fn is_large_primitive(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::PrimitiveLarge)
    }

    pub fn is_string(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::String)
    }

    pub fn is_record(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::Record)
    }

    pub fn is_enum(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::Enum)
    }

    pub fn is_data_enum(&self) -> bool {
        self.is_data_enum
    }

    pub fn box_class(&self) -> String {
        match self.inner_kind {
            JniOptionInnerKind::Primitive32 => "java/lang/Integer".to_string(),
            JniOptionInnerKind::PrimitiveLarge => "java/lang/Long".to_string(),
            _ => "java/lang/Object".to_string(),
        }
    }

    pub fn box_signature(&self) -> String {
        match self.inner_kind {
            JniOptionInnerKind::Primitive32 => "(I)Ljava/lang/Integer;".to_string(),
            JniOptionInnerKind::PrimitiveLarge => "(J)Ljava/lang/Long;".to_string(),
            _ => "()Ljava/lang/Object;".to_string(),
        }
    }

    pub fn box_jni_type(&self) -> String {
        match self.inner_kind {
            JniOptionInnerKind::Primitive32 => "jint".to_string(),
            JniOptionInnerKind::PrimitiveLarge => "jlong".to_string(),
            _ => "jobject".to_string(),
        }
    }
}

#[derive(Clone)]
pub struct JniResultView {
    pub ok: JniResultVariant,
    pub err: JniResultVariant,
}

impl JniResultView {
    pub fn is_void(&self) -> bool {
        matches!(self.ok, JniResultVariant::Void)
    }

    pub fn is_string(&self) -> bool {
        matches!(self.ok, JniResultVariant::String)
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self.ok, JniResultVariant::Primitive { .. })
    }

    pub fn primitive_c_type(&self) -> String {
        match &self.ok {
            JniResultVariant::Primitive { c_type, .. } => c_type.clone(),
            _ => String::new(),
        }
    }

    pub fn is_record(&self) -> bool {
        matches!(self.ok, JniResultVariant::Record { .. })
    }

    pub fn record_struct_size(&self) -> usize {
        match &self.ok {
            JniResultVariant::Record { struct_size, .. } => *struct_size,
            _ => 0,
        }
    }

    pub fn is_enum(&self) -> bool {
        matches!(self.ok, JniResultVariant::Enum { .. })
    }

    pub fn is_data_enum(&self) -> bool {
        matches!(self.ok, JniResultVariant::DataEnum { .. })
    }

    pub fn data_enum_struct_size(&self) -> usize {
        match &self.ok {
            JniResultVariant::DataEnum { struct_size, .. } => *struct_size,
            _ => 0,
        }
    }

    pub fn err_is_ffi_error(&self) -> bool {
        matches!(self.err, JniResultVariant::String)
    }

    pub fn err_struct_size(&self) -> usize {
        match &self.err {
            JniResultVariant::DataEnum { struct_size, .. } => *struct_size,
            JniResultVariant::String => 24,
            _ => 0,
        }
    }

    pub fn is_vec_primitive(&self) -> bool {
        matches!(self.ok, JniResultVariant::VecPrimitive { .. })
    }

    pub fn is_vec_record(&self) -> bool {
        matches!(self.ok, JniResultVariant::VecRecord { .. })
    }

    pub fn vec_primitive(&self) -> Option<&JniVecPrimitive> {
        match &self.ok {
            JniResultVariant::VecPrimitive { info, .. } => Some(info),
            _ => None,
        }
    }

    pub fn vec_record_struct_size(&self) -> usize {
        match &self.ok {
            JniResultVariant::VecRecord { struct_size, .. } => *struct_size,
            _ => 0,
        }
    }

    pub fn vec_len_fn(&self) -> String {
        match &self.ok {
            JniResultVariant::VecPrimitive { len_fn, .. } => len_fn.clone(),
            JniResultVariant::VecRecord { len_fn, .. } => len_fn.clone(),
            _ => String::new(),
        }
    }

    pub fn vec_copy_fn(&self) -> String {
        match &self.ok {
            JniResultVariant::VecPrimitive { copy_fn, .. } => copy_fn.clone(),
            JniResultVariant::VecRecord { copy_fn, .. } => copy_fn.clone(),
            _ => String::new(),
        }
    }

    pub fn ok_is_void(&self) -> bool {
        matches!(self.ok, JniResultVariant::Void)
    }

    pub fn ok_is_string(&self) -> bool {
        matches!(self.ok, JniResultVariant::String)
    }

    pub fn err_is_string(&self) -> bool {
        matches!(self.err, JniResultVariant::String)
    }

    pub fn ok_c_type(&self) -> String {
        match &self.ok {
            JniResultVariant::Primitive { c_type, .. } => c_type.clone(),
            JniResultVariant::Record { c_type, .. } => c_type.clone(),
            _ => String::new(),
        }
    }

    pub fn ok_jni_type(&self) -> String {
        match &self.ok {
            JniResultVariant::Primitive { jni_type, .. } => jni_type.clone(),
            JniResultVariant::Record { jni_type, .. } => jni_type.clone(),
            JniResultVariant::Enum { jni_type } => jni_type.clone(),
            JniResultVariant::DataEnum { jni_type, .. } => jni_type.clone(),
            _ => String::new(),
        }
    }
}

#[derive(Clone)]
pub enum JniReturnKind {
    Void,
    Primitive {
        jni_type: String,
    },
    String {
        ffi_name: String,
    },
    Vec {
        len_fn: String,
        copy_fn: String,
    },
    CStyleEnum,
    DataEnum {
        enum_name: String,
        struct_size: usize,
    },
    Option(JniOptionView),
    Result(JniResultView),
}

impl JniReturnKind {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String { .. })
    }

    pub fn is_vec(&self) -> bool {
        matches!(self, Self::Vec { .. })
    }

    pub fn is_c_style_enum(&self) -> bool {
        matches!(self, Self::CStyleEnum)
    }

    pub fn is_data_enum(&self) -> bool {
        matches!(self, Self::DataEnum { .. })
    }

    pub fn is_option(&self) -> bool {
        matches!(self, Self::Option(_))
    }

    pub fn is_result(&self) -> bool {
        matches!(self, Self::Result(_))
    }
}

#[derive(Clone)]
pub enum JniResultVariant {
    Void,
    Primitive {
        c_type: String,
        jni_type: String,
    },
    String,
    Record {
        c_type: String,
        jni_type: String,
        struct_size: usize,
    },
    Enum {
        jni_type: String,
    },
    DataEnum {
        jni_type: String,
        struct_size: usize,
    },
    VecPrimitive {
        info: JniVecPrimitive,
        len_fn: String,
        copy_fn: String,
    },
    VecRecord {
        len_fn: String,
        copy_fn: String,
        struct_size: usize,
    },
}

#[derive(Clone)]
pub struct JniVecPrimitive {
    pub c_type_name: String,
    pub jni_array_type: String,
}

impl JniVecPrimitive {
    pub fn c_type_name(&self) -> &str {
        &self.c_type_name
    }

    pub fn jni_array_type(&self) -> &str {
        &self.jni_array_type
    }
}

#[derive(Clone)]
pub struct JniWireFunction {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_params: String,
    pub params: Vec<JniParam>,
    pub return_abi: JniReturnAbi,
}

#[derive(Clone)]
pub struct JniWireMethod {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_params: String,
    pub params: Vec<JniParam>,
    pub return_abi: JniReturnAbi,
    pub include_handle: bool,
}

#[derive(Clone)]
pub struct JniWireCtor {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_params: String,
    pub params: Vec<JniParam>,
}

#[derive(Clone)]
pub enum JniReturnAbi {
    Unit,
    Direct {
        jni_return_type: String,
        jni_c_return_type: String,
        jni_result_cast: String,
    },
    WireEncoded,
}

impl JniReturnAbi {
    pub fn is_unit(&self) -> bool {
        matches!(self, Self::Unit)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct { .. })
    }

    pub fn is_wire_encoded(&self) -> bool {
        matches!(self, Self::WireEncoded)
    }

    pub fn jni_return_type(&self) -> String {
        match self {
            Self::Unit => "void".to_string(),
            Self::Direct {
                jni_return_type, ..
            } => jni_return_type.clone(),
            Self::WireEncoded => "jobject".to_string(),
        }
    }

    pub fn jni_c_return_type(&self) -> String {
        match self {
            Self::Direct {
                jni_c_return_type, ..
            } => jni_c_return_type.clone(),
            _ => String::new(),
        }
    }

    pub fn jni_result_cast(&self) -> String {
        match self {
            Self::Direct {
                jni_result_cast, ..
            } => jni_result_cast.clone(),
            _ => String::new(),
        }
    }
}

#[derive(Clone)]
pub struct JniClosureInfo {
    pub signature_id: String,
    pub callback_id: CallbackId,
}
