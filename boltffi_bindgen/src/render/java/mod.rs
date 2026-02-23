mod emit;
mod lower;
mod mappings;
mod names;
mod plan;
mod templates;

pub use emit::{JavaEmitter, JavaFile, JavaOutput};
pub use lower::JavaLowerer;
pub use names::NamingConvention;
pub use plan::*;

#[derive(Debug, Clone, Default)]
pub struct JavaOptions {
    pub library_name: Option<String>,
    pub min_java_version: JavaVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct JavaVersion(pub u8);

impl Default for JavaVersion {
    fn default() -> Self {
        Self(8)
    }
}

impl JavaVersion {
    pub const JAVA_8: Self = Self(8);
    pub const JAVA_11: Self = Self(11);
    pub const JAVA_17: Self = Self(17);
    pub const JAVA_21: Self = Self(21);
    pub const JAVA_22: Self = Self(22);
    pub const JAVA_23: Self = Self(23);
    pub const JAVA_24: Self = Self(24);

    pub fn supports_records(&self) -> bool {
        self.0 >= 16
    }

    pub fn supports_sealed(&self) -> bool {
        self.0 >= 17
    }
}
