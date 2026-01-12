use std::path::PathBuf;

use riff_bindgen::{CHeaderGenerator, FactoryStyle, JniGenerator, Kotlin, KotlinOptions, Swift, scan_crate};

use crate::config::{Config, FactoryStyle as ConfigFactoryStyle};
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

pub fn run_generate(config: &Config, target: GenerateTarget) -> Result<()> {
    match target {
        GenerateTarget::Swift => generate_swift(config, None),
        GenerateTarget::Kotlin => generate_kotlin(config, None),
        GenerateTarget::Header => generate_header(config, None),
        GenerateTarget::All => {
            generate_swift(config, None)?;
            generate_kotlin(config, None)?;
            generate_header(config, None)?;
            Ok(())
        }
    }
}

pub fn run_generate_with_output(config: &Config, options: GenerateOptions) -> Result<()> {
    match options.target {
        GenerateTarget::Swift => generate_swift(config, options.output),
        GenerateTarget::Kotlin => generate_kotlin(config, options.output),
        GenerateTarget::Header => generate_header(config, options.output),
        GenerateTarget::All => {
            generate_swift(config, options.output.clone())?;
            generate_kotlin(config, options.output.clone())?;
            generate_header(config, options.output)?;
            Ok(())
        }
    }
}

fn generate_swift(config: &Config, output: Option<PathBuf>) -> Result<()> {
    let output_dir = output.unwrap_or_else(|| PathBuf::from("dist/Sources"));
    let output_path = output_dir.join(format!("{}.swift", config.swift_module_name()));

    std::fs::create_dir_all(&output_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: output_dir.clone(),
        source,
    })?;

    let crate_dir = PathBuf::from(".");
    let crate_name = config.library_name();

    let module = scan_crate(&crate_dir, crate_name).map_err(|e| CliError::CommandFailed {
        command: format!("scan_crate: {}", e),
        status: None,
    })?;

    let swift_code = Swift::render_module(&module);

    std::fs::write(&output_path, swift_code).map_err(|source| CliError::WriteFailed {
        path: output_path.clone(),
        source,
    })?;

    println!("Generated: {}", output_path.display());
    Ok(())
}

fn generate_kotlin(config: &Config, output: Option<PathBuf>) -> Result<()> {
    let package_name = config
        .kotlin_package()
        .unwrap_or_else(|| "com.example".to_string());
    let package_path = package_name.replace('.', "/");

    let output_dir = output.unwrap_or_else(|| PathBuf::from("dist/kotlin"));
    let kotlin_dir = output_dir.join(&package_path);
    let jni_dir = output_dir.join("jni");

    std::fs::create_dir_all(&kotlin_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: kotlin_dir.clone(),
        source,
    })?;
    std::fs::create_dir_all(&jni_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: jni_dir.clone(),
        source,
    })?;

    let crate_dir = PathBuf::from(".");
    let crate_name = config.library_name();

    let module = scan_crate(&crate_dir, crate_name).map_err(|e| CliError::CommandFailed {
        command: format!("scan_crate: {}", e),
        status: None,
    })?;

    let factory_style = match config.kotlin.factory_style {
        ConfigFactoryStyle::Constructors => FactoryStyle::Constructors,
        ConfigFactoryStyle::CompanionMethods => FactoryStyle::CompanionMethods,
    };
    let options = KotlinOptions { factory_style };
    let kotlin_code = Kotlin::render_module_with_options(&module, &package_name, &options);
    let kotlin_path = kotlin_dir.join(format!("{}.kt", config.kotlin_class_name()));
    std::fs::write(&kotlin_path, kotlin_code).map_err(|source| CliError::WriteFailed {
        path: kotlin_path.clone(),
        source,
    })?;
    println!("Generated: {}", kotlin_path.display());

    let jni_code = JniGenerator::generate(&module, &package_name);
    let jni_path = jni_dir.join("jni_glue.c");
    std::fs::write(&jni_path, jni_code).map_err(|source| CliError::WriteFailed {
        path: jni_path.clone(),
        source,
    })?;
    println!("Generated: {}", jni_path.display());

    Ok(())
}

fn generate_header(config: &Config, output: Option<PathBuf>) -> Result<()> {
    let output_dir = output.unwrap_or_else(|| PathBuf::from("dist/include"));
    let output_path = output_dir.join(format!("{}.h", config.library_name()));

    std::fs::create_dir_all(&output_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: output_dir.clone(),
        source,
    })?;

    let crate_dir = PathBuf::from(".");
    let crate_name = config.library_name();

    let module = scan_crate(&crate_dir, crate_name).map_err(|e| CliError::CommandFailed {
        command: format!("scan_crate: {}", e),
        status: None,
    })?;

    let header_code = CHeaderGenerator::generate(&module);

    std::fs::write(&output_path, header_code).map_err(|source| CliError::WriteFailed {
        path: output_path.clone(),
        source,
    })?;

    println!("Generated: {}", output_path.display());
    Ok(())
}
