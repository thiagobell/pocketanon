fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // The CoreML execution provider (enabled for macOS in Cargo.toml) contains Objective-C
    // that calls `@available`, which needs `___isPlatformVersionAtLeast` from clang's
    // compiler runtime. rustc does not link that runtime by itself; this flag asks clang to.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-fapple-link-rtlib");
    }
}
