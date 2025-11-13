use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub package: PackageConfig,
    #[serde(default)]
    pub swift: SwiftConfig,
    #[serde(default)]
    pub kotlin: KotlinConfig,
    #[serde(default)]
    pub ios: IosConfig,
    #[serde(default)]
    pub android: AndroidConfig,
    #[serde(default)]
    pub pack: PackConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PackageConfig {
    pub name: String,
    #[serde(rename = "crate")]
    pub crate_name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SwiftConfig {
    pub module_name: Option<String>,
    pub output: PathBuf,
    pub tools_version: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KotlinConfig {
    pub package: String,
    pub output: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IosConfig {
    pub deployment_target: String,
    pub include_macos: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AndroidConfig {
    pub min_sdk: u32,
    pub ndk_version: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PackConfig {
    #[serde(default)]
    pub xcframework: XcframeworkConfig,
    #[serde(default)]
    pub spm: SpmConfig,
    #[serde(default)]
    pub android: AndroidPackConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct XcframeworkConfig {
    pub output: PathBuf,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SpmConfig {
    pub output: PathBuf,
    pub repo_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AndroidPackConfig {
    pub output: PathBuf,
}

impl Default for SwiftConfig {
    fn default() -> Self {
        Self {
            module_name: None,
            output: PathBuf::from("bindings/swift"),
            tools_version: None,
        }
    }
}

impl Default for KotlinConfig {
    fn default() -> Self {
        Self {
            package: String::from("com.example"),
            output: PathBuf::from("bindings/kotlin"),
        }
    }
}

impl Default for IosConfig {
    fn default() -> Self {
        Self {
            deployment_target: String::from("16.0"),
            include_macos: false,
        }
    }
}

impl Default for AndroidConfig {
    fn default() -> Self {
        Self {
            min_sdk: 24,
            ndk_version: None,
        }
    }
}

impl Default for PackConfig {
    fn default() -> Self {
        Self {
            xcframework: XcframeworkConfig::default(),
            spm: SpmConfig::default(),
            android: AndroidPackConfig::default(),
        }
    }
}

impl Default for XcframeworkConfig {
    fn default() -> Self {
        Self {
            output: PathBuf::from("dist"),
            name: None,
        }
    }
}

impl Default for SpmConfig {
    fn default() -> Self {
        Self {
            output: PathBuf::from("dist"),
            repo_url: None,
        }
    }
}

impl Default for AndroidPackConfig {
    fn default() -> Self {
        Self {
            output: PathBuf::from("dist/jniLibs"),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|err| ConfigError::ReadFailed {
            path: path.to_path_buf(),
            source: err,
        })?;

        toml::from_str(&content).map_err(|err| ConfigError::ParseFailed {
            path: path.to_path_buf(),
            source: err,
        })
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self).map_err(ConfigError::SerializeFailed)?;

        std::fs::write(path, content).map_err(|err| ConfigError::WriteFailed {
            path: path.to_path_buf(),
            source: err,
        })
    }

    pub fn library_name(&self) -> &str {
        self.package
            .crate_name
            .as_deref()
            .unwrap_or(&self.package.name)
    }

    pub fn swift_module_name(&self) -> String {
        self.swift
            .module_name
            .clone()
            .unwrap_or_else(|| to_pascal_case(&self.package.name))
    }

    pub fn xcframework_name(&self) -> String {
        self.pack
            .xcframework
            .name
            .clone()
            .unwrap_or_else(|| self.swift_module_name())
    }
}

fn to_pascal_case(input: &str) -> String {
    input
        .split(|c: char| c == '_' || c == '-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config from {path}")]
    ReadFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse config from {path}")]
    ParseFailed {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("failed to serialize config")]
    SerializeFailed(#[source] toml::ser::Error),

    #[error("failed to write config to {path}")]
    WriteFailed {
        path: PathBuf,
        source: std::io::Error,
    },
}
