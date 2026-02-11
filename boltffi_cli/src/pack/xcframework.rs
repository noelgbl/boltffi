use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::error::{CliError, Result};
use crate::target::{BuiltLibrary, Platform};

pub struct XcframeworkBuilder<'a> {
    config: &'a Config,
    libraries: Vec<BuiltLibrary>,
    headers_dir: PathBuf,
    output_dir: PathBuf,
}

pub struct XcframeworkOutput {
    pub xcframework_path: PathBuf,
    pub zip_path: Option<PathBuf>,
    pub checksum: Option<String>,
}

impl<'a> XcframeworkBuilder<'a> {
    pub fn new(config: &'a Config, libraries: Vec<BuiltLibrary>, headers_dir: PathBuf) -> Self {
        Self {
            config,
            libraries,
            headers_dir,
            output_dir: config.apple_xcframework_output(),
        }
    }

    pub fn build(self) -> Result<XcframeworkOutput> {
        std::fs::create_dir_all(&self.output_dir).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: self.output_dir.clone(),
                source,
            }
        })?;

        let device_libs = self.filter_device_libraries();
        let simulator_libs = self.filter_simulator_libraries();
        let macos_libs = self.filter_macos_libraries();

        let fat_sim_lib = self.create_fat_library(&simulator_libs, "ios-simulator-fat")?;
        let fat_macos_lib = self.create_fat_library(&macos_libs, "macos-fat")?;

        let xcframework_path =
            self.create_xcframework(&device_libs, fat_sim_lib.as_ref(), fat_macos_lib.as_ref())?;

        Ok(XcframeworkOutput {
            xcframework_path,
            zip_path: None,
            checksum: None,
        })
    }

    pub fn build_with_zip(self) -> Result<XcframeworkOutput> {
        let mut output = self.build()?;

        let zip_path = output.xcframework_path.with_extension("xcframework.zip");
        create_zip(&output.xcframework_path, &zip_path)?;

        let checksum = compute_checksum(&zip_path)?;

        output.zip_path = Some(zip_path);
        output.checksum = Some(checksum);

        Ok(output)
    }

    fn filter_device_libraries(&self) -> Vec<&BuiltLibrary> {
        self.libraries
            .iter()
            .filter(|lib| lib.target.platform() == Platform::Ios)
            .collect()
    }

    fn filter_simulator_libraries(&self) -> Vec<&BuiltLibrary> {
        self.libraries
            .iter()
            .filter(|lib| lib.target.platform() == Platform::IosSimulator)
            .collect()
    }

    fn filter_macos_libraries(&self) -> Vec<&BuiltLibrary> {
        if !self.config.apple_include_macos() {
            return Vec::new();
        }

        self.libraries
            .iter()
            .filter(|lib| lib.target.platform() == Platform::MacOs)
            .collect()
    }

    fn create_fat_library(
        &self,
        libs: &[&BuiltLibrary],
        output_dir_name: &str,
    ) -> Result<Option<PathBuf>> {
        if libs.is_empty() {
            return Ok(None);
        }

        if libs.len() == 1 {
            return Ok(Some(libs[0].path.clone()));
        }

        let fat_dir = self.output_dir.join(output_dir_name);
        std::fs::create_dir_all(&fat_dir).map_err(|source| CliError::CreateDirectoryFailed {
            path: fat_dir.clone(),
            source,
        })?;

        let lib_name = self.config.library_name();
        let fat_lib_path = fat_dir.join(format!("lib{}.a", lib_name));

        let mut lipo_cmd = Command::new("lipo");
        lipo_cmd.arg("-create");

        libs.iter().for_each(|lib| {
            lipo_cmd.arg(&lib.path);
        });

        lipo_cmd.arg("-output").arg(&fat_lib_path);

        let status = lipo_cmd
            .status()
            .map_err(|source| CliError::LipoFailed { source })?;

        if !status.success() {
            return Err(CliError::CommandFailed {
                command: "lipo".to_string(),
                status: status.code(),
            });
        }

        Ok(Some(fat_lib_path))
    }

    fn create_xcframework(
        &self,
        device_libs: &[&BuiltLibrary],
        fat_sim_lib: Option<&PathBuf>,
        fat_macos_lib: Option<&PathBuf>,
    ) -> Result<PathBuf> {
        let xcframework_name = self.config.xcframework_name();
        let xcframework_path = self
            .output_dir
            .join(format!("{}.xcframework", xcframework_name));

        if xcframework_path.exists() {
            std::fs::remove_dir_all(&xcframework_path).map_err(|source| {
                CliError::CreateDirectoryFailed {
                    path: xcframework_path.clone(),
                    source,
                }
            })?;
        }

        let headers_staging = self.prepare_headers()?;

        let mut xcodebuild_cmd = Command::new("xcodebuild");
        xcodebuild_cmd.arg("-create-xcframework");

        device_libs.iter().for_each(|lib| {
            xcodebuild_cmd
                .arg("-library")
                .arg(&lib.path)
                .arg("-headers")
                .arg(&headers_staging);
        });

        if let Some(sim_lib) = fat_sim_lib {
            xcodebuild_cmd
                .arg("-library")
                .arg(sim_lib)
                .arg("-headers")
                .arg(&headers_staging);
        }

        if let Some(macos_lib) = fat_macos_lib {
            xcodebuild_cmd
                .arg("-library")
                .arg(macos_lib)
                .arg("-headers")
                .arg(&headers_staging);
        }

        xcodebuild_cmd.arg("-output").arg(&xcframework_path);

        let status = xcodebuild_cmd
            .status()
            .map_err(|source| CliError::XcframeworkFailed { source })?;

        if !status.success() {
            return Err(CliError::CommandFailed {
                command: "xcodebuild -create-xcframework".to_string(),
                status: status.code(),
            });
        }

        Ok(xcframework_path)
    }

    fn prepare_headers(&self) -> Result<PathBuf> {
        let headers_staging = self.output_dir.join("headers_staging");

        if headers_staging.exists() {
            std::fs::remove_dir_all(&headers_staging).map_err(|source| {
                CliError::CreateDirectoryFailed {
                    path: headers_staging.clone(),
                    source,
                }
            })?;
        }

        std::fs::create_dir_all(&headers_staging).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: headers_staging.clone(),
                source,
            }
        })?;

        copy_directory_contents(&self.headers_dir, &headers_staging)?;

        let modulemap_content =
            generate_modulemap(&self.config.xcframework_name(), self.config.library_name());
        let modulemap_path = headers_staging.join("module.modulemap");

        std::fs::write(&modulemap_path, modulemap_content).map_err(|source| {
            CliError::WriteFailed {
                path: modulemap_path,
                source,
            }
        })?;

        Ok(headers_staging)
    }
}

fn generate_modulemap(module_name: &str, header_name: &str) -> String {
    format!(
        r#"module {}FFI {{
    header "{}.h"
    export *
}}
"#,
        module_name, header_name
    )
}

fn copy_directory_contents(from: &Path, to: &Path) -> Result<()> {
    walkdir::WalkDir::new(from)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .try_for_each(|entry| {
            let relative = entry.path().strip_prefix(from).unwrap();
            let dest = to.join(relative);

            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|source| {
                    CliError::CreateDirectoryFailed {
                        path: parent.to_path_buf(),
                        source,
                    }
                })?;
            }

            std::fs::copy(entry.path(), &dest).map_err(|source| CliError::CopyFailed {
                from: entry.path().to_path_buf(),
                to: dest,
                source,
            })?;

            Ok(())
        })
}

fn create_zip(source_dir: &Path, zip_path: &Path) -> Result<()> {
    let file = std::fs::File::create(zip_path).map_err(|source| CliError::WriteFailed {
        path: zip_path.to_path_buf(),
        source,
    })?;

    let mut zip_writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    walkdir::WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .try_for_each(|entry| {
            let relative = entry
                .path()
                .strip_prefix(source_dir.parent().unwrap())
                .unwrap();
            let path_string = relative.to_string_lossy().to_string();

            if entry.file_type().is_dir() {
                zip_writer
                    .add_directory(path_string, options)
                    .map_err(|_| CliError::ZipFailed {
                        source: std::io::Error::other("zip dir failed"),
                    })?;
            } else {
                zip_writer
                    .start_file(path_string, options)
                    .map_err(|_| CliError::ZipFailed {
                        source: std::io::Error::other("zip start failed"),
                    })?;

                let content =
                    std::fs::read(entry.path()).map_err(|source| CliError::ReadFailed {
                        path: entry.path().to_path_buf(),
                        source,
                    })?;

                std::io::Write::write_all(&mut zip_writer, &content)
                    .map_err(|source| CliError::ZipFailed { source })?;
            }

            Ok::<_, CliError>(())
        })?;

    zip_writer.finish().map_err(|_| CliError::ZipFailed {
        source: std::io::Error::other("zip finish failed"),
    })?;

    Ok(())
}

pub(crate) fn compute_checksum(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let content = std::fs::read(path).map_err(|source| CliError::ReadFailed {
        path: path.to_path_buf(),
        source,
    })?;

    let hash = Sha256::digest(&content);
    Ok(hex::encode(hash))
}
