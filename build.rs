use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=rsh/src");
    println!("cargo:rerun-if-changed=.git/modules/rsh/HEAD");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    update_submodules(&manifest_dir);
}

/// Initialize submodules at the commits pinned by this repository.
fn update_submodules(manifest_dir: &str) {
    let result = Command::new("git")
        .args(["submodule", "update", "--init", "rsh"])
        .current_dir(manifest_dir)
        .status();

    match result {
        Ok(s) if s.success() => {}
        Ok(s) => println!(
            "cargo:warning=git submodule update exited with {s}; \
             using currently checked-out submodule commits"
        ),
        Err(e) => println!(
            "cargo:warning=could not run git to update submodules ({e}); \
             using currently checked-out submodule commits"
        ),
    }
}
