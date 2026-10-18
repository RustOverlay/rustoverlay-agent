use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("../../proto");
    let proto = root.join("rustoverlay/v1/team.proto");
    println!("cargo:rerun-if-changed={}", proto.display());

    let descriptors = protox::compile([&proto], [&root])?;
    prost_build::Config::new().compile_fds(descriptors)?;

    println!("cargo:rerun-if-env-changed=RUSTOVERLAY_VERSION");
    let version = env::var("RUSTOVERLAY_VERSION").ok();
    println!("cargo:rustc-env=RO_AGENT_VERSION={}", assembly_version(version.as_deref()));
    Ok(())
}

/// What .NET's `-p:Version=X` produced as the assembly version: the numeric prefix padded to four
/// parts, with an unversioned build reporting 1.0.0.0.
fn assembly_version(version: Option<&str>) -> String {
    let numeric = version
        .map(|v| v.split(['-', '+']).next().unwrap_or_default())
        .filter(|v| !v.is_empty())
        .unwrap_or("1.0.0");

    let mut parts: Vec<&str> = numeric.split('.').collect();
    while parts.len() < 4 {
        parts.push("0");
    }
    parts.join(".")
}
