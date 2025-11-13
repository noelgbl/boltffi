use std::process::Command;

use crate::config::Config;
use crate::error::{CliError, Result};
use crate::target::RustTarget;

pub struct BuildOptions {
    pub release: bool,
    pub package: Option<String>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            release: false,
            package: None,
        }
    }
}

pub struct Builder<'a> {
    config: &'a Config,
    options: BuildOptions,
}

pub struct BuildResult {
    pub target: RustTarget,
    pub success: bool,
}

impl<'a> Builder<'a> {
    pub fn new(config: &'a Config, options: BuildOptions) -> Self {
        Self { config, options }
    }

    pub fn build_targets(&self, targets: &[RustTarget]) -> Vec<BuildResult> {
        targets
            .iter()
            .map(|target| self.build_single_target(target))
            .collect()
    }

    pub fn build_ios(&self) -> Vec<BuildResult> {
        self.build_targets(RustTarget::ALL_IOS)
    }

    pub fn build_android(&self) -> Vec<BuildResult> {
        self.build_targets(RustTarget::ALL_ANDROID)
    }

    pub fn build_macos(&self) -> Vec<BuildResult> {
        self.build_targets(RustTarget::ALL_MACOS)
    }

    fn build_single_target(&self, target: &RustTarget) -> BuildResult {
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

        let success = cmd.status().map(|status| status.success()).unwrap_or(false);

        BuildResult {
            target: target.clone(),
            success,
        }
    }
}

pub fn build_single(target: &RustTarget, package: &str, release: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build");

    if release {
        cmd.arg("--release");
    }

    cmd.arg("--target").arg(target.triple());
    cmd.arg("-p").arg(package);

    let status = cmd.status().map_err(|_| CliError::CommandFailed {
        command: format!("cargo build --target {}", target.triple()),
        status: None,
    })?;

    if !status.success() {
        return Err(CliError::CommandFailed {
            command: format!("cargo build --target {}", target.triple()),
            status: status.code(),
        });
    }

    Ok(())
}

pub fn count_successful(results: &[BuildResult]) -> usize {
    results.iter().filter(|r| r.success).count()
}

pub fn all_successful(results: &[BuildResult]) -> bool {
    results.iter().all(|r| r.success)
}

pub fn failed_targets(results: &[BuildResult]) -> Vec<&RustTarget> {
    results
        .iter()
        .filter(|r| !r.success)
        .map(|r| &r.target)
        .collect()
}
