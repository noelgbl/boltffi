use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Ios,
    IosSimulator,
    MacOs,
    Android,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Architecture {
    Arm64,
    X86_64,
    Armv7,
    X86,
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

    pub const ALL_IOS: &'static [Self] =
        &[Self::IOS_ARM64, Self::IOS_SIM_ARM64, Self::IOS_SIM_X86_64];

    pub const ALL_MACOS: &'static [Self] = &[Self::MACOS_ARM64, Self::MACOS_X86_64];

    pub const ALL_ANDROID: &'static [Self] = &[
        Self::ANDROID_ARM64,
        Self::ANDROID_ARMV7,
        Self::ANDROID_X86_64,
        Self::ANDROID_X86,
    ];

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
        target_dir
            .join(self.triple)
            .join(profile)
            .join(format!("lib{}.a", lib_name))
    }

    pub fn from_triple(triple: &str) -> Option<Self> {
        match triple {
            "aarch64-apple-ios" => Some(Self::IOS_ARM64),
            "aarch64-apple-ios-sim" => Some(Self::IOS_SIM_ARM64),
            "x86_64-apple-ios" => Some(Self::IOS_SIM_X86_64),
            "aarch64-apple-darwin" => Some(Self::MACOS_ARM64),
            "x86_64-apple-darwin" => Some(Self::MACOS_X86_64),
            "aarch64-linux-android" => Some(Self::ANDROID_ARM64),
            "armv7-linux-androideabi" => Some(Self::ANDROID_ARMV7),
            "x86_64-linux-android" => Some(Self::ANDROID_X86_64),
            "i686-linux-android" => Some(Self::ANDROID_X86),
            _ => None,
        }
    }
}

impl Platform {
    pub fn is_apple(&self) -> bool {
        matches!(
            self,
            Platform::Ios | Platform::IosSimulator | Platform::MacOs
        )
    }

    pub fn is_simulator(&self) -> bool {
        matches!(self, Platform::IosSimulator)
    }
}

impl Architecture {
    pub fn android_abi(&self) -> &'static str {
        match self {
            Architecture::Arm64 => "arm64-v8a",
            Architecture::Armv7 => "armeabi-v7a",
            Architecture::X86_64 => "x86_64",
            Architecture::X86 => "x86",
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
            .chain(RustTarget::ALL_ANDROID);

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

    pub fn filter_by_platform(libraries: &[Self], platform: Platform) -> Vec<&Self> {
        libraries
            .iter()
            .filter(|lib| lib.target.platform() == platform)
            .collect()
    }

    pub fn filter_simulators(libraries: &[Self]) -> Vec<&Self> {
        libraries
            .iter()
            .filter(|lib| lib.target.platform().is_simulator())
            .collect()
    }
}
