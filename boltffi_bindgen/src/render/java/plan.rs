use super::JavaVersion;

#[derive(Debug, Clone)]
pub struct JavaModule {
    pub package_name: String,
    pub class_name: String,
    pub lib_name: String,
    pub java_version: JavaVersion,
    pub prefix: String,
    pub records: Vec<JavaRecord>,
    pub functions: Vec<JavaFunction>,
}

impl JavaModule {
    pub fn package_path(&self) -> String {
        self.package_name.replace('.', "/")
    }

    pub fn has_wire_params(&self) -> bool {
        self.functions.iter().any(|f| !f.wire_writers.is_empty())
    }

    pub fn needs_wire_writer(&self) -> bool {
        self.has_wire_params() || !self.records.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct JavaRecord {
    pub shape: JavaRecordShape,
    pub class_name: String,
    pub fields: Vec<JavaRecordField>,
}

impl JavaRecord {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn uses_native_record_syntax(&self) -> bool {
        matches!(self.shape, JavaRecordShape::NativeRecord)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaRecordShape {
    ClassicClass,
    NativeRecord,
}

#[derive(Debug, Clone)]
pub struct JavaRecordField {
    pub name: String,
    pub java_type: String,
    pub wire_decode_expr: String,
    pub wire_size_expr: String,
    pub wire_encode_expr: String,
    pub equals_expr: String,
    pub hash_expr: String,
}

#[derive(Debug, Clone)]
pub enum JavaReturnStrategy {
    Void,
    Direct,
    WireDecode { decode_expr: String },
}

impl JavaReturnStrategy {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct)
    }

    pub fn is_wire(&self) -> bool {
        matches!(self, Self::WireDecode { .. })
    }

    pub fn decode_expr(&self) -> &str {
        match self {
            Self::WireDecode { decode_expr } => decode_expr,
            _ => "",
        }
    }

    pub fn native_return_type<'a>(&self, return_type: &'a str) -> &'a str {
        match self {
            Self::Void => "void",
            Self::Direct => return_type,
            Self::WireDecode { .. } => "byte[]",
        }
    }
}

#[derive(Debug, Clone)]
pub struct JavaFunction {
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<JavaParam>,
    pub return_type: String,
    pub strategy: JavaReturnStrategy,
    pub wire_writers: Vec<JavaWireWriter>,
}

impl JavaFunction {
    pub fn native_return_type(&self) -> &str {
        self.strategy.native_return_type(&self.return_type)
    }
}

#[derive(Debug, Clone)]
pub struct JavaWireWriter {
    pub binding_name: String,
    pub param_name: String,
    pub size_expr: String,
    pub encode_expr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaParamKind {
    Direct,
    Utf8Bytes,
    WireEncoded { binding_name: String },
}

#[derive(Debug, Clone)]
pub struct JavaParam {
    pub name: String,
    pub java_type: String,
    pub native_type: String,
    pub kind: JavaParamKind,
}

impl JavaParam {
    pub fn needs_conversion(&self) -> bool {
        self.kind != JavaParamKind::Direct
    }

    pub fn to_native_expr(&self) -> String {
        match &self.kind {
            JavaParamKind::Direct => self.name.clone(),
            JavaParamKind::Utf8Bytes => format!(
                "{}.getBytes(java.nio.charset.StandardCharsets.UTF_8)",
                self.name
            ),
            JavaParamKind::WireEncoded { binding_name } => {
                format!("{}.toBuffer()", binding_name)
            }
        }
    }
}
