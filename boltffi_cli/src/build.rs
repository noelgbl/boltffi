use std::process::Command;

use crate::android::AndroidToolchain;
use crate::config::Config;
use crate::error::{CliError, Result};
use crate::target::{Platform, RustTarget};

#[derive(Default)]
pub struct BuildOptions {
    pub release: bool,
    pub package: Option<String>,
}

pub struct Builder<'a> {
    config: &'a Config,
    options: BuildOptions,
}

pub struct BuildResult {
    pub triple: String,
    pub success: bool,
}

impl<'a> Builder<'a> {
    pub fn new(config: &'a Config, options: BuildOptions) -> Self {
        Self { config, options }
    }

    pub fn build_targets(&self, targets: &[RustTarget]) -> Result<Vec<BuildResult>> {
        let android_toolchain = targets
            .iter()
            .any(|target| target.platform() == Platform::Android)
            .then(|| {
                AndroidToolchain::discover(
                    self.config.android_min_sdk(),
                    self.config.android_ndk_version(),
                )
            })
            .transpose()?;

        targets
            .iter()
            .map(|target| self.build_single_target(target, android_toolchain.as_ref()))
            .collect()
    }

    pub fn build_ios(&self) -> Result<Vec<BuildResult>> {
        self.build_targets(RustTarget::ALL_IOS)
    }

    pub fn build_android(&self) -> Result<Vec<BuildResult>> {
        self.build_targets(RustTarget::ALL_ANDROID)
    }

    pub fn build_macos(&self) -> Result<Vec<BuildResult>> {
        self.build_targets(RustTarget::ALL_MACOS)
    }

    pub fn build_wasm_with_triple(&self, triple: &str) -> Result<Vec<BuildResult>> {
        let mut command = Command::new("cargo");
        command.arg("build");

        if self.options.release {
            command.arg("--release");
        }

        command.arg("--target").arg(triple);

        if let Some(ref package) = self.options.package {
            command.arg("-p").arg(package);
        } else {
            command.arg("-p").arg(self.config.library_name());
        }

        let success = command
            .status()
            .map(|status| status.success())
            .unwrap_or(false);

        Ok(vec![BuildResult {
            triple: triple.to_string(),
            success,
        }])
    }

    fn build_single_target(
        &self,
        target: &RustTarget,
        android_toolchain: Option<&AndroidToolchain>,
    ) -> Result<BuildResult> {
        let mut cmd = Command::new("cargo");
        cmd.arg("build");

        if self.options.release {
            cmd.arg("--release");
        }

        cmd.arg("--target").arg(target.triple());

        if let Some(ref package) = self.options.package {
            cmd.arg("-p").arg(package);
        } else {
            cmd.arg("-p").arg(self.config.library_name());
        }

        if target.platform() == Platform::Android {
            android_toolchain
                .ok_or(CliError::AndroidNdkNotFound)
                .and_then(|toolchain| toolchain.configure_cargo_for_target(&mut cmd, target))?;
        }

        let success = cmd.status().map(|status| status.success()).unwrap_or(false);

        Ok(BuildResult {
            triple: target.triple().to_string(),
            success,
        })
    }
}
pub fn count_successful(results: &[BuildResult]) -> usize {
    results.iter().filter(|r| r.success).count()
}

pub fn all_successful(results: &[BuildResult]) -> bool {
    results.iter().all(|r| r.success)
}

pub fn failed_targets(results: &[BuildResult]) -> Vec<String> {
    results
        .iter()
        .filter(|r| !r.success)
        .map(|r| r.triple.clone())
        .collect()
}
