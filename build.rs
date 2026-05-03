use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=third_party/tcp-ip/src");
    println!("cargo:rerun-if-changed=third_party/rsh/src");
    // Re-run whenever either submodule HEAD advances.
    println!("cargo:rerun-if-changed=.git/modules/third_party/tcp-ip/HEAD");
    println!("cargo:rerun-if-changed=.git/modules/third_party/rsh/HEAD");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();

    // Pull both submodules to their latest upstream commits before building.
    update_submodules(&manifest_dir);

    let tcp_ip_dir = Path::new(&manifest_dir).join("third_party/tcp-ip");

    if !tcp_ip_dir.exists() {
        println!(
            "cargo:warning=tcp-ip submodule not initialised; run `git submodule update --init --remote`"
        );
        write_empty_bins(&out_dir);
        return;
    }

    // Build the four userspace management ELF binaries.
    // The submodule carries its own .cargo/config.toml that sets the target,
    // build-std flags, and linker script, so a plain `cargo build --release`
    // inside the submodule directory is sufficient.
    //
    // Avoid nested `cargo` invocations when the binaries are already present —
    // running cargo-from-a-build-script causes lock-file contention and the
    // CARGO env-var may point to a cargo-clippy wrapper that doesn't support
    // the `build` subcommand.
    let release_dir = tcp_ip_dir.join("target/x86_64-unknown-rustos/release");
    let bin_names = ["wifi", "ping", "ifconfig", "netstat"];
    let all_exist = bin_names.iter().all(|n| release_dir.join(n).exists());

    let build_ok = all_exist
        || {
            let status = Command::new("cargo")
                .args(["build", "--release"])
                .current_dir(&tcp_ip_dir)
                .status();

            let ok = matches!(status, Ok(ref s) if s.success());
            if !ok {
                println!(
                    "cargo:warning=tcp-ip ELF build failed; /bin/wifi, /bin/ping, /bin/ifconfig, /bin/netstat will not be installed"
                );
            }
            ok
        };

    write_net_bins_rs(&out_dir, &release_dir, build_ok);
}

fn write_empty_bins(out_dir: &str) {
    let path = Path::new(out_dir).join("net_bins.rs");
    std::fs::write(
        &path,
        "pub static WIFI_ELF: &[u8] = &[];\n\
         pub static PING_ELF: &[u8] = &[];\n\
         pub static IFCONFIG_ELF: &[u8] = &[];\n\
         pub static NETSTAT_ELF: &[u8] = &[];\n",
    )
    .unwrap();
}

fn write_net_bins_rs(out_dir: &str, release_dir: &Path, build_ok: bool) {
    let path = Path::new(out_dir).join("net_bins.rs");
    let mut content = String::new();
    for name in &["wifi", "ping", "ifconfig", "netstat"] {
        let elf_path = release_dir.join(name);
        let const_name = name.to_uppercase();
        if build_ok && elf_path.exists() {
            // Use the absolute path so include_bytes! always resolves correctly.
            let abs = elf_path.to_str().unwrap().replace('\\', "/");
            content.push_str(&format!(
                "pub static {}_ELF: &[u8] = include_bytes!(\"{}\");\n",
                const_name, abs
            ));
        } else {
            content.push_str(&format!("pub static {}_ELF: &[u8] = &[];\n", const_name));
        }
    }
    std::fs::write(&path, content).unwrap();
}

/// Pull both submodules to their latest upstream commits.
///
/// Uses `git submodule update --init --remote` so the build always tracks the
/// HEAD of each submodule's default branch rather than a stale pinned commit.
/// Failures are downgraded to a cargo warning so an offline build can still
/// proceed with whatever commits are already checked out.
fn update_submodules(manifest_dir: &str) {
    let result = Command::new("git")
        .args([
            "submodule",
            "update",
            "--init",
            "--remote",
            "third_party/tcp-ip",
            "third_party/rsh",
        ])
        .current_dir(manifest_dir)
        .status();

    match result {
        Ok(s) if s.success() => {}
        Ok(s) => println!(
            "cargo:warning=git submodule update --remote exited with {s}; \
             using currently checked-out submodule commits"
        ),
        Err(e) => println!(
            "cargo:warning=could not run git to update submodules ({e}); \
             using currently checked-out submodule commits"
        ),
    }
}
