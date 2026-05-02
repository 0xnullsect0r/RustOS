use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=third_party/rsh");

    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set by Cargo"),
    );
    let rsh_dir = manifest_dir.join("third_party/rsh");
    let rsh_manifest = rsh_dir.join("Cargo.toml");
    if !rsh_manifest.exists() {
        panic!(
            "missing rsh submodule at {}; run: git submodule update --init --recursive",
            rsh_dir.display()
        );
    }

    let cargo = env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));
    let mut cmd = Command::new(cargo);
    cmd.current_dir(&rsh_dir).arg("build").arg("--release");
    if env::var_os("CLIPPY_ARGS").is_some() {
        cmd.env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("RUSTC_WRAPPER")
            .env_remove("CLIPPY_ARGS");
    }
    let status = cmd.status().expect("failed to execute cargo to build rsh");
    if !status.success() {
        panic!("failed to build rsh submodule");
    }

    let rsh_elf = rsh_dir.join("target/x86_64-unknown-rustos/release/rsh");
    if !rsh_elf.exists() {
        panic!("rsh binary not found after build: {}", rsh_elf.display());
    }

    println!("cargo:rustc-env=RSH_ELF_PATH={}", rsh_elf.display());
}
