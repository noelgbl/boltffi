use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::{CliError, Result};
use crate::target::{BuiltLibrary, Platform};

pub struct AndroidPackager<'a> {
    config: &'a Config,
    libraries: Vec<BuiltLibrary>,
}

pub struct AndroidOutput {
    pub jnilibs_path: PathBuf,
    pub copied_libraries: Vec<PathBuf>,
}

impl<'a> AndroidPackager<'a> {
    pub fn new(config: &'a Config, libraries: Vec<BuiltLibrary>) -> Self {
        Self { config, libraries }
    }

    pub fn package(self) -> Result<AndroidOutput> {
        let android_libs = self.filter_android_libraries();

        if android_libs.is_empty() {
            return Err(CliError::NoLibrariesFound {
                platform: "Android".to_string(),
            });
        }

        let jnilibs_path = &self.config.pack.android.output;

        std::fs::create_dir_all(jnilibs_path).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: jnilibs_path.clone(),
                source,
            }
        })?;

        let copied_libraries = android_libs
            .iter()
            .map(|lib| self.copy_to_jnilibs(lib, jnilibs_path))
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

    fn copy_to_jnilibs(&self, library: &BuiltLibrary, jnilibs_path: &Path) -> Result<PathBuf> {
        let abi = library.target.architecture().android_abi();
        let abi_dir = jnilibs_path.join(abi);

        std::fs::create_dir_all(&abi_dir).map_err(|source| CliError::CreateDirectoryFailed {
            path: abi_dir.clone(),
            source,
        })?;

        let lib_name = self.config.library_name();
        let dest_path = abi_dir.join(format!("lib{}.so", lib_name));

        std::fs::copy(&library.path, &dest_path).map_err(|source| CliError::CopyFailed {
            from: library.path.clone(),
            to: dest_path.clone(),
            source,
        })?;

        Ok(dest_path)
    }
}

pub fn generate_gradle_dependency(group: &str, artifact: &str, version: &str) -> String {
    format!(r#"implementation("{}:{}:{}")"#, group, artifact, version)
}
