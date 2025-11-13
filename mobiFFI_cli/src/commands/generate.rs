use std::path::PathBuf;
use std::process::Command;

use crate::config::Config;
use crate::error::{CliError, Result};

pub enum GenerateTarget {
    Swift,
    Kotlin,
    Header,
    All,
}

pub struct GenerateOptions {
    pub target: GenerateTarget,
    pub output: Option<PathBuf>,
}

pub fn run_generate(config: &Config, options: GenerateOptions) -> Result<()> {
    match options.target {
        GenerateTarget::Swift => generate_swift(config, options.output),
        GenerateTarget::Kotlin => generate_kotlin(config, options.output),
        GenerateTarget::Header => generate_header(config, options.output),
        GenerateTarget::All => {
            generate_swift(config, None)?;
            generate_kotlin(config, None)?;
            generate_header(config, None)?;
            Ok(())
        }
    }
}

fn generate_swift(config: &Config, output: Option<PathBuf>) -> Result<()> {
    let output_path = output
        .map(|dir| dir.join("Generated.swift"))
        .unwrap_or_else(|| config.swift.output.join("Generated.swift"));

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| CliError::CreateDirectoryFailed {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let crate_path = config.library_name();

    let status = Command::new("cargo")
        .args(["run", "-p", "mobiFFI_bindgen", "--"])
        .arg(crate_path)
        .arg(&output_path)
        .status()
        .map_err(|_| CliError::CommandFailed {
            command: "mobiFFI_bindgen".to_string(),
            status: None,
        })?;

    if !status.success() {
        return Err(CliError::CommandFailed {
            command: "mobiFFI_bindgen".to_string(),
            status: status.code(),
        });
    }

    println!("Generated Swift bindings -> {}", output_path.display());
    Ok(())
}

fn generate_kotlin(_config: &Config, _output: Option<PathBuf>) -> Result<()> {
    println!("Kotlin generation not yet implemented");
    Ok(())
}

fn generate_header(_config: &Config, _output: Option<PathBuf>) -> Result<()> {
    println!("Header generation via CLI not yet implemented");
    println!("Use: cargo build -p mobiFFI_core (headers generated via build.rs)");
    Ok(())
}
