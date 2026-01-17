use super::names::NamingConvention;
use super::types::TypeMapper;
use crate::model::{ReturnType, Type};

#[derive(Debug, Clone, Default)]
pub struct ReturnInfo {
    pub swift_type: Option<String>,
    pub is_void: bool,
    pub is_result: bool,
    pub result_ok_type: Option<String>,
}

impl ReturnInfo {
    pub fn from_return_type(returns: &ReturnType) -> Self {
        match returns {
            ReturnType::Void => Self {
                is_void: true,
                ..Default::default()
            },
            ReturnType::Value(ty) => match ty {
                Type::Void => Self {
                    is_void: true,
                    ..Default::default()
                },
                _ => Self {
                    swift_type: Some(TypeMapper::map_type(ty)),
                    ..Default::default()
                },
            },
            ReturnType::Fallible { ok, .. } => {
                let ok_type = match ok {
                    Type::Void => None,
                    _ => Some(TypeMapper::map_type(ok)),
                };
                Self {
                    swift_type: ok_type.clone(),
                    is_result: true,
                    result_ok_type: ok_type,
                    is_void: matches!(ok, Type::Void),
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParamInfo {
    pub swift_name: String,
    pub swift_type: String,
    pub ffi_conversion: String,
    pub is_string: bool,
    pub is_slice: bool,
    pub is_mut_slice: bool,
    pub is_vec: bool,
    pub is_vec_wire_encoded: bool,
    pub is_escaping: bool,
}

impl ParamInfo {
    pub fn from_param(name: &str, ty: &Type) -> Self {
        let swift_name = NamingConvention::param_name(name);
        let swift_type = TypeMapper::map_type(ty);
        let is_string = matches!(ty, Type::String);
        let is_slice = matches!(ty, Type::Slice(_));
        let is_mut_slice = matches!(ty, Type::MutSlice(_));
        let is_vec = matches!(ty, Type::Vec(_));
        let is_vec_wire_encoded =
            matches!(ty, Type::Vec(inner) if !matches!(inner.as_ref(), Type::Primitive(_)));
        let is_escaping = matches!(ty, Type::Closure(_));

        let ffi_conversion = match ty {
            Type::Enum(_) => format!("{}.cValue", swift_name),
            Type::BoxedTrait(trait_name) => {
                format!(
                    "{}Bridge.create({})",
                    NamingConvention::class_name(trait_name),
                    swift_name
                )
            }
            _ => swift_name.clone(),
        };

        Self {
            swift_name,
            swift_type,
            ffi_conversion,
            is_string,
            is_slice,
            is_mut_slice,
            is_vec,
            is_vec_wire_encoded,
            is_escaping,
        }
    }

    pub fn needs_wrapper(&self) -> bool {
        self.is_string || self.is_slice || self.is_mut_slice || self.is_vec
    }
}

#[derive(Debug, Clone)]
pub struct CallbackInfo {
    pub param_name: String,
    pub swift_type: String,
    pub ffi_arg_type: String,
    pub context_type: String,
    pub box_type: String,
    pub box_name: String,
    pub ptr_name: String,
    pub trampoline_name: String,
    pub trampoline_args: String,
    pub trampoline_call_args: String,
}

impl CallbackInfo {
    pub fn from_param(name: &str, ty: &Type, func_name_pascal: &str, index: usize) -> Option<Self> {
        let Type::Closure(sig) = ty else {
            return None;
        };

        let param_name = NamingConvention::param_name(name);
        let suffix = if index > 0 {
            format!("{}", index + 1)
        } else {
            String::new()
        };

        let params_swift = sig
            .params
            .iter()
            .map(|p| TypeMapper::map_type(p))
            .collect::<Vec<_>>()
            .join(", ");

        let params_ffi = sig
            .params
            .iter()
            .map(|p| TypeMapper::ffi_type(p))
            .collect::<Vec<_>>()
            .join(", ");

        let mut arg_names = Vec::new();
        let mut call_conversions = Vec::new();
        let mut arg_idx = 0;

        for param_ty in &sig.params {
            if matches!(param_ty, Type::Record(_)) {
                let ptr_name = format!("ptr{}", arg_idx);
                let len_name = format!("len{}", arg_idx);
                arg_names.push(ptr_name.clone());
                arg_names.push(len_name.clone());
                let record_name = if let Type::Record(n) = param_ty {
                    NamingConvention::class_name(n)
                } else {
                    "Unknown".to_string()
                };
                call_conversions.push(format!(
                    "{}.decode(wireBuffer: WireBuffer(ptr: {}!, len: Int({})), at: 0).value",
                    record_name, ptr_name, len_name
                ));
            } else {
                let val_name = format!("val{}", arg_idx);
                arg_names.push(val_name.clone());
                call_conversions.push(val_name);
            }
            arg_idx += 1;
        }

        let trampoline_args = arg_names.join(", ");
        let trampoline_call_args = call_conversions.join(", ");

        Some(Self {
            param_name: param_name.clone(),
            swift_type: params_swift,
            ffi_arg_type: params_ffi,
            context_type: format!("{}CallbackFn{}", func_name_pascal, suffix),
            box_type: format!("{}CallbackBox{}", func_name_pascal, suffix),
            box_name: format!("{}Box{}", param_name, suffix),
            ptr_name: format!("{}Ptr{}", param_name, suffix),
            trampoline_name: format!("{}Trampoline{}", param_name, suffix),
            trampoline_args,
            trampoline_call_args,
        })
    }
}

pub struct ParamsInfo {
    pub params: Vec<ParamInfo>,
    pub callbacks: Vec<CallbackInfo>,
    pub has_callbacks: bool,
}

impl ParamsInfo {
    pub fn from_inputs<'a>(
        inputs: impl Iterator<Item = (&'a str, &'a Type)>,
        func_name_pascal: &str,
    ) -> Self {
        let mut params = Vec::new();
        let mut callbacks = Vec::new();
        let mut callback_index = 0;

        for (name, ty) in inputs {
            params.push(ParamInfo::from_param(name, ty));

            if matches!(ty, Type::Closure(_)) {
                if let Some(cb) =
                    CallbackInfo::from_param(name, ty, func_name_pascal, callback_index)
                {
                    callbacks.push(cb);
                    callback_index += 1;
                }
            }
        }

        let has_callbacks = !callbacks.is_empty();

        Self {
            params,
            callbacks,
            has_callbacks,
        }
    }
}
