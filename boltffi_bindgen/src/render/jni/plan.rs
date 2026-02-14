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
}

#[derive(Clone)]
pub struct JniClass {
    pub ffi_prefix: String,
    pub jni_ffi_prefix: String,
    pub jni_prefix: String,
    pub ctors: Vec<JniWireCtor>,
    pub wire_methods: Vec<JniWireMethod>,
    pub async_methods: Vec<JniAsyncFunction>,
    pub streams: Vec<JniStream>,
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
    pub complete_kind: JniAsyncCompleteKind,
    pub params: Vec<JniParam>,
}

#[derive(Clone)]
pub enum JniAsyncCompleteKind {
    Void,
    WireEncoded,
    Direct { jni_return: String, c_type: String },
}

impl JniAsyncCompleteKind {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_wire_encoded(&self) -> bool {
        matches!(self, Self::WireEncoded)
    }

    pub fn jni_return(&self) -> &str {
        match self {
            Self::Void => "void",
            Self::WireEncoded => "jbyteArray",
            Self::Direct { jni_return, .. } => jni_return,
        }
    }

    pub fn c_type(&self) -> &str {
        match self {
            Self::Direct { c_type, .. } => c_type,
            _ => "",
        }
    }
}

#[derive(Clone)]
pub struct JniStream {
    pub subscribe_ffi: String,
    pub subscribe_jni: String,
    pub poll_ffi: String,
    pub poll_jni: String,
    pub pop_batch_ffi: String,
    pub pop_batch_jni: String,
    pub wait_ffi: String,
    pub wait_jni: String,
    pub unsubscribe_ffi: String,
    pub unsubscribe_jni: String,
    pub free_ffi: String,
    pub free_jni: String,
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
    pub return_c_type: Option<String>,
    pub invoker_jni_name: String,
    pub invoker_native_name: String,
}

impl JniAsyncCallbackMethod {
    pub fn has_return(&self) -> bool {
        self.return_c_type.is_some()
    }

    pub fn is_wire(&self) -> bool {
        self.return_c_type.as_deref() == Some("wire")
    }

    pub fn return_c_type(&self) -> &str {
        self.return_c_type.as_deref().unwrap_or("")
    }
}

#[derive(Clone)]
pub struct JniCallbackMethod {
    pub ffi_name: String,
    pub jni_method_name: String,
    pub jni_signature: String,
    pub c_params: Vec<JniCallbackCParam>,
    pub setup_lines: Vec<String>,
    pub jni_args: Vec<String>,
    pub return_info: Option<JniCallbackReturn>,
}

#[derive(Clone)]
pub struct JniCallbackReturn {
    pub jni_type: String,
    pub jni_call_type: String,
    pub c_type: String,
}

impl JniCallbackMethod {
    pub fn has_return(&self) -> bool {
        self.return_info.is_some()
    }

    pub fn jni_return_type(&self) -> &str {
        self.return_info
            .as_ref()
            .map(|r| r.jni_type.as_str())
            .unwrap_or("")
    }

    pub fn jni_call_type(&self) -> &str {
        self.return_info
            .as_ref()
            .map(|r| r.jni_call_type.as_str())
            .unwrap_or("")
    }

    pub fn c_return_type(&self) -> &str {
        self.return_info
            .as_ref()
            .map(|r| r.c_type.as_str())
            .unwrap_or("")
    }
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
    pub kind: JniParamKind,
}

#[derive(Clone)]
pub enum JniParamKind {
    Primitive,
    String,
    PrimitiveArray {
        c_type: String,
        release_mode: JniArrayReleaseMode,
    },
    Buffer,
    Closure,
}

#[derive(Clone, Copy)]
pub enum JniArrayReleaseMode {
    Commit,
    Abort,
}

impl JniArrayReleaseMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Commit => "0",
            Self::Abort => "JNI_ABORT",
        }
    }
}

impl JniParam {
    pub fn jni_param_decl(&self) -> &str {
        &self.jni_decl
    }

    pub fn ffi_arg(&self) -> &str {
        &self.ffi_arg
    }

    pub fn is_string(&self) -> bool {
        matches!(self.kind, JniParamKind::String)
    }

    pub fn is_primitive_array(&self) -> bool {
        matches!(self.kind, JniParamKind::PrimitiveArray { .. })
    }

    pub fn is_buffer(&self) -> bool {
        matches!(self.kind, JniParamKind::Buffer)
    }

    pub fn is_closure(&self) -> bool {
        matches!(self.kind, JniParamKind::Closure)
    }

    pub fn array_c_type(&self) -> &str {
        match &self.kind {
            JniParamKind::PrimitiveArray { c_type, .. } => c_type,
            _ => "",
        }
    }

    pub fn array_release_mode(&self) -> &str {
        match &self.kind {
            JniParamKind::PrimitiveArray { release_mode, .. } => release_mode.as_str(),
            _ => "",
        }
    }
}

#[derive(Clone)]
pub struct JniClosureTrampoline {
    pub trampoline_name: String,
    pub signature_id: String,
    pub callbacks_class_jni_path: String,
    pub c_params: String,
    pub jni_params_signature: String,
    pub jni_call_args: String,
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
    pub result_type: Option<JniInvokerResult>,
}

#[derive(Clone)]
pub struct JniInvokerResult {
    pub c_type: String,
    pub jni_type: String,
}

impl JniAsyncCallbackInvoker {
    pub fn has_result(&self) -> bool {
        self.result_type.is_some()
    }

    pub fn is_wire(&self) -> bool {
        self.result_type
            .as_ref()
            .map(|r| r.c_type == "wire")
            .unwrap_or(false)
    }

    pub fn c_result_type(&self) -> &str {
        self.result_type
            .as_ref()
            .map(|r| r.c_type.as_str())
            .unwrap_or("")
    }

    pub fn jni_result_type(&self) -> &str {
        self.result_type
            .as_ref()
            .map(|r| r.jni_type.as_str())
            .unwrap_or("")
    }
}

#[derive(Clone)]
pub struct JniOptionView {
    pub ffi_type: String,
    pub struct_size: usize,
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
    pub fn is_vec(&self) -> bool {
        matches!(
            self.inner_kind,
            JniOptionInnerKind::VecPrimitive
                | JniOptionInnerKind::VecRecord
                | JniOptionInnerKind::VecString
                | JniOptionInnerKind::VecEnum
        )
    }

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
            Self::WireEncoded => "jbyteArray".to_string(),
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
