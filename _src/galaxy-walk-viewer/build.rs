//! The self-contained windows-gnu Rust toolchain ships no `libshlwapi.a`, but a
//! dependency asks the linker for `-lshlwapi`. The functions it needs are already
//! provided by the `windows_x86_64_gnu` import library that is linked anyway, so an
//! empty placeholder archive is enough to satisfy the linker.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.ends_with("windows-gnu") {
        let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
        let dir = out.join("placeholder-libs");
        std::fs::create_dir_all(&dir).expect("create placeholder dir");
        let lib = dir.join("libshlwapi.a");
        if !lib.exists() {
            std::fs::write(&lib, b"!<arch>\n").expect("write placeholder archive");
        }
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
}
