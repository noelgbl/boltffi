use riff_ffi_rules::naming;

use crate::model::{
    CallContract, Class, Method, Module, Parameter, ParamTransport, PassThroughType, ReturnType,
    Type,
};

use super::marshal::ParamConversion;
use super::return_abi::ReturnAbi;
use super::{NamingConvention, TypeMapper, wire};

#[derive(Debug, Clone)]
pub struct ParamSpec {
    pub name: String,
    pub kotlin_type: String,
    pub conversion: String,
}

#[derive(Debug, Clone)]
pub struct SignatureParamSpec {
    pub name: String,
    pub kotlin_type: String,
}

#[derive(Debug, Clone)]
pub struct WireWriterBinding {
    pub binding_name: String,
    pub size_expr: String,
    pub encode_expr: String,
}

#[derive(Debug, Clone)]
pub struct WireFunctionPlan {
    pub func_name: String,
    pub ffi_name: String,
    pub signature_params: Vec<SignatureParamSpec>,
    pub wire_writers: Vec<WireWriterBinding>,
    pub wire_writer_closes: Vec<String>,
    pub native_args: Vec<String>,
    pub return_type: Option<String>,
    pub return_abi: ReturnAbi,
    pub decode_expr: String,
    pub throws: bool,
    pub err_type: String,
    pub is_blittable_return: bool,
}

impl WireFunctionPlan {
    pub fn supports_call(inputs: &[Parameter], returns: &ReturnType, module: &Module) -> bool {
        let params_ok = inputs
            .iter()
            .map(|param| &param.param_type)
            .all(|param_type| Self::supports_param_type(param_type, module));
        params_ok && Self::supports_return_type(returns, module)
    }

    pub fn for_function(
        function_name: &str,
        inputs: &[Parameter],
        returns: &ReturnType,
        module: &Module,
    ) -> Self {
        let contract = CallContract::for_function(inputs, returns, module);
        let ffi_name = naming::function_ffi_name(function_name);
        let return_abi = ReturnAbi::from_return_type(returns, module);

        let signature_params = inputs
            .iter()
            .map(|param| {
                let param_name = NamingConvention::param_name(&param.name);
                let kotlin_type = TypeMapper::map_type(&param.param_type);
                SignatureParamSpec {
                    name: param_name,
                    kotlin_type,
                }
            })
            .collect();

        let (wire_writers, native_args) = inputs
            .iter()
            .zip(contract.params.iter())
            .map(|(param, param_contract)| {
                let param_name = NamingConvention::param_name(&param.name);
                match &param_contract.transport {
                    ParamTransport::PassThrough(_) => (
                        None,
                        ParamConversion::to_ffi(&param_name, &param.param_type, module),
                    ),
                    ParamTransport::WireEncoded(_) => {
                        let encoder = wire::encode_type(&param.param_type, &param_name, module);
                        let binding_name = format!("wire_writer_{}", param_name);
                        (
                            Some(WireWriterBinding {
                                binding_name: binding_name.clone(),
                                size_expr: encoder.size_expr,
                                encode_expr: encoder.encode_expr,
                            }),
                            format!("{}.buffer", binding_name),
                        )
                    }
                }
            })
            .fold(
                (Vec::new(), Vec::new()),
                |(mut wire_writers, mut native_args), (maybe_wire_writer, native_arg)| {
                    if let Some(wire_writer) = maybe_wire_writer {
                        wire_writers.push(wire_writer);
                    }
                    native_args.push(native_arg);
                    (wire_writers, native_args)
                },
            );

        let wire_writer_closes = wire_writers
            .iter()
            .map(|binding| binding.binding_name.clone())
            .rev()
            .collect();

        let return_type = return_abi.kotlin_type().map(String::from);
        let throws = contract.returns.throws();
        let err_type = Self::error_type_name(returns, module);
        let is_blittable_return =
            return_abi.is_wire_encoded() && Self::is_blittable_return(returns, module);
        let decode_expr = return_abi
            .is_wire_encoded()
            .then(|| Self::compute_decode_expr(returns, module, is_blittable_return))
            .unwrap_or_default();

        Self {
            func_name: NamingConvention::method_name(function_name),
            ffi_name,
            signature_params,
            wire_writers,
            wire_writer_closes,
            native_args,
            return_type,
            return_abi,
            decode_expr,
            throws,
            err_type,
            is_blittable_return,
        }
    }

    fn supports_param_type(param_type: &Type, module: &Module) -> bool {
        PassThroughType::try_from_model(param_type).is_some() || Self::supports_wire_type(param_type, module)
    }

    fn supports_return_type(returns: &ReturnType, module: &Module) -> bool {
        match returns {
            ReturnType::Void => true,
            ReturnType::Value(ty) => PassThroughType::try_from_model(ty).is_some() || Self::supports_wire_type(ty, module),
            ReturnType::Fallible { ok, err } => {
                Self::supports_wire_type(ok, module) && Self::supports_wire_type(err, module)
            }
        }
    }

    fn supports_wire_type(ty: &Type, module: &Module) -> bool {
        match ty {
            Type::Primitive(_) | Type::String | Type::Bytes | Type::Void => true,
            Type::Vec(inner) | Type::Option(inner) => Self::supports_wire_type(inner, module),
            Type::Result { ok, err } => {
                Self::supports_wire_type(ok, module) && Self::supports_wire_type(err, module)
            }
            Type::Record(_) | Type::Enum(_) => true,
            Type::Slice(_) | Type::MutSlice(_) | Type::Object(_) | Type::BoxedTrait(_) | Type::Closure(_) => false,
        }
    }

    fn is_blittable_return(returns: &ReturnType, module: &Module) -> bool {
        match returns {
            ReturnType::Value(Type::Record(name)) => Self::is_record_blittable(name, module),
            ReturnType::Value(Type::Vec(inner)) => inner
                .as_ref()
                .record_name()
                .is_some_and(|record_name| Self::is_record_blittable(record_name, module)),
            _ => false,
        }
    }

    fn is_record_blittable(name: &str, module: &Module) -> bool {
        module
            .records
            .iter()
            .find(|record| record.name == name)
            .is_some_and(|record| record.is_blittable())
    }

    fn compute_decode_expr(returns: &ReturnType, module: &Module, is_blittable: bool) -> String {
        match returns {
            ReturnType::Void => String::new(),
            ReturnType::Value(Type::Record(name)) if is_blittable => {
                let class_name = NamingConvention::class_name(name);
                format!("{}Reader.read(buffer, 0)", class_name)
            }
            ReturnType::Value(Type::Vec(inner)) if is_blittable => inner
                .as_ref()
                .record_name()
                .map(|record_name| {
                    let class_name = NamingConvention::class_name(record_name);
                    format!("{}Reader.readAll(buffer, 4, buffer.getInt(0))", class_name)
                })
                .unwrap_or_else(|| {
                    let codec = wire::decode_type(&Type::Vec(inner.clone()), module);
                    codec.value_at("0")
                }),
            ReturnType::Value(ty) => {
                let codec = wire::decode_type(ty, module);
                codec.value_at("0")
            }
            ReturnType::Fallible { ok, err } => {
                let ok_codec = if ok.is_void() {
                    wire::decode_type(&Type::Void, module)
                } else {
                    wire::decode_type(ok, module)
                };
                let err_lambda = Self::error_lambda_reader(err, module);
                format!(
                    "wire.readResult(0, {}, {}).first.getOrThrow()",
                    ok_codec.as_lambda_reader(),
                    err_lambda
                )
            }
        }
    }

    fn error_lambda_reader(err: &Type, module: &Module) -> String {
        match err {
            Type::String => {
                "{ val (msg, s) = wire.readString(it); FfiException(-1, msg) to s }".into()
            }
            Type::Enum(name) if module.enums.iter().any(|e| &e.name == name && e.is_error) => {
                let codec = wire::decode_type(err, module);
                codec.as_lambda_reader()
            }
            _ => {
                let codec = wire::decode_type(err, module);
                codec.as_lambda_reader()
            }
        }
    }

    fn error_type_name(returns: &ReturnType, module: &Module) -> String {
        match returns.as_result_types() {
            Some((_, Type::Enum(name)))
                if module.enums.iter().any(|e| &e.name == name && e.is_error) =>
            {
                NamingConvention::class_name(name)
            }
            Some(_) => "FfiException".into(),
            None => "FfiException".into(),
        }
    }

    pub fn jni_param_type_for_wire_param(param_type: &Type) -> String {
        let pass_through = PassThroughType::try_from_model(param_type).is_some();
        if pass_through {
            TypeMapper::jni_type(param_type)
        } else {
            "ByteBuffer".into()
        }
    }
}

#[derive(Debug, Clone)]
pub struct AsyncCallPlan {
    pub func_name: String,
    pub ffi_name: String,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub signature_params: Vec<SignatureParamSpec>,
    pub wire_writers: Vec<WireWriterBinding>,
    pub wire_writer_closes: Vec<String>,
    pub native_args: Vec<String>,
    pub return_type: Option<String>,
    pub return_abi: ReturnAbi,
    pub decode_expr: String,
    pub throws: bool,
    pub err_type: String,
    pub is_blittable_return: bool,
}

impl AsyncCallPlan {
    pub fn supports_call(inputs: &[Parameter], returns: &ReturnType, module: &Module) -> bool {
        let inputs_supported = inputs.iter().all(|param| {
            let ty = &param.param_type;
            !matches!(ty, Type::Closure(_) | Type::Object(_) | Type::BoxedTrait(_) | Type::MutSlice(_))
                && WireFunctionPlan::supports_param_type(ty, module)
        });

        inputs_supported && Self::supports_return_type(returns, module)
    }

    pub fn supports_returns(returns: &ReturnType, module: &Module) -> bool {
        Self::supports_return_type(returns, module)
    }

    fn supports_return_type(returns: &ReturnType, module: &Module) -> bool {
        match returns {
            ReturnType::Void => true,
            ReturnType::Value(ty) => Self::supports_value_type(ty, module),
            ReturnType::Fallible { ok, err } => {
                Self::supports_value_type(ok, module) && Self::supports_value_type(err, module)
            }
        }
    }

    fn supports_value_type(ty: &Type, module: &Module) -> bool {
        match ty {
            Type::Void | Type::Primitive(_) | Type::String | Type::Bytes => true,
            Type::Vec(inner) | Type::Option(inner) => Self::supports_value_type(inner, module),
            Type::Result { ok, err } => {
                Self::supports_value_type(ok, module) && Self::supports_value_type(err, module)
            }
            Type::Record(_) | Type::Enum(_) => true,
            Type::Slice(_) | Type::MutSlice(_) | Type::Object(_) | Type::BoxedTrait(_) | Type::Closure(_) => false,
        }
    }

    pub fn for_function(function_name: &str, inputs: &[Parameter], returns: &ReturnType, module: &Module) -> Self {
        let contract = CallContract::for_function(inputs, returns, module);
        let return_abi = ReturnAbi::from_return_type(returns, module);
        let is_blittable_return =
            return_abi.is_wire_encoded() && WireFunctionPlan::is_blittable_return(returns, module);
        let decode_expr = return_abi
            .is_wire_encoded()
            .then(|| WireFunctionPlan::compute_decode_expr(returns, module, is_blittable_return))
            .unwrap_or_default();

        let signature_params = inputs
            .iter()
            .map(|param| {
                let param_name = NamingConvention::param_name(&param.name);
                let kotlin_type = TypeMapper::map_type(&param.param_type);
                SignatureParamSpec {
                    name: param_name,
                    kotlin_type,
                }
            })
            .collect();

        let (wire_writers, native_args) = inputs
            .iter()
            .zip(contract.params.iter())
            .map(|(param, param_contract)| {
                let param_name = NamingConvention::param_name(&param.name);
                match &param_contract.transport {
                    ParamTransport::PassThrough(_) => (
                        None,
                        ParamConversion::to_ffi(&param_name, &param.param_type, module),
                    ),
                    ParamTransport::WireEncoded(_) => {
                        let encoder = wire::encode_type(&param.param_type, &param_name, module);
                        let binding_name = format!("wire_writer_{}", param_name);
                        (
                            Some(WireWriterBinding {
                                binding_name: binding_name.clone(),
                                size_expr: encoder.size_expr,
                                encode_expr: encoder.encode_expr,
                            }),
                            format!("{}.buffer", binding_name),
                        )
                    }
                }
            })
            .fold(
                (Vec::new(), Vec::new()),
                |(mut wire_writers, mut native_args), (maybe_wire_writer, native_arg)| {
                    if let Some(wire_writer) = maybe_wire_writer {
                        wire_writers.push(wire_writer);
                    }
                    native_args.push(native_arg);
                    (wire_writers, native_args)
                },
            );

        let wire_writer_closes = wire_writers
            .iter()
            .map(|binding| binding.binding_name.clone())
            .rev()
            .collect();

        Self {
            func_name: NamingConvention::method_name(function_name),
            ffi_name: naming::function_ffi_name(function_name),
            ffi_poll: naming::function_ffi_poll(function_name),
            ffi_complete: naming::function_ffi_complete(function_name),
            ffi_cancel: naming::function_ffi_cancel(function_name),
            ffi_free: naming::function_ffi_free(function_name),
            signature_params,
            wire_writers,
            wire_writer_closes,
            native_args,
            return_type: return_abi.kotlin_type().map(String::from),
            return_abi,
            decode_expr,
            throws: contract.returns.throws(),
            err_type: WireFunctionPlan::error_type_name(returns, module),
            is_blittable_return,
        }
    }

    pub fn for_method(class: &Class, method: &Method, module: &Module) -> Self {
        let contract = CallContract::for_function(&method.inputs, &method.returns, module);
        let return_abi = ReturnAbi::from_return_type(&method.returns, module);
        let is_blittable_return = return_abi.is_wire_encoded()
            && WireFunctionPlan::is_blittable_return(&method.returns, module);
        let decode_expr = return_abi
            .is_wire_encoded()
            .then(|| {
                WireFunctionPlan::compute_decode_expr(&method.returns, module, is_blittable_return)
            })
            .unwrap_or_default();

        let signature_params = method
            .inputs
            .iter()
            .map(|param| {
                let param_name = NamingConvention::param_name(&param.name);
                let kotlin_type = TypeMapper::map_type(&param.param_type);
                SignatureParamSpec {
                    name: param_name,
                    kotlin_type,
                }
            })
            .collect();

        let (wire_writers, native_args) = method
            .inputs
            .iter()
            .zip(contract.params.iter())
            .map(|(param, param_contract)| {
                let param_name = NamingConvention::param_name(&param.name);
                match &param_contract.transport {
                    ParamTransport::PassThrough(_) => (
                        None,
                        ParamConversion::to_ffi(&param_name, &param.param_type, module),
                    ),
                    ParamTransport::WireEncoded(_) => {
                        let encoder = wire::encode_type(&param.param_type, &param_name, module);
                        let binding_name = format!("wire_writer_{}", param_name);
                        (
                            Some(WireWriterBinding {
                                binding_name: binding_name.clone(),
                                size_expr: encoder.size_expr,
                                encode_expr: encoder.encode_expr,
                            }),
                            format!("{}.buffer", binding_name),
                        )
                    }
                }
            })
            .fold(
                (Vec::new(), Vec::new()),
                |(mut wire_writers, mut native_args), (maybe_wire_writer, native_arg)| {
                    if let Some(wire_writer) = maybe_wire_writer {
                        wire_writers.push(wire_writer);
                    }
                    native_args.push(native_arg);
                    (wire_writers, native_args)
                },
            );

        let wire_writer_closes = wire_writers
            .iter()
            .map(|binding| binding.binding_name.clone())
            .rev()
            .collect();

        Self {
            func_name: NamingConvention::method_name(&method.name),
            ffi_name: naming::method_ffi_name(&class.name, &method.name),
            ffi_poll: naming::method_ffi_poll(&class.name, &method.name),
            ffi_complete: naming::method_ffi_complete(&class.name, &method.name),
            ffi_cancel: naming::method_ffi_cancel(&class.name, &method.name),
            ffi_free: naming::method_ffi_free(&class.name, &method.name),
            signature_params,
            wire_writers,
            wire_writer_closes,
            native_args,
            return_type: return_abi.kotlin_type().map(String::from),
            return_abi,
            decode_expr,
            throws: contract.returns.throws(),
            err_type: WireFunctionPlan::error_type_name(&method.returns, module),
            is_blittable_return,
        }
    }
}
