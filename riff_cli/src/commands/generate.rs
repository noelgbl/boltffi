use std::path::PathBuf;

use riff_bindgen::{
    CHeaderGenerator, FactoryStyle, JniGenerator, Kotlin, KotlinOptions, Swift, ir, render,
    scan_crate,
};

use crate::config::{Config, FactoryStyle as ConfigFactoryStyle, KotlinApiStyle};
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
    pub use_ir: bool,
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
        GenerateTarget::Swift if options.use_ir => generate_swift_ir(config, options.output),
        GenerateTarget::Swift => generate_swift(config, options.output),
        GenerateTarget::Kotlin if options.use_ir => generate_kotlin_ir(config, options.output),
        GenerateTarget::Kotlin => generate_kotlin(config, options.output),
        GenerateTarget::Header => generate_header(config, options.output),
        GenerateTarget::All => {
            if options.use_ir {
                generate_swift_ir(config, options.output.clone())?;
            } else {
                generate_swift(config, options.output.clone())?;
            }
            if options.use_ir {
                generate_kotlin_ir(config, options.output.clone())?;
            } else {
                generate_kotlin(config, options.output.clone())?;
            }
            generate_header(config, options.output)?;
            Ok(())
        }
    }
}

fn generate_swift(config: &Config, output: Option<PathBuf>) -> Result<()> {
    let output_dir = output.unwrap_or_else(|| config.apple_swift_output());
    let library_name = config.library_name();
    let capitalized = library_name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default()
        + &library_name[1..];
    let output_path = output_dir.join(format!("{}Riff.swift", capitalized));

    std::fs::create_dir_all(&output_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: output_dir.clone(),
        source,
    })?;

    let crate_dir = std::env::current_dir()
        .and_then(|p| p.canonicalize())
        .unwrap_or_else(|_| PathBuf::from("."));
    let crate_name = config.library_name();

    let module = scan_crate(&crate_dir, crate_name).map_err(|e| CliError::CommandFailed {
        command: format!("scan_crate: {}", e),
        status: None,
    })?;

    let ffi_module_name = config
        .apple_swift_ffi_module_name()
        .map(|name| name.to_string())
        .unwrap_or_else(|| format!("{}FFI", config.xcframework_name()));
    let swift_code = Swift::render_module_with_ffi_module_name(&module, Some(ffi_module_name));

    std::fs::write(&output_path, swift_code).map_err(|source| CliError::WriteFailed {
        path: output_path.clone(),
        source,
    })?;

    println!("Generated: {}", output_path.display());
    Ok(())
}

fn generate_swift_ir(config: &Config, output: Option<PathBuf>) -> Result<()> {
    let output_dir = output.unwrap_or_else(|| config.apple_swift_output());
    let library_name = config.library_name();
    let capitalized = library_name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default()
        + &library_name[1..];
    let output_path = output_dir.join(format!("{}Riff.swift", capitalized));

    std::fs::create_dir_all(&output_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: output_dir.clone(),
        source,
    })?;

    let crate_dir = std::env::current_dir()
        .and_then(|p| p.canonicalize())
        .unwrap_or_else(|_| PathBuf::from("."));
    let crate_name = config.library_name();

    let mut module = scan_crate(&crate_dir, crate_name).map_err(|e| CliError::CommandFailed {
        command: format!("scan_crate: {}", e),
        status: None,
    })?;

    let ffi_module_name = config
        .apple_swift_ffi_module_name()
        .map(|name| name.to_string())
        .unwrap_or_else(|| format!("{}FFI", config.xcframework_name()));

    let contract = ir::build_contract(&mut module);
    let abi_contract = ir::Lowerer::new(&contract).to_abi_contract();
    let swift_module = render::swift::SwiftLowerer::new(&contract, &abi_contract).lower();
    let swift_code = render::swift::SwiftEmitter::with_prefix(riff_bindgen::ffi_prefix())
        .with_ffi_module(&ffi_module_name)
        .emit(&swift_module);

    std::fs::write(&output_path, &swift_code).map_err(|source| CliError::WriteFailed {
        path: output_path.clone(),
        source,
    })?;

    println!("Generated (IR): {}", output_path.display());
    Ok(())
}

fn generate_kotlin(config: &Config, output: Option<PathBuf>) -> Result<()> {
    let package_name = config.android_kotlin_package();
    let package_path = package_name.replace('.', "/");

    let output_dir = output.unwrap_or_else(|| config.android_kotlin_output());
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

    let crate_dir = std::env::current_dir()
        .and_then(|p| p.canonicalize())
        .unwrap_or_else(|_| PathBuf::from("."));
    let crate_name = config.library_name();

    let module = scan_crate(&crate_dir, crate_name).map_err(|e| CliError::CommandFailed {
        command: format!("scan_crate: {}", e),
        status: None,
    })?;

    let factory_style = match config.android.kotlin.factory_style {
        ConfigFactoryStyle::Constructors => FactoryStyle::Constructors,
        ConfigFactoryStyle::CompanionMethods => FactoryStyle::CompanionMethods,
    };
    let module_name = config.android_kotlin_module_name();
    let options = KotlinOptions {
        factory_style,
        api_style: match config.android.kotlin.api_style {
            KotlinApiStyle::TopLevel => riff_bindgen::KotlinApiStyle::TopLevel,
            KotlinApiStyle::ModuleObject => riff_bindgen::KotlinApiStyle::ModuleObject,
        },
        module_object_name: Some(module_name.clone()),
        library_name: config
            .android_kotlin_library_name()
            .map(|name| name.to_string()),
    };
    let kotlin_code = Kotlin::render_module_with_options(&module, &package_name, &options);
    let kotlin_path = kotlin_dir.join(format!("{}.kt", module_name));
    std::fs::write(&kotlin_path, kotlin_code).map_err(|source| CliError::WriteFailed {
        path: kotlin_path.clone(),
        source,
    })?;
    println!("Generated: {}", kotlin_path.display());

    let jni_code = JniGenerator::generate_with_class_name(&module, &package_name, &module_name);
    let jni_path = jni_dir.join("jni_glue.c");
    std::fs::write(&jni_path, jni_code).map_err(|source| CliError::WriteFailed {
        path: jni_path.clone(),
        source,
    })?;
    println!("Generated: {}", jni_path.display());

    Ok(())
}

fn generate_kotlin_ir(config: &Config, output: Option<PathBuf>) -> Result<()> {
    let package_name = config.android_kotlin_package();
    let package_path = package_name.replace('.', "/");

    let output_dir = output.unwrap_or_else(|| config.android_kotlin_output());
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

    let crate_dir = std::env::current_dir()
        .and_then(|p| p.canonicalize())
        .unwrap_or_else(|_| PathBuf::from("."));
    let crate_name = config.library_name();

    let mut module = scan_crate(&crate_dir, crate_name).map_err(|e| CliError::CommandFailed {
        command: format!("scan_crate: {}", e),
        status: None,
    })?;

    let factory_style = match config.android.kotlin.factory_style {
        ConfigFactoryStyle::Constructors => FactoryStyle::Constructors,
        ConfigFactoryStyle::CompanionMethods => FactoryStyle::CompanionMethods,
    };
    let module_name = config.android_kotlin_module_name();
    let options = KotlinOptions {
        factory_style,
        api_style: match config.android.kotlin.api_style {
            KotlinApiStyle::TopLevel => riff_bindgen::KotlinApiStyle::TopLevel,
            KotlinApiStyle::ModuleObject => riff_bindgen::KotlinApiStyle::ModuleObject,
        },
        module_object_name: Some(module_name.clone()),
        library_name: config
            .android_kotlin_library_name()
            .map(|name| name.to_string()),
    };
    let kotlin_code = Kotlin::render_module_with_options(&module, &package_name, &options);
    let kotlin_path = kotlin_dir.join(format!("{}.kt", module_name));
    std::fs::write(&kotlin_path, kotlin_code).map_err(|source| CliError::WriteFailed {
        path: kotlin_path.clone(),
        source,
    })?;
    println!("Generated: {}", kotlin_path.display());

    let contract = ir::build_contract(&mut module);
    let abi_contract = ir::Lowerer::new(&contract).to_abi_contract();
    let jni_module =
        render::jni::JniLowerer::new(&contract, &abi_contract, package_name, module_name).lower();
    let jni_code = render::jni::JniEmitter::emit(&jni_module);
    let jni_path = jni_dir.join("jni_glue.c");
    std::fs::write(&jni_path, jni_code).map_err(|source| CliError::WriteFailed {
        path: jni_path.clone(),
        source,
    })?;
    println!("Generated (IR): {}", jni_path.display());

    Ok(())
}

fn generate_header(config: &Config, output: Option<PathBuf>) -> Result<()> {
    let output_dir = output.unwrap_or_else(|| config.apple_header_output());
    let output_path = output_dir.join(format!("{}.h", config.library_name()));

    std::fs::create_dir_all(&output_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: output_dir.clone(),
        source,
    })?;

    let crate_dir = std::env::current_dir()
        .and_then(|p| p.canonicalize())
        .unwrap_or_else(|_| PathBuf::from("."));
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
