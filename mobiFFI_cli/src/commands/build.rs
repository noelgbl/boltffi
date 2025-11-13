use crate::build::{
    all_successful, count_successful, failed_targets, BuildOptions, BuildResult, Builder,
};
use crate::config::Config;
use crate::error::Result;

pub enum BuildPlatform {
    Ios,
    Android,
    MacOs,
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
        BuildPlatform::Ios => {
            println!("Building for iOS ({})...", profile);
            builder.build_ios()
        }
        BuildPlatform::Android => {
            println!("Building for Android ({})...", profile);
            builder.build_android()
        }
        BuildPlatform::MacOs => {
            println!("Building for macOS ({})...", profile);
            builder.build_macos()
        }
        BuildPlatform::All => {
            println!("Building all targets ({})...", profile);
            let mut all_results = builder.build_ios();
            all_results.extend(builder.build_android());
            if config.ios.include_macos {
                all_results.extend(builder.build_macos());
            }
            all_results
        }
    };

    print_build_results(&results);

    Ok(results)
}

fn print_build_results(results: &[BuildResult]) {
    println!();

    results.iter().for_each(|result| {
        let icon = if result.success { "[ok]" } else { "[failed]" };
        println!("  {} {}", icon, result.target.triple());
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
        failed_targets(results).iter().for_each(|target| {
            println!("  - {}", target.triple());
        });
    }
}
