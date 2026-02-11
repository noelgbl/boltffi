use std::path::PathBuf;

use boltffi_bindgen::render::typescript::{TypeScriptEmitter, TypeScriptLowerer};
use boltffi_bindgen::{
    CHeaderGenerator, FactoryStyle, KotlinOptions, TypeConversion as BindgenTypeConversion,
    TypeMapping as BindgenTypeMapping, TypeMappings, ir, render, scan_crate,
};

use crate::config::{
    Config, FactoryStyle as ConfigFactoryStyle, KotlinApiStyle,
    TypeConversion as ConfigTypeConversion,
};
use crate::error::{CliError, Result};

pub enum GenerateTarget {
    Swift,
    Kotlin,
    Header,
    Typescript,
    All,
}

pub struct GenerateOptions {
    pub target: GenerateTarget,
    pub output: Option<PathBuf>,
}

pub fn run_generate_with_output(config: &Config, options: GenerateOptions) -> Result<()> {
    match options.target {
        GenerateTarget::Swift => generate_swift(config, options.output),
        GenerateTarget::Kotlin => generate_kotlin(config, options.output),
        GenerateTarget::Header => generate_header(config, options.output),
        GenerateTarget::Typescript => generate_typescript(config, options.output),
        GenerateTarget::All => {
            if config.is_apple_enabled() {
                generate_swift(config, options.output.clone())?;
            }
            if config.is_android_enabled() {
                generate_kotlin(config, options.output.clone())?;
            }
            if config.is_apple_enabled() || config.is_android_enabled() {
                generate_header(config, options.output.clone())?;
            }
            if config.is_wasm_enabled() {
                generate_typescript(config, options.output)?;
            }
            Ok(())
        }
    }
}

fn convert_type_mappings(
    config_mappings: &std::collections::HashMap<String, crate::config::TypeMapping>,
) -> TypeMappings {
    config_mappings
        .iter()
        .map(|(name, mapping)| {
            let conversion = match mapping.conversion {
                ConfigTypeConversion::UuidString => BindgenTypeConversion::UuidString,
                ConfigTypeConversion::UrlString => BindgenTypeConversion::UrlString,
            };
            (
                name.clone(),
                BindgenTypeMapping {
                    native_type: mapping.native_type.clone(),
                    conversion,
                },
            )
        })
        .collect()
}

fn generate_swift(config: &Config, output: Option<PathBuf>) -> Result<()> {
    if !config.is_apple_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.apple.enabled = false".to_string(),
            status: None,
        });
    }

    let output_dir = output.unwrap_or_else(|| config.apple_swift_output());
    let library_name = config.library_name();
    let capitalized = library_name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default()
        + &library_name[1..];
    let output_path = output_dir.join(format!("{}BoltFFI.swift", capitalized));

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

    let type_mappings = convert_type_mappings(config.swift_type_mappings());

    let contract = ir::build_contract(&mut module);
    let abi_contract = ir::Lowerer::new(&contract).to_abi_contract();
    let swift_module = render::swift::SwiftLowerer::new(&contract, &abi_contract)
        .with_type_mappings(type_mappings)
        .lower();
    let swift_code = render::swift::SwiftEmitter::with_prefix(boltffi_bindgen::ffi_prefix())
        .with_ffi_module(&ffi_module_name)
        .emit(&swift_module);

    std::fs::write(&output_path, &swift_code).map_err(|source| CliError::WriteFailed {
        path: output_path.clone(),
        source,
    })?;

    println!("Generated: {}", output_path.display());
    Ok(())
}

fn generate_kotlin(config: &Config, output: Option<PathBuf>) -> Result<()> {
    if !config.is_android_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.android.enabled = false".to_string(),
            status: None,
        });
    }

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

    let factory_style = match config.android_kotlin_factory_style() {
        ConfigFactoryStyle::Constructors => FactoryStyle::Constructors,
        ConfigFactoryStyle::CompanionMethods => FactoryStyle::CompanionMethods,
    };
    let module_name = config.android_kotlin_module_name();
    let kotlin_options = KotlinOptions {
        factory_style,
        api_style: match config.android_kotlin_api_style() {
            KotlinApiStyle::TopLevel => boltffi_bindgen::KotlinApiStyle::TopLevel,
            KotlinApiStyle::ModuleObject => boltffi_bindgen::KotlinApiStyle::ModuleObject,
        },
        module_object_name: Some(module_name.clone()),
        library_name: config
            .android_kotlin_library_name()
            .map(|name| name.to_string()),
    };

    let type_mappings = convert_type_mappings(config.kotlin_type_mappings());

    let contract = ir::build_contract(&mut module);
    let abi_contract = ir::Lowerer::new(&contract).to_abi_contract();

    let kotlin_module = render::kotlin::KotlinLowerer::new(
        &contract,
        &abi_contract,
        package_name.clone(),
        module_name.clone(),
        kotlin_options,
    )
    .with_type_mappings(type_mappings)
    .lower();
    let kotlin_code = render::kotlin::KotlinEmitter::emit(&kotlin_module);
    let kotlin_path = kotlin_dir.join(format!("{}.kt", module_name));
    std::fs::write(&kotlin_path, &kotlin_code).map_err(|source| CliError::WriteFailed {
        path: kotlin_path.clone(),
        source,
    })?;
    println!("Generated: {}", kotlin_path.display());

    let jni_module =
        render::jni::JniLowerer::new(&contract, &abi_contract, package_name, module_name).lower();
    let jni_code = render::jni::JniEmitter::emit(&jni_module);
    let jni_path = jni_dir.join("jni_glue.c");
    std::fs::write(&jni_path, &jni_code).map_err(|source| CliError::WriteFailed {
        path: jni_path.clone(),
        source,
    })?;
    println!("Generated: {}", jni_path.display());

    Ok(())
}

fn generate_header(config: &Config, output: Option<PathBuf>) -> Result<()> {
    if !config.is_apple_enabled() && !config.is_android_enabled() {
        return Err(CliError::CommandFailed {
            command: "both targets.apple.enabled and targets.android.enabled are false".to_string(),
            status: None,
        });
    }

    let output_dir = output.unwrap_or_else(|| {
        if config.is_apple_enabled() {
            config.apple_header_output()
        } else {
            config.android_header_output()
        }
    });
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

fn generate_typescript(config: &Config, output: Option<PathBuf>) -> Result<()> {
    if !config.is_wasm_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.wasm.enabled = false".to_string(),
            status: None,
        });
    }

    let output_dir = output.unwrap_or_else(|| config.wasm_typescript_output());
    let output_path = output_dir.join(format!("{}.ts", config.wasm_typescript_module_name()));

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

    let contract = ir::build_contract(&mut module);
    let abi_contract = ir::Lowerer::new(&contract).to_abi_contract();

    let ts_module =
        TypeScriptLowerer::new(&contract, &abi_contract, crate_name.to_string()).lower();
    let runtime_package = config.wasm_runtime_package();
    let ts_code = TypeScriptEmitter::emit(&ts_module).replacen(
        "from \"@boltffi/runtime\"",
        &format!("from \"{}\"", runtime_package),
        1,
    );

    std::fs::write(&output_path, &ts_code).map_err(|source| CliError::WriteFailed {
        path: output_path.clone(),
        source,
    })?;

    println!("Generated: {}", output_path.display());
    Ok(())
}
