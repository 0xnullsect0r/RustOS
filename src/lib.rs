//! rustos-rt — Rust runtime for RustOS userspace programs.
//!
//! This crate provides:
//!
//! * `_start` — the ELF entry point called by the RustOS process loader.
//! * Syscall wrappers (`sys_write`, `sys_read`, `sys_exit`) via `int 0x80`.
//! * A minimal `#[panic_handler]` that calls `sys_exit(1)`.
//!
//! # Usage
//!
//! Add `rustos-rt` as a dependency in your program's `Cargo.toml`.  Build with
//! the `x86_64-unknown-rustos` target (see `x86_64-unknown-rustos.json` in the
//! crate root).
//!
//! ```
//! cargo +nightly build --target path/to/x86_64-unknown-rustos.json -Z build-std=core,alloc
//! ```

#![no_std]
#![no_main]
#![feature(lang_items)]
#![allow(internal_features)]

use core::panic::PanicInfo;

// ── Syscall numbers (must match src/syscall/mod.rs in the kernel) ─────────────

pub const SYS_READ:  u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_EXIT:  u64 = 60;
pub const SYS_OPEN:  u64 = 2;
pub const SYS_CLOSE: u64 = 3;

// ── Low-level syscall shim ────────────────────────────────────────────────────

/// Perform a raw RustOS syscall via `int 0x80`.
///
/// # Safety
/// All six argument registers are clobbered per RustOS ABI.
#[inline(always)]
pub unsafe fn syscall(nr: u64, a0: u64, a1: u64, a2: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "int 0x80",
        inout("rax") nr => ret,
        in("rdi") a0,
        in("rsi") a1,
        in("rdx") a2,
        options(nostack, preserves_flags),
    );
    ret
}

// ── High-level syscall wrappers ───────────────────────────────────────────────

/// Write `buf` to file descriptor `fd` (0 = stdin, 1 = stdout, 2 = stderr).
/// Returns the number of bytes written, or a negative error code.
pub fn sys_write(fd: u64, buf: &[u8]) -> i64 {
    unsafe { syscall(SYS_WRITE, fd, buf.as_ptr() as u64, buf.len() as u64) }
}

/// Read up to `buf.len()` bytes from `fd` into `buf`.
/// Returns the number of bytes read, or a negative error code.
pub fn sys_read(fd: u64, buf: &mut [u8]) -> i64 {
    unsafe { syscall(SYS_READ, fd, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

/// Terminate the process with the given exit code.
pub fn sys_exit(code: i64) -> ! {
    unsafe { syscall(SYS_EXIT, code as u64, 0, 0) };
    // Unreachable — the kernel never returns from SYS_EXIT.
    loop {}
}

/// Convenience: print a string slice to stdout.
pub fn print(s: &str) {
    sys_write(1, s.as_bytes());
}

/// Convenience: print a string slice to stdout followed by a newline.
pub fn println(s: &str) {
    sys_write(1, s.as_bytes());
    sys_write(1, b"\n");
}

// ── Entry-point glue ──────────────────────────────────────────────────────────

extern "Rust" {
    /// User programs must define `fn main() -> i64`.  The return value is used
    /// as the process exit code.
    fn main() -> i64;
}

/// ELF entry point — set up a minimal environment and call `main`.
///
/// # Safety
/// Called by the kernel's ELF loader; the stack is initialised, but there is no
/// C runtime or TLS.  Do not use thread-locals or any library that assumes them.
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    // Zero the BSS segment.
    extern "C" {
        static mut __bss_start: u8;
        static mut __bss_end: u8;
    }
    let bss_len = (&raw const __bss_end as usize) - (&raw const __bss_start as usize);
    core::ptr::write_bytes(&raw mut __bss_start, 0, bss_len);

    let exit_code = main();
    sys_exit(exit_code);
}

// ── Panic handler ─────────────────────────────────────────────────────────────

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    sys_exit(1);
}
