fn main() {
    // PerMonitorV2 must be declared before any window exists; see the comment in app.manifest.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest::embed_manifest_file("app.manifest").expect("app.manifest embeds");
        embed_resource::compile("app.rc", embed_resource::NONE).manifest_optional().expect("app.rc compiles");
    }
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
}
