//! hello — minimal RustOS userspace program demonstrating rustos-rt.
//!
//! Build:
//!   cargo +nightly build --example hello \
//!     --target x86_64-unknown-rustos.json \
//!     -Z build-std=core \
//!     --release
//!
//! The resulting ELF is at:
//!   target/x86_64-unknown-rustos/release/examples/hello
//!
//! Copy it onto a FAT32 USB drive as /hello, then from the RustOS shell run:
//!   exec /usb/hello

#![no_std]
#![no_main]

#[no_mangle]
fn main() -> i64 {
    rustos_rt::println!("Hello from RustOS userspace!");
    0
}
