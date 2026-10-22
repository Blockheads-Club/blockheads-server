//! Android builds with the mod host link Luau against the static C++ runtime (`CXXSTDLIB_*=c++_static` in
//! `.cargo/config.toml`); libc++_static needs libc++abi next to it.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let android = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "android");
    let mods = std::env::var_os("CARGO_FEATURE_MODS").is_some();
    let static_cxx = std::env::var("CXXSTDLIB_".to_string() + &std::env::var("TARGET").unwrap_or_default().replace('-', "_"))
        .is_ok_and(|v| v == "c++_static");
    if android && mods && static_cxx {
        println!("cargo:rustc-link-lib=c++abi");
    }
}
