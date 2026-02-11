use std::path::{Path, PathBuf};
use std::process::Command;

use crate::android::AndroidToolchain;
use crate::config::Config;
use crate::error::{CliError, Result};
use crate::target::{BuiltLibrary, Platform};

pub struct AndroidPackager<'a> {
    config: &'a Config,
    libraries: Vec<BuiltLibrary>,
    release: bool,
}

pub struct AndroidOutput {
    pub jnilibs_path: PathBuf,
    pub copied_libraries: Vec<PathBuf>,
}

impl<'a> AndroidPackager<'a> {
    pub fn new(config: &'a Config, libraries: Vec<BuiltLibrary>, release: bool) -> Self {
        Self {
            config,
            libraries,
            release,
        }
    }

    pub fn package(self) -> Result<AndroidOutput> {
        let android_libs = self.filter_android_libraries();

        if android_libs.is_empty() {
            return Err(CliError::NoLibrariesFound {
                platform: "Android".to_string(),
            });
        }

        let jnilibs_path = self.config.android_pack_output();
        let android_toolchain = AndroidToolchain::discover(
            self.config.android_min_sdk(),
            self.config.android_ndk_version(),
        )?;

        std::fs::create_dir_all(&jnilibs_path).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: jnilibs_path.clone(),
                source,
            }
        })?;

        let jni_glue_path = self.android_jni_glue_path()?;
        let header_include_dir = self.config.android_header_output();
        let header_path = header_include_dir.join(format!("{}.h", self.config.library_name()));
        if !header_path.exists() {
            return Err(CliError::FileNotFound(header_path));
        }

        let copied_libraries = android_libs
            .iter()
            .map(|lib| {
                self.link_shared_library(
                    lib,
                    &jnilibs_path,
                    &android_toolchain,
                    &jni_glue_path,
                    &header_include_dir,
                )
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(AndroidOutput {
            jnilibs_path: jnilibs_path.clone(),
            copied_libraries,
        })
    }

    fn filter_android_libraries(&self) -> Vec<&BuiltLibrary> {
        self.libraries
            .iter()
            .filter(|lib| lib.target.platform() == Platform::Android)
            .collect()
    }

    fn android_jni_glue_path(&self) -> Result<PathBuf> {
        let jni_glue_path = self
            .config
            .android_kotlin_output()
            .join("jni")
            .join("jni_glue.c");
        jni_glue_path
            .exists()
            .then_some(jni_glue_path.clone())
            .ok_or(CliError::FileNotFound(jni_glue_path))
    }

    fn link_shared_library(
        &self,
        library: &BuiltLibrary,
        jnilibs_path: &Path,
        android_toolchain: &AndroidToolchain,
        jni_glue_path: &Path,
        header_include_dir: &Path,
    ) -> Result<PathBuf> {
        let abi = library.target.architecture().android_abi();
        let abi_dir = jnilibs_path.join(abi);

        std::fs::create_dir_all(&abi_dir).map_err(|source| CliError::CreateDirectoryFailed {
            path: abi_dir.clone(),
            source,
        })?;

        let lib_name = self.config.library_name();
        let dest_path = abi_dir.join(format!("lib{}.so", lib_name));
        let build_dir = PathBuf::from("target")
            .join("boltffi")
            .join("android")
            .join(library.target.triple())
            .join(if self.release { "release" } else { "debug" });
        std::fs::create_dir_all(&build_dir).map_err(|source| CliError::CreateDirectoryFailed {
            path: build_dir.clone(),
            source,
        })?;

        let clang = android_toolchain.clang_for_target(&library.target)?;
        let object_path = build_dir.join("jni_glue.o");

        let mut compile = Command::new(&clang);
        compile
            .arg("-c")
            .arg("-fPIC")
            .arg(if self.release { "-O3" } else { "-O0" })
            .arg("-I")
            .arg(header_include_dir)
            .arg(jni_glue_path)
            .arg("-o")
            .arg(&object_path);
        run_command(compile)?;

        let mut link = Command::new(&clang);
        link.arg("-shared")
            .arg("-o")
            .arg(&dest_path)
            .arg(&object_path)
            .arg("-Wl,--whole-archive")
            .arg(&library.path)
            .arg("-Wl,--no-whole-archive")
            .arg("-lm")
            .arg("-llog")
            .arg("-ldl");
        run_command(link)?;

        Ok(dest_path)
    }
}

fn run_command(mut command: Command) -> Result<()> {
    let command_string = format!("{:?}", command);
    let status = command.status().map_err(|_| CliError::CommandFailed {
        command: command_string.clone(),
        status: None,
    })?;

    status
        .success()
        .then_some(())
        .ok_or(CliError::CommandFailed {
            command: command_string,
            status: status.code(),
        })
}
