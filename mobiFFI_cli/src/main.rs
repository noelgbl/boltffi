mod build;
mod check;
mod commands;
mod config;
mod error;
mod pack;
mod target;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use commands::build::{BuildCommandOptions, BuildPlatform};
use commands::check::CheckOptions;
use commands::generate::{GenerateOptions, GenerateTarget};
use commands::init::InitOptions;
use commands::pack::{PackOptions, PackTarget};
use commands::{run_build, run_check, run_generate, run_init, run_pack};
use config::Config;
use error::{CliError, Result};

#[derive(Parser)]
#[command(name = "mobiFFI")]
#[command(about = "Mobile FFI toolchain for Rust")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        #[arg(long)]
        name: Option<String>,
    },

    Check {
        #[arg(long)]
        fix: bool,

        #[arg(long)]
        ios: bool,

        #[arg(long)]
        android: bool,
    },

    Generate {
        #[arg(value_enum)]
        target: Option<GenerateTargetArg>,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    Build {
        #[arg(value_enum)]
        platform: Option<BuildPlatformArg>,

        #[arg(long)]
        release: bool,
    },

    Pack {
        #[arg(value_enum)]
        target: PackTargetArg,

        #[arg(long)]
        release: bool,

        #[arg(long)]
        version: Option<String>,
    },

    Release {
        #[arg(value_enum)]
        platform: Option<BuildPlatformArg>,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum GenerateTargetArg {
    Swift,
    Kotlin,
    Header,
    All,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum BuildPlatformArg {
    Ios,
    Android,
    Macos,
    All,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PackTargetArg {
    Xcframework,
    Spm,
    Ios,
    Android,
}

fn main() {
    let cli = Cli::parse();

    let result = execute_command(cli.command);

    if let Err(err) = result {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}

fn execute_command(command: Commands) -> Result<()> {
    match command {
        Commands::Init { name } => {
            let options = InitOptions {
                name,
                path: std::env::current_dir().unwrap_or_default(),
            };
            run_init(options).map(|_| ())
        }

        Commands::Check { fix, ios, android } => {
            let options = CheckOptions {
                fix,
                ios: ios || (!ios && !android),
                android: android || (!ios && !android),
            };
            run_check(options).map(|_| ())
        }

        Commands::Generate { target, output } => {
            let config = load_config()?;
            let options = GenerateOptions {
                target: target
                    .map(|t| match t {
                        GenerateTargetArg::Swift => GenerateTarget::Swift,
                        GenerateTargetArg::Kotlin => GenerateTarget::Kotlin,
                        GenerateTargetArg::Header => GenerateTarget::Header,
                        GenerateTargetArg::All => GenerateTarget::All,
                    })
                    .unwrap_or(GenerateTarget::All),
                output,
            };
            run_generate(&config, options)
        }

        Commands::Build { platform, release } => {
            let config = load_config()?;
            let options = BuildCommandOptions {
                platform: platform
                    .map(|p| match p {
                        BuildPlatformArg::Ios => BuildPlatform::Ios,
                        BuildPlatformArg::Android => BuildPlatform::Android,
                        BuildPlatformArg::Macos => BuildPlatform::MacOs,
                        BuildPlatformArg::All => BuildPlatform::All,
                    })
                    .unwrap_or(BuildPlatform::All),
                release,
            };
            run_build(&config, options).map(|_| ())
        }

        Commands::Pack {
            target,
            release,
            version,
        } => {
            let config = load_config()?;
            let options = PackOptions {
                target: match target {
                    PackTargetArg::Xcframework => PackTarget::Xcframework,
                    PackTargetArg::Spm => PackTarget::Spm,
                    PackTargetArg::Ios => PackTarget::Ios,
                    PackTargetArg::Android => PackTarget::Android,
                },
                release,
                version,
            };
            run_pack(&config, options)
        }

        Commands::Release { platform } => {
            let config = load_config()?;
            run_release(&config, platform)
        }
    }
}

fn load_config() -> Result<Config> {
    let config_path = PathBuf::from("mobiFFI.toml");

    if !config_path.exists() {
        return Err(CliError::ConfigNotFound);
    }

    Config::load(&config_path).map_err(Into::into)
}

fn run_release(config: &Config, platform: Option<BuildPlatformArg>) -> Result<()> {
    println!("Running full release pipeline...");
    println!();

    let check_options = CheckOptions {
        fix: false,
        ios: true,
        android: true,
    };

    println!("[1/4] Checking environment...");
    let env_ok = run_check(check_options)?;

    if !env_ok {
        println!("Environment check failed. Run 'mobiFFI check --fix' to install missing targets.");
        return Ok(());
    }
    println!();

    println!("[2/4] Building...");
    let build_options = BuildCommandOptions {
        platform: platform
            .map(|p| match p {
                BuildPlatformArg::Ios => BuildPlatform::Ios,
                BuildPlatformArg::Android => BuildPlatform::Android,
                BuildPlatformArg::Macos => BuildPlatform::MacOs,
                BuildPlatformArg::All => BuildPlatform::All,
            })
            .unwrap_or(BuildPlatform::All),
        release: true,
    };
    run_build(config, build_options)?;
    println!();

    println!("[3/4] Generating bindings...");
    let generate_options = GenerateOptions {
        target: GenerateTarget::All,
        output: None,
    };
    run_generate(config, generate_options)?;
    println!();

    println!("[4/4] Packaging...");

    match platform {
        Some(BuildPlatformArg::Ios) | None => {
            let pack_options = PackOptions {
                target: PackTarget::Ios,
                release: true,
                version: None,
            };
            run_pack(config, pack_options)?;
        }
        Some(BuildPlatformArg::Android) => {
            let pack_options = PackOptions {
                target: PackTarget::Android,
                release: true,
                version: None,
            };
            run_pack(config, pack_options)?;
        }
        _ => {}
    }

    println!();
    println!("Release complete!");

    Ok(())
}
