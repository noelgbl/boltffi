use std::path::{Path, PathBuf};

use crate::config::{
    AndroidConfig, AndroidKotlinConfig, AndroidPackConfig, AppleConfig, AppleSwiftConfig, Config,
    ErrorStyle, FactoryStyle, HeaderConfig, PackageConfig, SpmConfig, TargetsConfig, WasmConfig,
    XcframeworkConfig,
};
use crate::error::Result;

pub struct InitOptions {
    pub name: Option<String>,
    pub path: PathBuf,
}

pub fn run_init(options: InitOptions) -> Result<PathBuf> {
    let config_path = options.path.join("boltffi.toml");

    if config_path.exists() {
        println!("boltffi.toml already exists");
        return Ok(config_path);
    }

    let package_name = options
        .name
        .or_else(|| detect_package_name(&options.path))
        .unwrap_or_else(|| "mylib".to_string());

    let config = create_default_config(&package_name);
    config.save(&config_path)?;

    println!("Created boltffi.toml");
    println!();
    println!("Next steps:");
    println!("  1. boltffi check     # verify your environment");
    println!("  2. boltffi generate  # generate bindings");
    println!("  3. boltffi build     # compile for targets");
    println!("  4. boltffi pack apple  # package Apple artifacts");

    Ok(config_path)
}

fn detect_package_name(path: &Path) -> Option<String> {
    let cargo_toml = path.join("Cargo.toml");

    if !cargo_toml.exists() {
        return None;
    }

    std::fs::read_to_string(&cargo_toml)
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|line| line.starts_with("name = "))
                .and_then(|line| {
                    line.split('=')
                        .nth(1)
                        .map(|s| s.trim().trim_matches('"').to_string())
                })
        })
}

fn create_default_config(package_name: &str) -> Config {
    let module_name = to_pascal_case(package_name);
    let normalized_kotlin_name = package_name.replace('-', "_");

    Config {
        package: PackageConfig {
            name: package_name.to_string(),
            crate_name: None,
            version: None,
            description: None,
            license: None,
            repository: None,
        },
        targets: TargetsConfig {
            apple: AppleConfig {
                enabled: true,
                output: PathBuf::from("dist/apple"),
                deployment_target: "16.0".to_string(),
                include_macos: false,
                swift: AppleSwiftConfig {
                    module_name: Some(module_name),
                    output: None,
                    ffi_module_name: None,
                    tools_version: Some("5.9".to_string()),
                    error_style: ErrorStyle::default(),
                    type_mappings: Default::default(),
                },
                header: HeaderConfig { output: None },
                xcframework: XcframeworkConfig {
                    output: None,
                    name: None,
                },
                spm: SpmConfig {
                    output: None,
                    distribution: Default::default(),
                    repo_url: None,
                    layout: Default::default(),
                    package_name: None,
                    wrapper_sources: None,
                    skip_package_swift: false,
                },
            },
            android: AndroidConfig {
                enabled: true,
                output: PathBuf::from("dist/android"),
                min_sdk: 24,
                ndk_version: None,
                kotlin: AndroidKotlinConfig {
                    package: Some(format!("com.example.{}", normalized_kotlin_name)),
                    output: None,
                    module_name: None,
                    library_name: None,
                    api_style: Default::default(),
                    error_style: ErrorStyle::default(),
                    factory_style: FactoryStyle::default(),
                    type_mappings: Default::default(),
                },
                header: HeaderConfig { output: None },
                pack: AndroidPackConfig { output: None },
            },
            wasm: WasmConfig::default(),
        },
    }
}

fn to_pascal_case(input: &str) -> String {
    input
        .split(['_', '-'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}
