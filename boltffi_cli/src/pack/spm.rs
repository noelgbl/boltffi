use std::path::PathBuf;

use crate::config::{Config, SpmDistribution, SpmLayout};
use crate::error::{CliError, Result};

pub struct SpmPackageGenerator<'a> {
    config: &'a Config,
    xcframework_name: String,
    checksum: Option<String>,
    version: Option<String>,
    layout: SpmLayout,
}

impl<'a> SpmPackageGenerator<'a> {
    pub fn new_local(config: &'a Config, layout: SpmLayout) -> Self {
        Self {
            config,
            xcframework_name: config.xcframework_name(),
            checksum: None,
            version: None,
            layout,
        }
    }

    pub fn new_remote(
        config: &'a Config,
        checksum: String,
        version: String,
        layout: SpmLayout,
    ) -> Self {
        Self {
            config,
            xcframework_name: config.xcframework_name(),
            checksum: Some(checksum),
            version: Some(version),
            layout,
        }
    }

    pub fn generate(&self) -> Result<PathBuf> {
        let output_path = self.config.apple_spm_output().join("Package.swift");

        let content = match self.config.apple_spm_distribution() {
            SpmDistribution::Local => self.generate_local_package(),
            SpmDistribution::Remote => self.generate_remote_package()?,
        };

        let spm_output = self.config.apple_spm_output();
        std::fs::create_dir_all(&spm_output).map_err(|source| CliError::CreateDirectoryFailed {
            path: spm_output.clone(),
            source,
        })?;

        std::fs::write(&output_path, content).map_err(|source| CliError::WriteFailed {
            path: output_path.clone(),
            source,
        })?;

        Ok(output_path)
    }

    fn generate_local_package(&self) -> String {
        let layout = self.layout;
        let package_name = self.package_name_for_layout(layout);
        let module_name = self.config.swift_module_name();
        let tools_version = self.config.apple_swift_tools_version().unwrap_or("5.9");
        let wrapper_sources = self.wrapper_sources_path(layout);
        let binary_target_name = format!("{}FFI", self.xcframework_name);
        let xcframework_path = self.local_xcframework_path();

        if matches!(layout, SpmLayout::Split) {
            return format!(
                r#"// swift-tools-version:{tools_version}
import PackageDescription

let package = Package(
    name: "{package_name}",
    platforms: [
        .iOS(.{ios_version}),
{macos_platform}
    ],
    products: [
        .library(
            name: "{package_name}",
            targets: ["{binary_target_name}"]
        ),
    ],
    targets: [
        .binaryTarget(
            name: "{binary_target_name}",
            path: "{xcframework_path}"
        ),
    ]
)
"#,
                tools_version = tools_version,
                package_name = package_name,
                ios_version = self.ios_version_for_spm(),
                macos_platform = self.macos_platforms_fragment(),
                binary_target_name = binary_target_name,
                xcframework_path = xcframework_path,
            );
        }

        format!(
            r#"// swift-tools-version:{tools_version}
import PackageDescription

let package = Package(
    name: "{package_name}",
    platforms: [
        .iOS(.{ios_version}),
{macos_platform}
    ],
    products: [
        .library(
            name: "{package_name}",
            targets: ["{module_name}"]
        ),
    ],
    targets: [
        .binaryTarget(
            name: "{binary_target_name}",
            path: "{xcframework_path}"
        ),
        .target(
            name: "{module_name}",
            dependencies: ["{binary_target_name}"],
            path: "{wrapper_sources}"
        ),
    ]
)
"#,
            tools_version = tools_version,
            package_name = package_name,
            module_name = module_name,
            ios_version = self.ios_version_for_spm(),
            macos_platform = self.macos_platforms_fragment(),
            binary_target_name = binary_target_name,
            xcframework_path = xcframework_path,
            wrapper_sources = wrapper_sources,
        )
    }

    fn generate_remote_package(&self) -> Result<String> {
        let layout = self.layout;
        let package_name = self.package_name_for_layout(layout);
        let module_name = self.config.swift_module_name();
        let tools_version = self.config.apple_swift_tools_version().unwrap_or("5.9");
        let repo_url = self
            .config
            .apple_spm_repo_url()
            .unwrap_or("https://github.com/user/repo");
        let wrapper_sources = self.wrapper_sources_path(layout);
        let binary_target_name = format!("{}FFI", self.xcframework_name);
        let checksum = self
            .checksum
            .clone()
            .ok_or_else(|| CliError::CommandFailed {
                command: "missing checksum for remote SPM package".to_string(),
                status: None,
            })?;
        let version = self
            .version
            .clone()
            .ok_or_else(|| CliError::CommandFailed {
                command: "missing version for remote SPM package".to_string(),
                status: None,
            })?;

        if matches!(layout, SpmLayout::Split) {
            return Ok(format!(
                r#"// swift-tools-version:{tools_version}
import PackageDescription

let releaseTag = "{version}"
let releaseChecksum = "{checksum}"

let package = Package(
    name: "{package_name}",
    platforms: [
        .iOS(.{ios_version}),
{macos_platform}
    ],
    products: [
        .library(
            name: "{package_name}",
            targets: ["{binary_target_name}"]
        ),
    ],
    targets: [
        .binaryTarget(
            name: "{binary_target_name}",
            url: "{repo_url}/releases/download/\(releaseTag)/{xcframework_name}.xcframework.zip",
            checksum: releaseChecksum
        ),
    ]
)
"#,
                tools_version = tools_version,
                version = version,
                checksum = checksum,
                package_name = package_name,
                ios_version = self.ios_version_for_spm(),
                macos_platform = self.macos_platforms_fragment(),
                binary_target_name = binary_target_name,
                repo_url = repo_url,
                xcframework_name = self.xcframework_name,
            ));
        }

        Ok(format!(
            r#"// swift-tools-version:{tools_version}
import PackageDescription

let releaseTag = "{version}"
let releaseChecksum = "{checksum}"

let package = Package(
    name: "{package_name}",
    platforms: [
        .iOS(.{ios_version}),
{macos_platform}
    ],
    products: [
        .library(
            name: "{package_name}",
            targets: ["{module_name}"]
        ),
    ],
    targets: [
        .binaryTarget(
            name: "{binary_target_name}",
            url: "{repo_url}/releases/download/\(releaseTag)/{xcframework_name}.xcframework.zip",
            checksum: releaseChecksum
        ),
        .target(
            name: "{module_name}",
            dependencies: ["{binary_target_name}"],
            path: "{wrapper_sources}"
        ),
    ]
)
"#,
            tools_version = tools_version,
            version = version,
            checksum = checksum,
            package_name = package_name,
            module_name = module_name,
            ios_version = self.ios_version_for_spm(),
            repo_url = repo_url,
            xcframework_name = self.xcframework_name,
            binary_target_name = binary_target_name,
            macos_platform = self.macos_platforms_fragment(),
            wrapper_sources = wrapper_sources,
        ))
    }

    fn ios_version_for_spm(&self) -> String {
        let deployment_target = self.config.apple_deployment_target();

        deployment_target
            .split('.')
            .next()
            .map(|major| format!("v{}", major))
            .unwrap_or_else(|| "v16".to_string())
    }

    fn macos_platforms_fragment(&self) -> String {
        if self.config.apple_include_macos() {
            "        .macOS(.v13)\n".to_string()
        } else {
            Default::default()
        }
    }

    fn wrapper_sources_path(&self, layout: SpmLayout) -> String {
        match layout {
            SpmLayout::Bundled => self
                .config
                .apple_spm_wrapper_sources()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "Sources".to_string()),
            SpmLayout::Split | SpmLayout::FfiOnly => "Sources".to_string(),
        }
    }

    fn local_xcframework_path(&self) -> String {
        let package_root = self.config.apple_spm_output();
        let xcframework_path = self
            .config
            .apple_xcframework_output()
            .join(format!("{}.xcframework", self.xcframework_name));
        let rel = relative_path(&package_root, &xcframework_path);
        rel.to_string_lossy().to_string()
    }

    fn package_name_for_layout(&self, layout: SpmLayout) -> String {
        self.config
            .apple_spm_package_name()
            .map(|name| name.to_string())
            .unwrap_or_else(|| match layout {
                SpmLayout::Split => format!("{}FFI", self.config.swift_module_name()),
                SpmLayout::Bundled | SpmLayout::FfiOnly => self.config.swift_module_name(),
            })
    }
}

fn relative_path(from_dir: &std::path::Path, to_path: &std::path::Path) -> PathBuf {
    if from_dir == std::path::Path::new(".") || from_dir == std::path::Path::new("") {
        return to_path.to_path_buf();
    }

    let from_components = from_dir.components().collect::<Vec<_>>();
    let to_components = to_path.components().collect::<Vec<_>>();

    let common_len = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let parent_count = from_components.len().saturating_sub(common_len);
    let parent_prefix = (0..parent_count).map(|_| std::path::Component::ParentDir);
    let suffix = to_components.iter().skip(common_len).copied();

    parent_prefix.chain(suffix).collect()
}
