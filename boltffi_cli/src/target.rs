use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Ios,
    IosSimulator,
    MacOs,
    Android,
    Wasm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Architecture {
    Arm64,
    X86_64,
    Armv7,
    X86,
    Wasm32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RustTarget {
    triple: &'static str,
    platform: Platform,
    architecture: Architecture,
}

impl RustTarget {
    pub const IOS_ARM64: Self = Self {
        triple: "aarch64-apple-ios",
        platform: Platform::Ios,
        architecture: Architecture::Arm64,
    };

    pub const IOS_SIM_ARM64: Self = Self {
        triple: "aarch64-apple-ios-sim",
        platform: Platform::IosSimulator,
        architecture: Architecture::Arm64,
    };

    pub const IOS_SIM_X86_64: Self = Self {
        triple: "x86_64-apple-ios",
        platform: Platform::IosSimulator,
        architecture: Architecture::X86_64,
    };

    pub const MACOS_ARM64: Self = Self {
        triple: "aarch64-apple-darwin",
        platform: Platform::MacOs,
        architecture: Architecture::Arm64,
    };

    pub const MACOS_X86_64: Self = Self {
        triple: "x86_64-apple-darwin",
        platform: Platform::MacOs,
        architecture: Architecture::X86_64,
    };

    pub const ANDROID_ARM64: Self = Self {
        triple: "aarch64-linux-android",
        platform: Platform::Android,
        architecture: Architecture::Arm64,
    };

    pub const ANDROID_ARMV7: Self = Self {
        triple: "armv7-linux-androideabi",
        platform: Platform::Android,
        architecture: Architecture::Armv7,
    };

    pub const ANDROID_X86_64: Self = Self {
        triple: "x86_64-linux-android",
        platform: Platform::Android,
        architecture: Architecture::X86_64,
    };

    pub const ANDROID_X86: Self = Self {
        triple: "i686-linux-android",
        platform: Platform::Android,
        architecture: Architecture::X86,
    };

    pub const WASM32_UNKNOWN_UNKNOWN: Self = Self {
        triple: "wasm32-unknown-unknown",
        platform: Platform::Wasm,
        architecture: Architecture::Wasm32,
    };

    pub const ALL_IOS: &'static [Self] =
        &[Self::IOS_ARM64, Self::IOS_SIM_ARM64, Self::IOS_SIM_X86_64];

    pub const ALL_MACOS: &'static [Self] = &[Self::MACOS_ARM64, Self::MACOS_X86_64];

    pub const ALL_ANDROID: &'static [Self] = &[
        Self::ANDROID_ARM64,
        Self::ANDROID_ARMV7,
        Self::ANDROID_X86_64,
        Self::ANDROID_X86,
    ];

    pub const ALL_WASM: &'static [Self] = &[Self::WASM32_UNKNOWN_UNKNOWN];

    pub fn triple(&self) -> &'static str {
        self.triple
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn architecture(&self) -> Architecture {
        self.architecture
    }

    pub fn library_path(&self, target_dir: &Path, lib_name: &str, release: bool) -> PathBuf {
        let profile = if release { "release" } else { "debug" };
        let artifact_name = match self.platform {
            Platform::Wasm => format!("{}.wasm", lib_name),
            Platform::Ios | Platform::IosSimulator | Platform::MacOs | Platform::Android => {
                format!("lib{}.a", lib_name)
            }
        };
        target_dir
            .join(self.triple)
            .join(profile)
            .join(artifact_name)
    }
}

impl Platform {
    pub fn is_apple(&self) -> bool {
        matches!(
            self,
            Platform::Ios | Platform::IosSimulator | Platform::MacOs
        )
    }
}

impl Architecture {
    pub fn android_abi(&self) -> &'static str {
        match self {
            Architecture::Arm64 => "arm64-v8a",
            Architecture::Armv7 => "armeabi-v7a",
            Architecture::X86_64 => "x86_64",
            Architecture::X86 => "x86",
            Architecture::Wasm32 => unreachable!("wasm targets do not map to android abi"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuiltLibrary {
    pub target: RustTarget,
    pub path: PathBuf,
}

impl BuiltLibrary {
    pub fn discover(target_dir: &Path, lib_name: &str, release: bool) -> Vec<Self> {
        let all_targets = RustTarget::ALL_IOS
            .iter()
            .chain(RustTarget::ALL_MACOS)
            .chain(RustTarget::ALL_ANDROID)
            .chain(RustTarget::ALL_WASM);

        all_targets
            .filter_map(|target| {
                let path = target.library_path(target_dir, lib_name, release);
                path.exists().then(|| BuiltLibrary {
                    target: target.clone(),
                    path,
                })
            })
            .collect()
    }
}
