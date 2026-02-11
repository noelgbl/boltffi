use crate::build::{
    BuildOptions, BuildResult, Builder, all_successful, count_successful, failed_targets,
};
use crate::config::Config;
use crate::error::{CliError, Result};

pub enum BuildPlatform {
    Apple,
    Android,
    Wasm,
    All,
}

pub struct BuildCommandOptions {
    pub platform: BuildPlatform,
    pub release: bool,
}

pub fn run_build(config: &Config, options: BuildCommandOptions) -> Result<Vec<BuildResult>> {
    let build_options = BuildOptions {
        release: options.release,
        package: Some(config.library_name().to_string()),
    };

    let builder = Builder::new(config, build_options);

    let profile = if options.release { "release" } else { "debug" };

    let results = match options.platform {
        BuildPlatform::Apple => {
            if !config.is_apple_enabled() {
                return Ok(Vec::new());
            }
            println!("Building for Apple ({})...", profile);
            let mut apple_results = builder.build_ios()?;
            if config.apple_include_macos() {
                apple_results.extend(builder.build_macos()?);
            }
            apple_results
        }
        BuildPlatform::Android => {
            if !config.is_android_enabled() {
                return Ok(Vec::new());
            }
            println!("Building for Android ({})...", profile);
            builder.build_android()?
        }
        BuildPlatform::Wasm => {
            if !config.is_wasm_enabled() {
                return Ok(Vec::new());
            }
            println!("Building for wasm ({})...", profile);
            builder.build_wasm_with_triple(config.wasm_triple())?
        }
        BuildPlatform::All => {
            println!("Building all targets ({})...", profile);
            let mut all_results = Vec::new();
            if config.is_apple_enabled() {
                all_results.extend(builder.build_ios()?);
            }
            if config.is_android_enabled() {
                all_results.extend(builder.build_android()?);
            }
            if config.is_apple_enabled() && config.apple_include_macos() {
                all_results.extend(builder.build_macos()?);
            }
            if config.is_wasm_enabled() {
                all_results.extend(builder.build_wasm_with_triple(config.wasm_triple())?);
            }
            all_results
        }
    };

    if results.is_empty() {
        println!("No enabled targets matched the requested platform");
        return Ok(results);
    }

    print_build_results(&results);

    if all_successful(&results) {
        Ok(results)
    } else {
        Err(CliError::BuildFailed {
            targets: failed_targets(&results),
        })
    }
}

fn print_build_results(results: &[BuildResult]) {
    println!();

    results.iter().for_each(|result| {
        let icon = if result.success { "[ok]" } else { "[failed]" };
        println!("  {} {}", icon, result.triple);
    });

    println!();

    let success_count = count_successful(results);
    let total = results.len();

    if all_successful(results) {
        println!("Built {}/{} targets successfully", success_count, total);
    } else {
        println!(
            "Built {}/{} targets ({} failed)",
            success_count,
            total,
            total - success_count
        );
        println!();
        println!("Failed targets:");
        failed_targets(results).iter().for_each(|triple| {
            println!("  - {}", triple);
        });
    }
}
