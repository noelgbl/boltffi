use std::path::PathBuf;

fn main() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let header_path = out_dir.join("mobiFFI_core.h");
    
    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(cbindgen::Config::from_root_or_default(&crate_dir))
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(&header_path);
    
    println!("cargo:rerun-if-changed=src/");
}
