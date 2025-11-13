use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::{CliError, Result};

pub struct SpmPackageGenerator<'a> {
    config: &'a Config,
    xcframework_name: String,
    checksum: String,
    version: String,
}

impl<'a> SpmPackageGenerator<'a> {
    pub fn new(config: &'a Config, checksum: String, version: String) -> Self {
        Self {
            config,
            xcframework_name: config.xcframework_name(),
            checksum,
            version,
        }
    }

    pub fn generate(&self) -> Result<PathBuf> {
        let output_path = self.config.pack.spm.output.join("Package.swift");

        let content = self.generate_package_swift();

        std::fs::create_dir_all(&self.config.pack.spm.output).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: self.config.pack.spm.output.clone(),
                source,
            }
        })?;

        std::fs::write(&output_path, content).map_err(|source| CliError::WriteFailed {
            path: output_path.clone(),
            source,
        })?;

        Ok(output_path)
    }

    fn generate_package_swift(&self) -> String {
        let package_name = &self.config.package.name;
        let module_name = self.config.swift_module_name();
        let tools_version = self.config.swift.tools_version.as_deref().unwrap_or("5.9");
        let repo_url = self
            .config
            .pack
            .spm
            .repo_url
            .as_deref()
            .unwrap_or("https://github.com/user/repo");

        format!(
            r#"// swift-tools-version:{tools_version}
import PackageDescription

let releaseTag = "{version}"
let releaseChecksum = "{checksum}"

let package = Package(
    name: "{package_name}",
    platforms: [
        .iOS(.{ios_version}),
        .macOS(.v13)
    ],
    products: [
        .library(
            name: "{module_name}",
            targets: ["{module_name}"]
        ),
    ],
    targets: [
        .binaryTarget(
            name: "{module_name}",
            url: "{repo_url}/releases/download/\(releaseTag)/{xcframework_name}.xcframework.zip",
            checksum: releaseChecksum
        ),
    ]
)
"#,
            tools_version = tools_version,
            version = self.version,
            checksum = self.checksum,
            package_name = package_name,
            module_name = module_name,
            ios_version = self.ios_version_for_spm(),
            repo_url = repo_url,
            xcframework_name = self.xcframework_name,
        )
    }

    fn ios_version_for_spm(&self) -> String {
        let deployment_target = &self.config.ios.deployment_target;

        deployment_target
            .split('.')
            .next()
            .map(|major| format!("v{}", major))
            .unwrap_or_else(|| "v16".to_string())
    }
}

pub fn update_existing_package_swift(
    package_path: &Path,
    version: &str,
    checksum: &str,
) -> Result<()> {
    let content = std::fs::read_to_string(package_path).map_err(|source| CliError::ReadFailed {
        path: package_path.to_path_buf(),
        source,
    })?;

    let updated = content
        .lines()
        .map(|line| {
            if line.starts_with("let releaseTag = ") {
                format!(r#"let releaseTag = "{}""#, version)
            } else if line.starts_with("let releaseChecksum = ") {
                format!(r#"let releaseChecksum = "{}""#, checksum)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    std::fs::write(package_path, updated).map_err(|source| CliError::WriteFailed {
        path: package_path.to_path_buf(),
        source,
    })?;

    Ok(())
}
