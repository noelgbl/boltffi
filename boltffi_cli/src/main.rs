mod android;
mod build;
mod check;
mod commands;
mod config;
mod error;
mod pack;
mod reporter;
mod target;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use commands::build::{BuildCommandOptions, BuildPlatform};
use commands::check::CheckOptions;
use commands::doctor::DoctorOptions;
use commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use commands::init::InitOptions;
use commands::pack::{
    PackAllOptions, PackAndroidOptions, PackAppleOptions, PackCommand, PackWasmOptions,
};
use commands::verify::VerifyOptions;
use commands::{run_build, run_check, run_doctor, run_init, run_pack, run_verify};
use config::Config;
use error::{CliError, Result};

#[derive(Parser)]
#[command(name = "boltffi")]
#[command(about = "BoltFFI - Rust FFI toolchain (Apple + Android + WASM)")]
#[command(
    after_help = "Examples:\n  boltffi init\n  boltffi check --apple\n  boltffi generate swift\n  boltffi build apple --release\n  boltffi build wasm --release\n  boltffi pack apple --layout bundled\n  boltffi pack wasm --release\n\nConfig:\n  boltffi reads ./boltffi.toml\n  Settings live under [targets.apple.*], [targets.android.*], [targets.wasm.*]\n"
)]
#[command(version)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count, global = true, help = "Increase verbosity (-v, -vv)")]
    verbose: u8,

    #[arg(short, long, global = true, help = "Suppress all output")]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Create boltffi.toml with sensible defaults")]
    Init {
        #[arg(long)]
        name: Option<String>,
    },

    #[command(
        about = "Check toolchain + required rust targets",
        long_about = "Check toolchain + required rust targets.\n\nIf no platform flags are provided, boltffi checks Apple, Android, and WASM.\n\nExamples:\n  boltffi check\n  boltffi check --apple\n  boltffi check --android\n  boltffi check --wasm\n  boltffi check --fix\n"
    )]
    Check {
        #[arg(long, help = "Install missing rust targets")]
        fix: bool,

        #[arg(long, help = "Check Apple (iOS/iOS-simulator) targets + Xcode tooling")]
        apple: bool,

        #[arg(long, help = "Check Android targets + NDK")]
        android: bool,

        #[arg(long, help = "Check WASM target")]
        wasm: bool,
    },

    #[command(
        about = "Print diagnostic environment info",
        long_about = "Print diagnostic environment info.\n\nExamples:\n  boltffi doctor\n  boltffi doctor --apple\n  boltffi doctor --android\n  boltffi doctor --wasm\n"
    )]
    Doctor {
        #[arg(long)]
        apple: bool,

        #[arg(long)]
        android: bool,

        #[arg(long)]
        wasm: bool,
    },

    #[command(
        about = "Generate bindings (Swift/Kotlin/header)",
        long_about = "Generate bindings.\n\nExamples:\n  boltffi generate\n  boltffi generate swift\n  boltffi generate kotlin\n  boltffi generate header\n"
    )]
    Generate {
        #[arg(value_enum)]
        target: Option<GenerateTargetArg>,

        #[arg(
            short,
            long,
            help = "Override output directory (default comes from boltffi.toml)"
        )]
        output: Option<PathBuf>,
    },

    #[command(
        about = "Build rust libraries for targets",
        long_about = "Build rust libraries for targets.\n\nExamples:\n  boltffi build\n  boltffi build apple\n  boltffi build android --release\n  boltffi build wasm --release\n"
    )]
    Build {
        #[arg(value_enum)]
        platform: Option<BuildPlatformArg>,

        #[arg(long)]
        release: bool,
    },

    #[command(
        about = "Package platform artifacts (xcframework/SPM/jniLibs/npm)",
        long_about = "Package platform artifacts.\n\nExamples:\n  boltffi pack apple\n  boltffi pack apple --layout bundled\n  boltffi pack android --release\n  boltffi pack wasm --release\n"
    )]
    Pack {
        #[command(subcommand)]
        target: PackTargetArg,
    },

    #[command(about = "Run check/build/generate/pack in order")]
    Release {
        #[arg(value_enum)]
        platform: Option<BuildPlatformArg>,
    },

    #[command(about = "Verify a generated binding file")]
    Verify {
        path: PathBuf,

        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum GenerateTargetArg {
    #[value(help = "Generate Swift bindings")]
    Swift,
    #[value(help = "Generate Kotlin bindings + JNI glue")]
    Kotlin,
    #[value(help = "Generate C header")]
    Header,
    #[value(help = "Generate TypeScript bindings for WASM")]
    Typescript,
    #[value(help = "Generate all bindings")]
    All,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum BuildPlatformArg {
    #[value(help = "Build Apple targets (iOS + iOS-simulator, and macOS if enabled)")]
    Apple,
    #[value(help = "Build Android targets")]
    Android,
    #[value(help = "Build wasm target")]
    Wasm,
    #[value(help = "Build all configured targets")]
    All,
}

#[derive(Subcommand)]
enum PackTargetArg {
    #[command(about = "Package all enabled targets")]
    All {
        #[arg(long)]
        release: bool,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,
    },

    #[command(
        about = "Build + package Apple artifacts",
        long_about = "Build + package Apple artifacts.\n\nOutputs:\n  - xcframework: {targets.apple.xcframework.output}/{Name}.xcframework\n  - SwiftPM:      {targets.apple.spm.output}/Package.swift\n\nLayout:\n  bundled  -> one package with wrapper target\n  ffi-only -> standalone FFI package with Swift target\n  split    -> binary-only package (Swift bindings generated to targets.apple.swift.output)\n"
    )]
    Apple {
        #[arg(long)]
        release: bool,

        #[arg(long)]
        version: Option<String>,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,

        #[arg(long)]
        spm_only: bool,

        #[arg(long)]
        xcframework_only: bool,

        #[arg(long, value_enum)]
        layout: Option<PackLayoutArg>,
    },

    #[command(
        about = "Build + package Android artifacts",
        long_about = "Build + package Android artifacts.\n\nOutputs:\n  - Kotlin/JNI: {targets.android.kotlin.output}\n  - jniLibs:    {targets.android.pack.output}\n"
    )]
    Android {
        #[arg(long)]
        release: bool,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,
    },

    #[command(
        about = "Build + package WASM artifacts",
        long_about = "Build + package WASM artifacts.\n\nOutputs:\n  - wasm binary: {targets.wasm.npm.output}/{module_name}_bg.wasm\n  - JS/TS files: {targets.wasm.npm.output}\n  - npm metadata: package.json/README.md (when enabled)\n"
    )]
    Wasm {
        #[arg(long)]
        release: bool,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PackLayoutArg {
    #[value(help = "Single SwiftPM package with wrapper target + generated sources")]
    Bundled,
    #[value(help = "Binary-only SwiftPM package; generate Swift sources outside package")]
    Split,
    #[value(help = "Standalone FFI SwiftPM package (binary target + generated Swift target)")]
    FfiOnly,
}

fn main() {
    let cli = Cli::parse();

    let verbosity = if cli.quiet {
        reporter::Verbosity::Quiet
    } else if cli.verbose > 0 {
        reporter::Verbosity::Verbose
    } else {
        reporter::Verbosity::Normal
    };

    let reporter = reporter::Reporter::new(verbosity);
    let result = execute_command(cli.command, &reporter);

    if let Err(err) = result {
        eprintln!("\n{} {}", console::style("error:").red().bold(), err);
        std::process::exit(1);
    }
}

fn execute_command(command: Commands, reporter: &reporter::Reporter) -> Result<()> {
    match command {
        Commands::Init { name } => {
            let options = InitOptions {
                name,
                path: std::env::current_dir().unwrap_or_default(),
            };
            run_init(options).map(|_| ())
        }

        Commands::Check {
            fix,
            apple,
            android,
            wasm,
        } => {
            let explicit_target_selected = apple || android || wasm;
            let check_wasm = if explicit_target_selected { wasm } else { true };
            let wasm_target_triple = if check_wasm {
                load_config_if_present()?.map(|config| config.wasm_triple().to_string())
            } else {
                None
            };
            let options = CheckOptions {
                fix,
                apple: if explicit_target_selected {
                    apple
                } else {
                    true
                },
                android: if explicit_target_selected {
                    android
                } else {
                    true
                },
                wasm: check_wasm,
                wasm_target_triple,
            };
            run_check(options).map(|_| ())
        }

        Commands::Doctor {
            apple,
            android,
            wasm,
        } => {
            let explicit_target_selected = apple || android || wasm;
            let options = DoctorOptions {
                apple: if explicit_target_selected {
                    apple
                } else {
                    true
                },
                android: if explicit_target_selected {
                    android
                } else {
                    true
                },
                wasm: if explicit_target_selected { wasm } else { true },
            };
            run_doctor(options)
        }

        Commands::Generate { target, output } => {
            let config = load_config()?;
            let options = GenerateOptions {
                target: target
                    .map(|t| match t {
                        GenerateTargetArg::Swift => GenerateTarget::Swift,
                        GenerateTargetArg::Kotlin => GenerateTarget::Kotlin,
                        GenerateTargetArg::Header => GenerateTarget::Header,
                        GenerateTargetArg::Typescript => GenerateTarget::Typescript,
                        GenerateTargetArg::All => GenerateTarget::All,
                    })
                    .unwrap_or(GenerateTarget::All),
                output,
            };
            run_generate_with_output(&config, options)
        }

        Commands::Build { platform, release } => {
            let config = load_config()?;
            let options = BuildCommandOptions {
                platform: platform
                    .map(|p| match p {
                        BuildPlatformArg::Apple => BuildPlatform::Apple,
                        BuildPlatformArg::Android => BuildPlatform::Android,
                        BuildPlatformArg::Wasm => BuildPlatform::Wasm,
                        BuildPlatformArg::All => BuildPlatform::All,
                    })
                    .unwrap_or(BuildPlatform::All),
                release,
            };
            run_build(&config, options).map(|_| ())
        }

        Commands::Pack { target } => {
            let config = load_config()?;
            let command = match target {
                PackTargetArg::All {
                    release,
                    regenerate,
                    no_build,
                } => PackCommand::All(PackAllOptions {
                    release,
                    regenerate,
                    no_build,
                }),
                PackTargetArg::Apple {
                    release,
                    version,
                    regenerate,
                    no_build,
                    spm_only,
                    xcframework_only,
                    layout,
                } => PackCommand::Apple(PackAppleOptions {
                    release,
                    version,
                    regenerate,
                    no_build,
                    spm_only,
                    xcframework_only,
                    layout: layout.map(|l| match l {
                        PackLayoutArg::Bundled => crate::config::SpmLayout::Bundled,
                        PackLayoutArg::Split => crate::config::SpmLayout::Split,
                        PackLayoutArg::FfiOnly => crate::config::SpmLayout::FfiOnly,
                    }),
                }),
                PackTargetArg::Android {
                    release,
                    regenerate,
                    no_build,
                } => PackCommand::Android(PackAndroidOptions {
                    release,
                    regenerate,
                    no_build,
                }),
                PackTargetArg::Wasm {
                    release,
                    regenerate,
                    no_build,
                } => PackCommand::Wasm(PackWasmOptions {
                    release,
                    regenerate,
                    no_build,
                }),
            };
            run_pack(&config, command, reporter)
        }

        Commands::Release { platform } => {
            let config = load_config()?;
            run_release(&config, platform, reporter)
        }

        Commands::Verify { path, json } => {
            let options = VerifyOptions { path, json };
            run_verify(options).map(|verified| {
                if !verified {
                    std::process::exit(1);
                }
            })
        }
    }
}

fn load_config() -> Result<Config> {
    let config_path = PathBuf::from("boltffi.toml");

    if !config_path.exists() {
        return Err(CliError::ConfigNotFound);
    }

    Config::load(&config_path).map_err(Into::into)
}

fn load_config_if_present() -> Result<Option<Config>> {
    let config_path = PathBuf::from("boltffi.toml");

    if !config_path.exists() {
        return Ok(None);
    }

    Config::load(&config_path).map(Some).map_err(Into::into)
}

fn run_release(
    config: &Config,
    platform: Option<BuildPlatformArg>,
    reporter: &reporter::Reporter,
) -> Result<()> {
    reporter.section("🚀", "Running full release pipeline");

    let check_options = CheckOptions {
        fix: false,
        apple: config.is_apple_enabled(),
        android: config.is_android_enabled(),
        wasm: config.is_wasm_enabled(),
        wasm_target_triple: Some(config.wasm_triple().to_string()),
    };

    println!("[1/4] Checking environment...");
    let env_ok = run_check(check_options)?;

    if !env_ok {
        println!("Environment check failed. Run 'boltffi check --fix' to install missing targets.");
        return Ok(());
    }
    println!();

    println!("[2/4] Building...");
    let build_options = BuildCommandOptions {
        platform: platform
            .map(|p| match p {
                BuildPlatformArg::Apple => BuildPlatform::Apple,
                BuildPlatformArg::Android => BuildPlatform::Android,
                BuildPlatformArg::Wasm => BuildPlatform::Wasm,
                BuildPlatformArg::All => BuildPlatform::All,
            })
            .unwrap_or(BuildPlatform::All),
        release: true,
    };
    run_build(config, build_options)?;
    println!();

    println!("[3/4] Generating bindings...");
    run_generate_with_output(
        config,
        GenerateOptions {
            target: GenerateTarget::All,
            output: None,
        },
    )?;
    println!();

    println!("[4/4] Packaging...");

    match platform {
        Some(BuildPlatformArg::Apple) => {
            if config.is_apple_enabled() {
                run_pack(
                    config,
                    PackCommand::Apple(PackAppleOptions {
                        release: true,
                        version: None,
                        regenerate: false,
                        no_build: true,
                        spm_only: false,
                        xcframework_only: false,
                        layout: None,
                    }),
                    reporter,
                )?;
            }
        }
        Some(BuildPlatformArg::Android) => {
            if config.is_android_enabled() {
                run_pack(
                    config,
                    PackCommand::Android(PackAndroidOptions {
                        release: true,
                        regenerate: false,
                        no_build: true,
                    }),
                    reporter,
                )?;
            }
        }
        Some(BuildPlatformArg::Wasm) => {
            if config.is_wasm_enabled() {
                run_pack(
                    config,
                    PackCommand::Wasm(PackWasmOptions {
                        release: true,
                        regenerate: false,
                        no_build: true,
                    }),
                    reporter,
                )?;
            }
        }
        Some(BuildPlatformArg::All) | None => {
            if config.is_apple_enabled() {
                run_pack(
                    config,
                    PackCommand::Apple(PackAppleOptions {
                        release: true,
                        version: None,
                        regenerate: false,
                        no_build: true,
                        spm_only: false,
                        xcframework_only: false,
                        layout: None,
                    }),
                    reporter,
                )?;
            }
            if config.is_android_enabled() {
                run_pack(
                    config,
                    PackCommand::Android(PackAndroidOptions {
                        release: true,
                        regenerate: false,
                        no_build: true,
                    }),
                    reporter,
                )?;
            }
            if config.is_wasm_enabled() {
                run_pack(
                    config,
                    PackCommand::Wasm(PackWasmOptions {
                        release: true,
                        regenerate: false,
                        no_build: true,
                    }),
                    reporter,
                )?;
            }
        }
    }

    println!();
    println!("Release complete!");

    Ok(())
}
