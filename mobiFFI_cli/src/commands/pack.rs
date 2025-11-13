use std::path::PathBuf;

use crate::config::Config;
use crate::error::{CliError, Result};
use crate::pack::{AndroidPackager, SpmPackageGenerator, XcframeworkBuilder};
use crate::target::BuiltLibrary;

pub enum PackTarget {
    Xcframework,
    Spm,
    Ios,
    Android,
}

pub struct PackOptions {
    pub target: PackTarget,
    pub release: bool,
    pub version: Option<String>,
}

pub fn run_pack(config: &Config, options: PackOptions) -> Result<()> {
    let target_dir = PathBuf::from("target");
    let libraries = BuiltLibrary::discover(&target_dir, config.library_name(), options.release);

    if libraries.is_empty() {
        return Err(CliError::NoLibrariesFound {
            platform: "any".to_string(),
        });
    }

    match options.target {
        PackTarget::Xcframework => pack_xcframework(config, libraries, options.release),
        PackTarget::Spm => pack_spm(config, libraries, options.release, options.version),
        PackTarget::Ios => {
            pack_xcframework(config, libraries.clone(), options.release)?;
            pack_spm(config, libraries, options.release, options.version)?;
            Ok(())
        }
        PackTarget::Android => pack_android(config, libraries),
    }
}

fn pack_xcframework(config: &Config, libraries: Vec<BuiltLibrary>, release: bool) -> Result<()> {
    let profile = if release { "release" } else { "debug" };

    let ios_libs: Vec<_> = libraries
        .into_iter()
        .filter(|lib| lib.target.platform().is_apple())
        .collect();

    if ios_libs.is_empty() {
        return Err(CliError::NoLibrariesFound {
            platform: format!("iOS ({})", profile),
        });
    }

    println!("Creating XCFramework ({} build)...", profile);

    ios_libs.iter().for_each(|lib| {
        println!("  Found: {} ({})", lib.target.triple(), lib.path.display());
    });

    let headers_dir = PathBuf::from("include");

    if !headers_dir.exists() {
        println!();
        println!("Headers directory not found. Run 'mobiFFI generate header' first.");
        return Err(CliError::NoLibrariesFound {
            platform: "headers".to_string(),
        });
    }

    let builder = XcframeworkBuilder::new(config, ios_libs, headers_dir);
    let output = builder.build_with_zip()?;

    println!();
    println!("Created: {}", output.xcframework_path.display());

    if let Some(zip_path) = &output.zip_path {
        println!("Created: {}", zip_path.display());
    }

    if let Some(checksum) = &output.checksum {
        println!("Checksum: {}", checksum);
    }

    Ok(())
}

fn pack_spm(
    config: &Config,
    libraries: Vec<BuiltLibrary>,
    release: bool,
    version: Option<String>,
) -> Result<()> {
    if !release {
        println!("Warning: SPM packages are typically built from release artifacts");
    }

    let headers_dir = PathBuf::from("include");

    let builder = XcframeworkBuilder::new(config, libraries, headers_dir);
    let xcframework_output = builder.build_with_zip()?;

    let checksum = xcframework_output
        .checksum
        .ok_or_else(|| CliError::NoLibrariesFound {
            platform: "checksum".to_string(),
        })?;

    let version =
        version.unwrap_or_else(|| detect_version().unwrap_or_else(|| "0.1.0".to_string()));

    println!("Generating Package.swift...");

    let generator = SpmPackageGenerator::new(config, checksum.clone(), version.clone());
    let package_path = generator.generate()?;

    println!();
    println!("Created: {}", package_path.display());
    println!("Version: {}", version);
    println!("Checksum: {}", checksum);

    Ok(())
}

fn pack_android(config: &Config, libraries: Vec<BuiltLibrary>) -> Result<()> {
    println!("Creating Android jniLibs...");

    let android_libs: Vec<_> = libraries
        .into_iter()
        .filter(|lib| lib.target.platform() == crate::target::Platform::Android)
        .collect();

    if android_libs.is_empty() {
        return Err(CliError::NoLibrariesFound {
            platform: "Android".to_string(),
        });
    }

    android_libs.iter().for_each(|lib| {
        println!("  Found: {} ({})", lib.target.triple(), lib.path.display());
    });

    let packager = AndroidPackager::new(config, android_libs);
    let output = packager.package()?;

    println!();
    println!("Created: {}", output.jnilibs_path.display());

    output.copied_libraries.iter().for_each(|path| {
        println!("  {}", path.display());
    });

    Ok(())
}

fn detect_version() -> Option<String> {
    std::fs::read_to_string("Cargo.toml")
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|line| line.starts_with("version = "))
                .and_then(|line| {
                    line.split('=')
                        .nth(1)
                        .map(|s| s.trim().trim_matches('"').to_string())
                })
        })
}
