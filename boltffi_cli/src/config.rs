use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Swift,
    Kotlin,
    Java,
    TypeScript,
    Header,
}

impl Target {
    pub const fn name(self) -> &'static str {
        match self {
            Target::Swift => "swift",
            Target::Kotlin => "kotlin",
            Target::Java => "java",
            Target::TypeScript => "typescript",
            Target::Header => "header",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Experimental {
    WholeTarget(Target),
    Feature { target: Target, name: &'static str },
}

impl Experimental {
    pub const ALL: &'static [Experimental] = &[
        Experimental::WholeTarget(Target::Java),
        Experimental::Feature {
            target: Target::TypeScript,
            name: "async_streams",
        },
    ];

    pub fn is_target_experimental(target: Target) -> bool {
        Self::ALL
            .iter()
            .any(|e| matches!(e, Experimental::WholeTarget(t) if *t == target))
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub experimental: Vec<String>,
    pub package: PackageConfig,
    #[serde(default)]
    pub targets: TargetsConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageConfig {
    pub name: String,
    #[serde(rename = "crate")]
    pub crate_name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TargetsConfig {
    #[serde(default)]
    pub apple: AppleConfig,
    #[serde(default)]
    pub android: AndroidConfig,
    #[serde(default)]
    pub wasm: WasmConfig,
    #[serde(default)]
    pub java: JavaConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ErrorStyle {
    #[default]
    Throwing,
    Result,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FactoryStyle {
    #[default]
    Constructors,
    CompanionMethods,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TypeConversion {
    UuidString,
    UrlString,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct TypeMapping {
    #[serde(rename = "type")]
    pub native_type: String,
    pub conversion: TypeConversion,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AppleSwiftConfig {
    pub module_name: Option<String>,
    pub output: Option<PathBuf>,
    pub ffi_module_name: Option<String>,
    pub tools_version: Option<String>,
    #[serde(default)]
    pub error_style: ErrorStyle,
    #[serde(default)]
    pub type_mappings: HashMap<String, TypeMapping>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AndroidKotlinConfig {
    pub package: Option<String>,
    pub output: Option<PathBuf>,
    pub module_name: Option<String>,
    pub library_name: Option<String>,
    #[serde(default)]
    pub api_style: KotlinApiStyle,
    #[serde(default)]
    pub error_style: ErrorStyle,
    #[serde(default)]
    pub factory_style: FactoryStyle,
    #[serde(default)]
    pub type_mappings: HashMap<String, TypeMapping>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum KotlinApiStyle {
    #[default]
    TopLevel,
    ModuleObject,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppleConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_apple_output")]
    pub output: PathBuf,
    #[serde(default = "default_apple_deployment_target")]
    pub deployment_target: String,
    #[serde(default)]
    pub include_macos: bool,
    #[serde(default)]
    pub swift: AppleSwiftConfig,
    #[serde(default)]
    pub header: HeaderConfig,
    #[serde(default)]
    pub xcframework: XcframeworkConfig,
    #[serde(default)]
    pub spm: SpmConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AndroidConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_android_output")]
    pub output: PathBuf,
    #[serde(default = "default_android_min_sdk")]
    pub min_sdk: u32,
    pub ndk_version: Option<String>,
    #[serde(default)]
    pub kotlin: AndroidKotlinConfig,
    #[serde(default)]
    pub header: HeaderConfig,
    #[serde(default)]
    pub pack: AndroidPackConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct HeaderConfig {
    pub output: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct XcframeworkConfig {
    pub output: Option<PathBuf>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SpmDistribution {
    #[default]
    Local,
    Remote,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SpmConfig {
    pub output: Option<PathBuf>,
    #[serde(default)]
    pub distribution: SpmDistribution,
    pub repo_url: Option<String>,
    #[serde(default)]
    pub layout: SpmLayout,
    pub package_name: Option<String>,
    pub wrapper_sources: Option<PathBuf>,
    #[serde(default)]
    pub skip_package_swift: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AndroidPackConfig {
    pub output: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct JavaConfig {
    pub package: Option<String>,
    pub module_name: Option<String>,
    pub min_version: Option<u8>,
    #[serde(default)]
    pub jvm: JavaJvmConfig,
    #[serde(default)]
    pub android: JavaAndroidConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JavaJvmConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_java_jvm_output")]
    pub output: PathBuf,
}

impl Default for JavaJvmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output: default_java_jvm_output(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JavaAndroidConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_java_android_output")]
    pub output: PathBuf,
    #[serde(default = "default_android_min_sdk")]
    pub min_sdk: u32,
}

impl Default for JavaAndroidConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output: default_java_android_output(),
            min_sdk: default_android_min_sdk(),
        }
    }
}

fn default_java_jvm_output() -> PathBuf {
    PathBuf::from("dist/java")
}

fn default_java_android_output() -> PathBuf {
    PathBuf::from("dist/java/android")
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SpmLayout {
    Bundled,
    Split,
    #[default]
    FfiOnly,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WasmConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_wasm_triple")]
    pub triple: String,
    #[serde(default)]
    pub profile: WasmProfile,
    #[serde(default = "default_wasm_output")]
    pub output: PathBuf,
    pub artifact_path: Option<PathBuf>,
    #[serde(default)]
    pub optimize: WasmOptimizeConfig,
    #[serde(default)]
    pub typescript: WasmTypeScriptConfig,
    #[serde(default)]
    pub npm: WasmNpmConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum WasmProfile {
    Debug,
    #[default]
    Release,
}

impl WasmProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct WasmOptimizeConfig {
    pub enabled: Option<bool>,
    pub level: Option<WasmOptimizeLevel>,
    pub strip_debug: Option<bool>,
    pub on_missing: Option<WasmOptimizeOnMissing>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
pub enum WasmOptimizeLevel {
    #[serde(rename = "0")]
    O0,
    #[serde(rename = "1")]
    O1,
    #[serde(rename = "2")]
    O2,
    #[serde(rename = "3")]
    O3,
    #[serde(rename = "4")]
    O4,
    #[serde(rename = "s")]
    Size,
    #[serde(rename = "z")]
    MinSize,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WasmOptimizeOnMissing {
    Error,
    Warn,
    Skip,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct WasmTypeScriptConfig {
    pub output: Option<PathBuf>,
    pub runtime_package: Option<String>,
    pub runtime_version: Option<String>,
    pub module_name: Option<String>,
    pub source_map: Option<bool>,
    #[serde(default)]
    pub type_mappings: HashMap<String, TypeMapping>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct WasmNpmConfig {
    pub package_name: Option<String>,
    pub output: Option<PathBuf>,
    pub targets: Option<Vec<WasmNpmTarget>>,
    pub generate_package_json: Option<bool>,
    pub generate_readme: Option<bool>,
    pub version: Option<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WasmNpmTarget {
    Bundler,
    Web,
    Nodejs,
}

impl Default for AppleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            output: default_apple_output(),
            deployment_target: default_apple_deployment_target(),
            include_macos: false,
            swift: AppleSwiftConfig::default(),
            header: HeaderConfig::default(),
            xcframework: XcframeworkConfig::default(),
            spm: SpmConfig::default(),
        }
    }
}

impl Default for AndroidConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            output: default_android_output(),
            min_sdk: default_android_min_sdk(),
            ndk_version: None,
            kotlin: AndroidKotlinConfig::default(),
            header: HeaderConfig::default(),
            pack: AndroidPackConfig::default(),
        }
    }
}

impl Default for SpmConfig {
    fn default() -> Self {
        Self {
            output: None,
            distribution: SpmDistribution::Local,
            repo_url: None,
            layout: SpmLayout::default(),
            package_name: None,
            wrapper_sources: None,
            skip_package_swift: false,
        }
    }
}

impl Default for WasmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            triple: default_wasm_triple(),
            profile: WasmProfile::Release,
            output: default_wasm_output(),
            artifact_path: None,
            optimize: WasmOptimizeConfig::default(),
            typescript: WasmTypeScriptConfig::default(),
            npm: WasmNpmConfig::default(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_apple_output() -> PathBuf {
    PathBuf::from("dist/apple")
}

fn default_apple_deployment_target() -> String {
    "16.0".to_string()
}

fn default_android_output() -> PathBuf {
    PathBuf::from("dist/android")
}

fn default_android_min_sdk() -> u32 {
    24
}

fn default_wasm_triple() -> String {
    "wasm32-unknown-unknown".to_string()
}

fn default_wasm_output() -> PathBuf {
    PathBuf::from("dist/wasm")
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|err| ConfigError::Read {
            path: path.to_path_buf(),
            source: err,
        })?;

        let config: Config = toml::from_str(&content).map_err(|err| ConfigError::Parse {
            path: path.to_path_buf(),
            source: err,
        })?;

        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        self.validate()?;
        let content = toml::to_string_pretty(self).map_err(ConfigError::Serialize)?;

        std::fs::write(path, content).map_err(|err| ConfigError::Write {
            path: path.to_path_buf(),
            source: err,
        })
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.is_apple_enabled()
            && self.apple_spm_distribution() == SpmDistribution::Remote
            && self.apple_spm_repo_url().is_none()
        {
            return Err(ConfigError::Validation(
                "targets.apple.spm.repo_url is required when distribution = \"remote\"".to_string(),
            ));
        }

        if self.is_wasm_enabled()
            && let Some(targets) = self.targets.wasm.npm.targets.as_ref()
            && targets.is_empty()
        {
            return Err(ConfigError::Validation(
                "targets.wasm.npm.targets must be non-empty when provided".to_string(),
            ));
        }

        Ok(())
    }

    pub fn library_name(&self) -> &str {
        self.package
            .crate_name
            .as_deref()
            .unwrap_or(&self.package.name)
    }

    pub fn crate_artifact_name(&self) -> String {
        self.library_name().replace('-', "_")
    }

    pub fn swift_module_name(&self) -> String {
        self.targets
            .apple
            .swift
            .module_name
            .clone()
            .unwrap_or_else(|| to_pascal_case(&self.package.name))
    }

    pub fn xcframework_name(&self) -> String {
        self.targets
            .apple
            .xcframework
            .name
            .clone()
            .unwrap_or_else(|| self.swift_module_name())
    }

    pub fn is_apple_enabled(&self) -> bool {
        self.targets.apple.enabled
    }

    pub fn is_android_enabled(&self) -> bool {
        self.targets.android.enabled
    }

    pub fn is_wasm_enabled(&self) -> bool {
        self.targets.wasm.enabled
    }

    pub fn apple_include_macos(&self) -> bool {
        self.targets.apple.include_macos
    }

    pub fn apple_deployment_target(&self) -> &str {
        &self.targets.apple.deployment_target
    }

    pub fn apple_output(&self) -> PathBuf {
        self.targets.apple.output.clone()
    }

    pub fn apple_swift_output(&self) -> PathBuf {
        self.targets
            .apple
            .swift
            .output
            .clone()
            .unwrap_or_else(|| self.targets.apple.output.join("Sources"))
    }

    pub fn apple_swift_ffi_module_name(&self) -> Option<&str> {
        self.targets.apple.swift.ffi_module_name.as_deref()
    }

    pub fn apple_header_output(&self) -> PathBuf {
        self.targets
            .apple
            .header
            .output
            .clone()
            .unwrap_or_else(|| self.targets.apple.output.join("include"))
    }

    pub fn apple_xcframework_output(&self) -> PathBuf {
        self.targets
            .apple
            .xcframework
            .output
            .clone()
            .unwrap_or_else(|| self.targets.apple.output.clone())
    }

    pub fn apple_spm_output(&self) -> PathBuf {
        self.targets
            .apple
            .spm
            .output
            .clone()
            .unwrap_or_else(|| self.targets.apple.output.clone())
    }

    pub fn apple_spm_layout(&self) -> SpmLayout {
        self.targets.apple.spm.layout
    }

    pub fn apple_spm_wrapper_sources(&self) -> Option<&Path> {
        self.targets.apple.spm.wrapper_sources.as_deref()
    }

    pub fn android_min_sdk(&self) -> u32 {
        self.targets.android.min_sdk
    }

    pub fn android_ndk_version(&self) -> Option<&str> {
        self.targets.android.ndk_version.as_deref()
    }

    pub fn android_output(&self) -> PathBuf {
        self.targets.android.output.clone()
    }

    pub fn android_kotlin_package(&self) -> String {
        self.targets
            .android
            .kotlin
            .package
            .clone()
            .unwrap_or_else(|| {
                let normalized_name = self.package.name.replace('-', "_");
                format!("com.example.{}", normalized_name)
            })
    }

    pub fn android_kotlin_module_name(&self) -> String {
        self.targets
            .android
            .kotlin
            .module_name
            .clone()
            .unwrap_or_else(|| self.kotlin_class_name())
    }

    pub fn android_kotlin_library_name(&self) -> Option<&str> {
        self.targets.android.kotlin.library_name.as_deref()
    }

    pub fn android_kotlin_api_style(&self) -> KotlinApiStyle {
        self.targets.android.kotlin.api_style
    }

    pub fn android_kotlin_factory_style(&self) -> FactoryStyle {
        self.targets.android.kotlin.factory_style
    }

    pub fn android_kotlin_output(&self) -> PathBuf {
        self.targets
            .android
            .kotlin
            .output
            .clone()
            .unwrap_or_else(|| self.targets.android.output.join("kotlin"))
    }

    pub fn android_header_output(&self) -> PathBuf {
        self.targets
            .android
            .header
            .output
            .clone()
            .unwrap_or_else(|| self.targets.android.output.join("include"))
    }

    pub fn android_pack_output(&self) -> PathBuf {
        self.targets
            .android
            .pack
            .output
            .clone()
            .unwrap_or_else(|| self.targets.android.output.join("jniLibs"))
    }

    pub fn kotlin_class_name(&self) -> String {
        to_pascal_case(&self.package.name)
    }

    pub fn apple_spm_distribution(&self) -> SpmDistribution {
        self.targets.apple.spm.distribution
    }

    pub fn apple_spm_repo_url(&self) -> Option<&str> {
        self.targets.apple.spm.repo_url.as_deref()
    }

    pub fn apple_swift_tools_version(&self) -> Option<&str> {
        self.targets.apple.swift.tools_version.as_deref()
    }

    pub fn apple_spm_package_name(&self) -> Option<&str> {
        self.targets.apple.spm.package_name.as_deref()
    }

    pub fn apple_spm_skip_package_swift(&self) -> bool {
        self.targets.apple.spm.skip_package_swift
    }

    pub fn swift_type_mappings(&self) -> &HashMap<String, TypeMapping> {
        &self.targets.apple.swift.type_mappings
    }

    pub fn kotlin_type_mappings(&self) -> &HashMap<String, TypeMapping> {
        &self.targets.android.kotlin.type_mappings
    }

    pub fn is_java_jvm_enabled(&self) -> bool {
        self.targets.java.jvm.enabled
    }

    pub fn is_java_android_enabled(&self) -> bool {
        self.targets.java.android.enabled
    }

    pub fn is_enabled(&self, target: Target) -> bool {
        match target {
            Target::Swift => self.is_apple_enabled(),
            Target::Kotlin => self.is_android_enabled(),
            Target::Java => self.is_java_jvm_enabled(),
            Target::TypeScript => self.is_wasm_enabled(),
            Target::Header => self.is_apple_enabled() || self.is_android_enabled(),
        }
    }

    pub fn should_process(&self, target: Target, experimental_flag: bool) -> bool {
        self.is_enabled(target)
            && (!Experimental::is_target_experimental(target) || experimental_flag)
    }

    fn is_experimental_enabled(&self, exp: &Experimental) -> bool {
        let key = match exp {
            Experimental::WholeTarget(t) => t.name().to_string(),
            Experimental::Feature { target, name } => format!("{}.{}", target.name(), name),
        };
        self.experimental.contains(&key)
    }

    pub fn typescript_experimental(
        &self,
    ) -> boltffi_bindgen::render::typescript::TypeScriptExperimental {
        boltffi_bindgen::render::typescript::TypeScriptExperimental {
            async_streams: self.is_experimental_enabled(&Experimental::Feature {
                target: Target::TypeScript,
                name: "async_streams",
            }),
        }
    }

    pub fn java_package(&self) -> String {
        self.targets
            .java
            .package
            .clone()
            .unwrap_or_else(|| format!("com.example.{}", self.package.name.replace('-', "_")))
    }

    pub fn java_module_name(&self) -> String {
        self.targets
            .java
            .module_name
            .clone()
            .unwrap_or_else(|| to_pascal_case(&self.package.name))
    }

    pub fn java_min_version(&self) -> Option<u8> {
        self.targets.java.min_version
    }

    pub fn java_jvm_output(&self) -> PathBuf {
        self.targets.java.jvm.output.clone()
    }

    pub fn java_android_output(&self) -> PathBuf {
        self.targets.java.android.output.clone()
    }

    pub fn wasm_triple(&self) -> &str {
        &self.targets.wasm.triple
    }

    pub fn wasm_profile(&self) -> WasmProfile {
        self.targets.wasm.profile
    }

    pub fn wasm_output(&self) -> PathBuf {
        self.targets.wasm.output.clone()
    }

    pub fn wasm_artifact_path(&self, profile: WasmProfile) -> PathBuf {
        self.targets.wasm.artifact_path.clone().unwrap_or_else(|| {
            PathBuf::from("target")
                .join(self.wasm_triple())
                .join(profile.as_str())
                .join(format!("{}.wasm", self.crate_artifact_name()))
        })
    }

    pub fn wasm_optimize_enabled(&self, profile: WasmProfile) -> bool {
        self.targets
            .wasm
            .optimize
            .enabled
            .unwrap_or(matches!(profile, WasmProfile::Release))
    }

    pub fn wasm_optimize_level(&self) -> WasmOptimizeLevel {
        self.targets
            .wasm
            .optimize
            .level
            .unwrap_or(WasmOptimizeLevel::Size)
    }

    pub fn wasm_optimize_strip_debug(&self) -> bool {
        self.targets.wasm.optimize.strip_debug.unwrap_or(true)
    }

    pub fn wasm_optimize_on_missing(&self) -> WasmOptimizeOnMissing {
        self.targets
            .wasm
            .optimize
            .on_missing
            .unwrap_or(WasmOptimizeOnMissing::Error)
    }

    pub fn wasm_typescript_output(&self) -> PathBuf {
        self.targets
            .wasm
            .typescript
            .output
            .clone()
            .unwrap_or_else(|| self.targets.wasm.output.join("pkg"))
    }

    pub fn wasm_runtime_package(&self) -> String {
        self.targets
            .wasm
            .typescript
            .runtime_package
            .clone()
            .unwrap_or_else(|| "@boltffi/runtime".to_string())
    }

    pub fn wasm_runtime_version(&self) -> String {
        self.targets
            .wasm
            .typescript
            .runtime_version
            .clone()
            .unwrap_or_else(|| "*".to_string())
    }

    pub fn wasm_typescript_module_name(&self) -> String {
        self.targets
            .wasm
            .typescript
            .module_name
            .clone()
            .unwrap_or_else(|| normalize_module_name(&self.package.name))
    }

    pub fn wasm_source_map_enabled(&self) -> bool {
        self.targets.wasm.typescript.source_map.unwrap_or(true)
    }

    #[allow(dead_code)]
    pub fn wasm_typescript_type_mappings(&self) -> &HashMap<String, TypeMapping> {
        &self.targets.wasm.typescript.type_mappings
    }

    pub fn wasm_npm_package_name(&self) -> Option<&str> {
        self.targets.wasm.npm.package_name.as_deref()
    }

    pub fn wasm_npm_output(&self) -> PathBuf {
        self.targets
            .wasm
            .npm
            .output
            .clone()
            .unwrap_or_else(|| self.wasm_typescript_output())
    }

    pub fn wasm_npm_targets(&self) -> Vec<WasmNpmTarget> {
        self.targets.wasm.npm.targets.clone().unwrap_or_else(|| {
            vec![
                WasmNpmTarget::Bundler,
                WasmNpmTarget::Web,
                WasmNpmTarget::Nodejs,
            ]
        })
    }

    pub fn wasm_npm_generate_package_json(&self) -> bool {
        self.targets.wasm.npm.generate_package_json.unwrap_or(true)
    }

    pub fn wasm_npm_generate_readme(&self) -> bool {
        self.targets.wasm.npm.generate_readme.unwrap_or(true)
    }

    pub fn package_version(&self) -> Option<String> {
        self.package
            .version
            .clone()
            .or_else(|| cargo_package_field("version"))
    }

    pub fn wasm_npm_version(&self) -> Option<String> {
        self.targets
            .wasm
            .npm
            .version
            .clone()
            .or_else(|| self.package_version())
    }

    pub fn package_license(&self) -> Option<String> {
        self.package
            .license
            .clone()
            .or_else(|| cargo_package_field("license"))
    }

    pub fn wasm_npm_license(&self) -> Option<String> {
        self.targets
            .wasm
            .npm
            .license
            .clone()
            .or_else(|| self.package_license())
    }

    pub fn package_repository(&self) -> Option<String> {
        self.package
            .repository
            .clone()
            .or_else(|| cargo_package_field("repository"))
    }

    pub fn wasm_npm_repository(&self) -> Option<String> {
        self.targets
            .wasm
            .npm
            .repository
            .clone()
            .or_else(|| self.package_repository())
    }
}

fn normalize_module_name(input: &str) -> String {
    let normalized = input
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();

    if normalized
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        format!("_{}", normalized)
    } else {
        normalized
    }
}

fn to_pascal_case(input: &str) -> String {
    input
        .split(['_', '-'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

fn cargo_package_field(field_name: &str) -> Option<String> {
    std::fs::read_to_string("Cargo.toml")
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find_map(|line| parse_key_value(line).filter(|(key, _)| key == field_name))
        })
        .map(|(_, value)| value)
}

fn parse_key_value(line: &str) -> Option<(String, String)> {
    let (raw_key, raw_value) = line.split_once('=')?;
    let key = raw_key.trim().to_string();
    let value = raw_value.trim().trim_matches('"').to_string();
    Some((key, value))
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config from {path}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse config from {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("invalid config: {0}")]
    Validation(String),

    #[error("failed to serialize config")]
    Serialize(#[source] toml::ser::Error),

    #[error("failed to write config to {path}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    #[test]
    fn parses_targets_apple_table() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.apple]
deployment_target = "16.0"
include_macos = false
"#,
        );

        assert_eq!(config.targets.apple.deployment_target, "16.0");
        assert!(!config.targets.apple.include_macos);
    }

    #[test]
    fn rejects_empty_wasm_npm_targets() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.wasm.npm]
targets = []
"#,
        )
        .expect("toml parse failed");

        assert!(parsed.validate().is_err());
    }

    #[test]
    fn rejects_remote_spm_without_repo_url() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.apple.spm]
distribution = "remote"
"#,
        )
        .expect("toml parse failed");

        assert!(parsed.validate().is_err());
    }

    #[test]
    fn allows_remote_spm_without_repo_url_when_apple_disabled() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.apple]
enabled = false

[targets.apple.spm]
distribution = "remote"
"#,
        )
        .expect("toml parse failed");

        assert!(parsed.validate().is_ok());
    }

    #[test]
    fn allows_empty_wasm_npm_targets_when_wasm_disabled() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.wasm]
enabled = false

[targets.wasm.npm]
targets = []
"#,
        )
        .expect("toml parse failed");

        assert!(parsed.validate().is_ok());
    }

    #[test]
    fn rejects_legacy_top_level_target_tables() {
        let parsed = toml::from_str::<Config>(
            r#"
[package]
name = "mylib"

[apple]
deployment_target = "16.0"
"#,
        );

        assert!(parsed.is_err());
    }
}
