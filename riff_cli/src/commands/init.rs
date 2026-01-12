use std::path::{Path, PathBuf};

use crate::config::{
    AndroidConfig, Config, ErrorStyle, FactoryStyle, IosConfig, KotlinConfig, PackConfig, PackageConfig, SwiftConfig,
};
use crate::error::Result;

pub struct InitOptions {
    pub name: Option<String>,
    pub path: PathBuf,
}

pub fn run_init(options: InitOptions) -> Result<PathBuf> {
    let config_path = options.path.join("riff.toml");

    if config_path.exists() {
        println!("riff.toml already exists");
        return Ok(config_path);
    }

    let package_name = options
        .name
        .or_else(|| detect_package_name(&options.path))
        .unwrap_or_else(|| "mylib".to_string());

    let config = create_default_config(&package_name);
    config.save(&config_path)?;

    println!("Created riff.toml");
    println!();
    println!("Next steps:");
    println!("  1. riff check     # verify your environment");
    println!("  2. riff generate  # generate bindings");
    println!("  3. riff build     # compile for targets");

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

    Config {
        package: PackageConfig {
            name: package_name.to_string(),
            crate_name: None,
        },
        swift: SwiftConfig {
            module_name: Some(module_name.clone()),
            output: PathBuf::from("bindings/swift"),
            tools_version: Some("5.9".to_string()),
            error_style: ErrorStyle::default(),
        },
        kotlin: KotlinConfig {
            package: format!("com.example.{}", package_name.replace('-', "_")),
            output: PathBuf::from("bindings/kotlin"),
            error_style: ErrorStyle::default(),
            factory_style: FactoryStyle::default(),
        },
        ios: IosConfig {
            deployment_target: "16.0".to_string(),
            include_macos: false,
        },
        android: AndroidConfig {
            min_sdk: 24,
            ndk_version: None,
        },
        pack: PackConfig::default(),
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
