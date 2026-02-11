use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::build::{BuildOptions, Builder, all_successful, failed_targets};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::config::{
    Config, SpmDistribution, SpmLayout, WasmNpmTarget, WasmOptimizeLevel, WasmOptimizeOnMissing,
    WasmProfile,
};
use crate::error::{CliError, Result};
use crate::pack::{AndroidPackager, SpmPackageGenerator, XcframeworkBuilder, compute_checksum};
use crate::target::{BuiltLibrary, Platform};

pub enum PackCommand {
    Apple(PackAppleOptions),
    Android(PackAndroidOptions),
    Wasm(PackWasmOptions),
}

pub struct PackAppleOptions {
    pub release: bool,
    pub version: Option<String>,
    pub regenerate: bool,
    pub no_build: bool,
    pub spm_only: bool,
    pub xcframework_only: bool,
    pub layout: Option<SpmLayout>,
}

pub struct PackAndroidOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
}

pub struct PackWasmOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
}

pub fn run_pack(config: &Config, command: PackCommand) -> Result<()> {
    match command {
        PackCommand::Apple(options) => pack_apple(config, options),
        PackCommand::Android(options) => pack_android(config, options),
        PackCommand::Wasm(options) => pack_wasm(config, options),
    }
}

fn pack_apple(config: &Config, options: PackAppleOptions) -> Result<()> {
    if !config.is_apple_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.apple.enabled = false".to_string(),
            status: None,
        });
    }

    if !config.apple_include_macos() {
        println!("macOS excluded because targets.apple.include_macos = false");
    }

    if options.spm_only && options.xcframework_only {
        return Err(CliError::CommandFailed {
            command: "cannot combine --spm-only and --xcframework-only".to_string(),
            status: None,
        });
    }

    if !options.no_build {
        run_step("Building Apple targets", || {
            build_apple_targets(config, options.release)
        })?;
    }

    let layout = options.layout.unwrap_or_else(|| config.apple_spm_layout());
    let package_root = config.apple_spm_output();

    if options.regenerate {
        run_step("Generating Apple bindings", || {
            generate_apple_bindings(config, layout, &package_root)
        })?;
    }

    let libraries = discover_built_libraries(&config.crate_artifact_name(), options.release);
    let apple_libraries: Vec<_> = libraries
        .into_iter()
        .filter(|lib| lib.target.platform().is_apple())
        .collect();

    if apple_libraries.is_empty() {
        return Err(CliError::NoLibrariesFound {
            platform: "Apple".to_string(),
        });
    }

    let headers_dir = config.apple_header_output();
    if !headers_dir.exists() {
        return Err(CliError::FileNotFound(headers_dir));
    }

    let should_build_xcframework = !options.spm_only;
    let should_generate_spm = !options.xcframework_only;

    let xcframework_output = should_build_xcframework
        .then(|| {
            run_step("Creating xcframework", || {
                XcframeworkBuilder::new(config, apple_libraries.clone(), headers_dir.clone())
                    .build_with_zip()
            })
        })
        .transpose()?;

    if should_generate_spm {
        let (checksum, version) = match config.apple_spm_distribution() {
            SpmDistribution::Local => (None, None),
            SpmDistribution::Remote => {
                let checksum = xcframework_output
                    .as_ref()
                    .and_then(|o| o.checksum.clone())
                    .map(Ok)
                    .unwrap_or_else(|| {
                        run_step("Computing checksum from existing xcframework.zip", || {
                            existing_xcframework_checksum(config)
                        })
                    })?;
                let version = options
                    .version
                    .or_else(detect_version)
                    .unwrap_or_else(|| "0.1.0".to_string());
                (Some(checksum), Some(version))
            }
        };

        if config.apple_spm_skip_package_swift() {
            println!("Skipping Package.swift generation (skip_package_swift = true)");
        } else {
            let generator = match config.apple_spm_distribution() {
                SpmDistribution::Local => SpmPackageGenerator::new_local(config, layout),
                SpmDistribution::Remote => {
                    let checksum = checksum.ok_or_else(|| CliError::CommandFailed {
                        command: "remote SPM requires checksum".to_string(),
                        status: None,
                    })?;
                    let version = version.ok_or_else(|| CliError::CommandFailed {
                        command: "remote SPM requires version".to_string(),
                        status: None,
                    })?;
                    SpmPackageGenerator::new_remote(config, checksum, version, layout)
                }
            };

            let package_path = run_step("Generating Package.swift", || generator.generate())?;
            println!("Created: {}", package_path.display());
        }
    }

    if let Some(output) = xcframework_output {
        println!("Created: {}", output.xcframework_path.display());
        output
            .zip_path
            .as_ref()
            .iter()
            .for_each(|path| println!("Created: {}", path.display()));
        output
            .checksum
            .as_ref()
            .iter()
            .for_each(|checksum| println!("Checksum: {}", checksum));
    }

    Ok(())
}

fn pack_wasm(config: &Config, options: PackWasmOptions) -> Result<()> {
    if !config.is_wasm_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.wasm.enabled = false".to_string(),
            status: None,
        });
    }
    if config.wasm_npm_generate_package_json() && config.wasm_npm_package_name().is_none() {
        return Err(CliError::CommandFailed {
            command: "targets.wasm.npm.package_name is required for pack wasm".to_string(),
            status: None,
        });
    }

    let profile = if options.release {
        WasmProfile::Release
    } else {
        config.wasm_profile()
    };

    if !options.no_build {
        run_step("Building WASM target", || {
            build_wasm_target(config, profile)
        })?;
    }

    let wasm_artifact_path = config.wasm_artifact_path(profile);
    if !wasm_artifact_path.exists() {
        return Err(CliError::FileNotFound(wasm_artifact_path));
    }

    if config.wasm_optimize_enabled(profile) {
        run_step("Optimizing WASM binary", || {
            optimize_wasm_binary(config, &wasm_artifact_path)
        })?;
    }

    if options.regenerate {
        run_step("Generating TypeScript bindings", || {
            run_generate_with_output(
                config,
                GenerateOptions {
                    target: GenerateTarget::Typescript,
                    output: Some(config.wasm_typescript_output()),
                },
            )
        })?;
    }

    let npm_output = config.wasm_npm_output();
    std::fs::create_dir_all(&npm_output).map_err(|source| CliError::CreateDirectoryFailed {
        path: npm_output.clone(),
        source,
    })?;

    let module_name = config.wasm_typescript_module_name();
    let packaged_wasm_path = npm_output.join(format!("{}_bg.wasm", module_name));
    std::fs::copy(&wasm_artifact_path, &packaged_wasm_path).map_err(|source| {
        CliError::CopyFailed {
            from: wasm_artifact_path.clone(),
            to: packaged_wasm_path.clone(),
            source,
        }
    })?;

    let generated_typescript_source = config
        .wasm_typescript_output()
        .join(format!("{}.ts", module_name));
    if !generated_typescript_source.exists() {
        return Err(CliError::FileNotFound(generated_typescript_source));
    }

    run_step("Transpiling TypeScript bindings", || {
        transpile_typescript_bundle(config, &generated_typescript_source, &npm_output)
    })?;

    let enabled_targets = config.wasm_npm_targets();
    run_step("Generating WASM loader entrypoints", || {
        generate_wasm_loader_entrypoints(&module_name, &enabled_targets, &npm_output)
    })?;

    if config.wasm_npm_generate_package_json() {
        let package_json_path = run_step("Generating package.json", || {
            generate_wasm_package_json(config, &module_name, &enabled_targets, &npm_output)
        })?;
        println!("Created: {}", package_json_path.display());
    }

    if config.wasm_npm_generate_readme() {
        let readme_path = run_step("Generating README.md", || {
            generate_wasm_readme(config, &module_name, &enabled_targets, &npm_output)
        })?;
        println!("Created: {}", readme_path.display());
    }

    println!("Created: {}", packaged_wasm_path.display());
    println!(
        "Created: {}",
        npm_output.join(format!("{}.js", module_name)).display()
    );
    println!(
        "Created: {}",
        npm_output.join(format!("{}.d.ts", module_name)).display()
    );

    Ok(())
}

fn pack_android(config: &Config, options: PackAndroidOptions) -> Result<()> {
    if !config.is_android_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.android.enabled = false".to_string(),
            status: None,
        });
    }

    if !options.no_build {
        run_step("Building Android targets", || {
            build_android_targets(config, options.release)
        })?;
    }

    if options.regenerate {
        run_step("Generating Kotlin bindings", || {
            run_generate_with_output(
                config,
                GenerateOptions {
                    target: GenerateTarget::Kotlin,
                    output: Some(config.android_kotlin_output()),
                },
            )
        })?;
        run_step("Generating C header", || {
            run_generate_with_output(
                config,
                GenerateOptions {
                    target: GenerateTarget::Header,
                    output: Some(config.android_header_output()),
                },
            )
        })?;
    }

    let libraries = discover_built_libraries(&config.crate_artifact_name(), options.release);
    let android_libraries: Vec<_> = libraries
        .into_iter()
        .filter(|lib| lib.target.platform() == Platform::Android)
        .collect();

    if android_libraries.is_empty() {
        return Err(CliError::NoLibrariesFound {
            platform: "Android".to_string(),
        });
    }

    let packager = AndroidPackager::new(config, android_libraries, options.release);
    let output = run_step("Packaging jniLibs", || packager.package())?;

    println!("Created: {}", output.jnilibs_path.display());
    output
        .copied_libraries
        .iter()
        .for_each(|path| println!("  {}", path.display()));

    Ok(())
}

fn build_apple_targets(config: &Config, release: bool) -> Result<()> {
    let build_options = BuildOptions {
        release,
        package: Some(config.library_name().to_string()),
    };
    let builder = Builder::new(config, build_options);

    let mut results = builder.build_ios()?;
    if config.apple_include_macos() {
        results.extend(builder.build_macos()?);
    }

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results)
        .iter()
        .map(|triple| triple.to_string())
        .collect::<Vec<_>>();

    Err(CliError::BuildFailed { targets: failed })
}

fn build_android_targets(config: &Config, release: bool) -> Result<()> {
    let build_options = BuildOptions {
        release,
        package: Some(config.library_name().to_string()),
    };
    let builder = Builder::new(config, build_options);
    let results = builder.build_android()?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results)
        .iter()
        .map(|triple| triple.to_string())
        .collect::<Vec<_>>();

    Err(CliError::BuildFailed { targets: failed })
}

fn build_wasm_target(config: &Config, profile: WasmProfile) -> Result<()> {
    let build_options = BuildOptions {
        release: matches!(profile, WasmProfile::Release),
        package: Some(config.library_name().to_string()),
    };
    let builder = Builder::new(config, build_options);
    let results = builder.build_wasm_with_triple(config.wasm_triple())?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results)
        .iter()
        .map(|triple| triple.to_string())
        .collect::<Vec<_>>();

    Err(CliError::BuildFailed { targets: failed })
}

fn generate_apple_bindings(config: &Config, layout: SpmLayout, package_root: &Path) -> Result<()> {
    let swift_output_dir = match layout {
        SpmLayout::Bundled => config
            .apple_spm_wrapper_sources()
            .map(|path| package_root.join(path).join("BoltFFI"))
            .unwrap_or_else(|| package_root.join("Sources").join("BoltFFI")),
        SpmLayout::FfiOnly => package_root.join("Sources").join("BoltFFI"),
        SpmLayout::Split => config.apple_swift_output().join("BoltFFI"),
    };

    run_generate_with_output(
        config,
        GenerateOptions {
            target: GenerateTarget::Swift,
            output: Some(swift_output_dir),
        },
    )?;

    run_generate_with_output(
        config,
        GenerateOptions {
            target: GenerateTarget::Header,
            output: Some(config.apple_header_output()),
        },
    )?;

    Ok(())
}

fn optimize_wasm_binary(config: &Config, wasm_path: &Path) -> Result<()> {
    let optimize_level_flag = match config.wasm_optimize_level() {
        WasmOptimizeLevel::O0 => "-O0",
        WasmOptimizeLevel::O1 => "-O1",
        WasmOptimizeLevel::O2 => "-O2",
        WasmOptimizeLevel::O3 => "-O3",
        WasmOptimizeLevel::O4 => "-O4",
        WasmOptimizeLevel::Size => "-Os",
        WasmOptimizeLevel::MinSize => "-Oz",
    };

    let wasm_opt_path = match which::which("wasm-opt") {
        Ok(path) => path,
        Err(_) => {
            return match config.wasm_optimize_on_missing() {
                WasmOptimizeOnMissing::Error => Err(CliError::CommandFailed {
                    command: "wasm-opt not found in PATH".to_string(),
                    status: None,
                }),
                WasmOptimizeOnMissing::Warn => {
                    println!("warning: wasm-opt not found, skipping optimization");
                    Ok(())
                }
                WasmOptimizeOnMissing::Skip => Ok(()),
            };
        }
    };

    let optimized_path = wasm_path.with_extension("optimized.wasm");
    let mut command = Command::new(wasm_opt_path);
    command
        .arg(optimize_level_flag)
        .arg(wasm_path)
        .arg("-o")
        .arg(&optimized_path);

    if !config.wasm_optimize_strip_debug() {
        command.arg("-g");
    }

    let status = command.status().map_err(|_| CliError::CommandFailed {
        command: "wasm-opt".to_string(),
        status: None,
    })?;

    if !status.success() {
        return Err(CliError::CommandFailed {
            command: "wasm-opt".to_string(),
            status: status.code(),
        });
    }

    std::fs::rename(&optimized_path, wasm_path).map_err(|source| CliError::WriteFailed {
        path: wasm_path.to_path_buf(),
        source,
    })
}

fn transpile_typescript_bundle(
    config: &Config,
    source_file: &Path,
    output_dir: &Path,
) -> Result<()> {
    let mut command = Command::new("tsc");
    command
        .arg(source_file)
        .arg("--target")
        .arg("ES2020")
        .arg("--module")
        .arg("ES2020")
        .arg("--moduleResolution")
        .arg("bundler")
        .arg("--declaration")
        .arg("--sourceMap")
        .arg(if config.wasm_source_map_enabled() {
            "true"
        } else {
            "false"
        })
        .arg("--skipLibCheck")
        .arg("--noEmitOnError")
        .arg("false")
        .arg("--outDir")
        .arg(output_dir);

    let output = command.output().map_err(|_| CliError::CommandFailed {
        command: "tsc".to_string(),
        status: None,
    })?;

    let module_name = config.wasm_typescript_module_name();
    let javascript_path = output_dir.join(format!("{}.js", module_name));
    let declarations_path = output_dir.join(format!("{}.d.ts", module_name));
    let emitted_outputs_exist = javascript_path.exists() && declarations_path.exists();

    if output.status.success() || emitted_outputs_exist {
        return Ok(());
    }

    Err(CliError::CommandFailed {
        command: format!("tsc failed: {}", String::from_utf8_lossy(&output.stderr)),
        status: output.status.code(),
    })
}

fn generate_wasm_loader_entrypoints(
    module_name: &str,
    enabled_targets: &[WasmNpmTarget],
    output_dir: &Path,
) -> Result<()> {
    enabled_targets
        .iter()
        .try_for_each(|target| {
            let (filename, content) = match target {
                WasmNpmTarget::Bundler => (
                    "bundler.js",
                    format!(
                        "import init from \"./{module}.js\";\nexport * from \"./{module}.js\";\nexport {{ default as init }} from \"./{module}.js\";\nexport const initialized = (async () => {{\n  const response = await fetch(new URL(\"./{module}_bg.wasm\", import.meta.url));\n  await init(response);\n}})();\n",
                        module = module_name
                    ),
                ),
                WasmNpmTarget::Web => (
                    "web.js",
                    format!(
                        "import init from \"./{module}.js\";\nexport * from \"./{module}.js\";\nexport {{ default as init }} from \"./{module}.js\";\nexport const initialized = (async () => {{\n  const response = await fetch(new URL(\"./{module}_bg.wasm\", import.meta.url));\n  await init(response);\n}})();\n",
                        module = module_name
                    ),
                ),
                WasmNpmTarget::Nodejs => (
                    "node.js",
                    format!(
                        "import init from \"./{module}.js\";\nimport {{ readFile }} from \"node:fs/promises\";\nimport {{ dirname, join }} from \"node:path\";\nimport {{ fileURLToPath }} from \"node:url\";\nexport * from \"./{module}.js\";\nexport {{ default as init }} from \"./{module}.js\";\nexport const initialized = (async () => {{\n  const thisDir = dirname(fileURLToPath(import.meta.url));\n  const wasmBytes = await readFile(join(thisDir, \"{module}_bg.wasm\"));\n  await init(wasmBytes);\n}})();\n",
                        module = module_name
                    ),
                ),
            };

            let path = output_dir.join(filename);
            std::fs::write(&path, content).map_err(|source| CliError::WriteFailed {
                path,
                source,
            })
        })
}

fn generate_wasm_package_json(
    config: &Config,
    module_name: &str,
    enabled_targets: &[WasmNpmTarget],
    output_dir: &Path,
) -> Result<PathBuf> {
    let package_name = config
        .wasm_npm_package_name()
        .ok_or_else(|| CliError::CommandFailed {
            command: "targets.wasm.npm.package_name is required for pack wasm".to_string(),
            status: None,
        })?;
    let package_version = config
        .wasm_npm_version()
        .unwrap_or_else(|| "0.1.0".to_string());

    let has_bundler = enabled_targets.contains(&WasmNpmTarget::Bundler);
    let has_web = enabled_targets.contains(&WasmNpmTarget::Web);
    let has_node = enabled_targets.contains(&WasmNpmTarget::Nodejs);
    let default_entry = if has_bundler {
        "./bundler.js"
    } else if has_web {
        "./web.js"
    } else {
        "./node.js"
    };

    let mut export_map = serde_json::Map::new();
    export_map.insert(
        "types".to_string(),
        serde_json::Value::String(format!("./{}.d.ts", module_name)),
    );
    if has_web {
        export_map.insert(
            "browser".to_string(),
            serde_json::Value::String("./web.js".to_string()),
        );
    }
    if has_node {
        export_map.insert(
            "node".to_string(),
            serde_json::Value::String("./node.js".to_string()),
        );
    }
    export_map.insert(
        "default".to_string(),
        serde_json::Value::String(default_entry.to_string()),
    );

    let package_json = serde_json::json!({
        "name": package_name,
        "version": package_version,
        "type": "module",
        "exports": {
            ".": serde_json::Value::Object(export_map)
        },
        "types": format!("./{}.d.ts", module_name),
        "files": [
            format!("{}.js", module_name),
            format!("{}.d.ts", module_name),
            format!("{}_bg.wasm", module_name),
            "bundler.js",
            "web.js",
            "node.js"
        ]
    });

    let mut package_json_object =
        package_json
            .as_object()
            .cloned()
            .ok_or_else(|| CliError::CommandFailed {
                command: "failed to construct package.json payload".to_string(),
                status: None,
            })?;
    if let Some(license) = config.wasm_npm_license() {
        package_json_object.insert("license".to_string(), serde_json::Value::String(license));
    }
    if let Some(repository) = config.wasm_npm_repository() {
        package_json_object.insert(
            "repository".to_string(),
            serde_json::Value::String(repository),
        );
    }

    let rendered = serde_json::to_string_pretty(&serde_json::Value::Object(package_json_object))
        .map_err(|source| CliError::CommandFailed {
            command: format!("failed to serialize package.json: {}", source),
            status: None,
        })?;
    let package_json_path = output_dir.join("package.json");
    std::fs::write(&package_json_path, rendered).map_err(|source| CliError::WriteFailed {
        path: package_json_path.clone(),
        source,
    })?;

    Ok(package_json_path)
}

fn generate_wasm_readme(
    config: &Config,
    module_name: &str,
    enabled_targets: &[WasmNpmTarget],
    output_dir: &Path,
) -> Result<PathBuf> {
    let package_name = config.wasm_npm_package_name().unwrap_or(module_name);
    let targets_text = enabled_targets
        .iter()
        .map(|target| match target {
            WasmNpmTarget::Bundler => "bundler",
            WasmNpmTarget::Web => "web",
            WasmNpmTarget::Nodejs => "nodejs",
        })
        .collect::<Vec<_>>()
        .join(", ");
    let content = format!(
        "# {package_name}\n\nGenerated by boltffi.\n\nEnabled wasm npm targets: {targets_text}\n\n```ts\nimport {{ initialized }} from \"{package_name}\";\nawait initialized;\n```\n"
    );

    let readme_path = output_dir.join("README.md");
    std::fs::write(&readme_path, content).map_err(|source| CliError::WriteFailed {
        path: readme_path.clone(),
        source,
    })?;

    Ok(readme_path)
}

fn discover_built_libraries(crate_artifact_name: &str, release: bool) -> Vec<BuiltLibrary> {
    BuiltLibrary::discover(&PathBuf::from("target"), crate_artifact_name, release)
}

fn existing_xcframework_checksum(config: &Config) -> Result<String> {
    let xcframework_zip = config
        .apple_xcframework_output()
        .join(format!("{}.xcframework.zip", config.xcframework_name()));

    if xcframework_zip.exists() {
        return compute_checksum(&xcframework_zip);
    }

    Err(CliError::FileNotFound(xcframework_zip))
}

fn detect_version() -> Option<String> {
    std::fs::read_to_string("Cargo.toml")
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|line| line.starts_with("version = "))
                .and_then(|line| {
                    line.split('=')
                        .nth(1)
                        .map(|s| s.trim().trim_matches('"').to_string())
                })
        })
}

fn run_step<T>(label: &str, action: impl FnOnce() -> Result<T>) -> Result<T> {
    print!("{}... ", label);
    io::stdout().flush().ok();
    action().inspect(|_value| {
        println!("✓");
    })
}
