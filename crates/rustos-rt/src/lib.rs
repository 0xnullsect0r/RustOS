//! rustos-rt — Rust runtime for RustOS userspace programs.
//!
//! This crate provides:
//!
//! * `_start` — the ELF entry point called by the RustOS process loader.
//! * A complete set of syscall constants that match the RustOS kernel ABI.
//! * High-level syscall wrappers (`sys_write`, `sys_read`, `sys_exit`, …).
//! * [`print!`] and [`println!`] macros backed by `core::fmt`.
//! * A minimal `#[panic_handler]` that calls `sys_exit(1)`.
//!
//! Network syscall constants and wrappers (`SYS_WIFI_*`, `SYS_NET_*`) are
//! available when the **`net`** feature is enabled.
//!
//! # Usage
//!
//! Add `rustos-rt` as a dependency in your program's `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! rustos-rt = { git = "https://github.com/RustOS-Dev/rustos-rt" }
//! ```
//!
//! Build with the bundled target spec:
//!
//! ```bash
//! cargo +nightly build \
//!   --target path/to/x86_64-unknown-rustos.json \
//!   -Z build-std=core \
//!   --release
//! ```
//!
//! Your program must define `fn main() -> i64`; `_start` calls it and exits
//! with the returned code.

#![no_std]

use core::fmt::{self, Write};
use core::panic::PanicInfo;

// ── Syscall numbers ───────────────────────────────────────────────────────────
//
// These must match the dispatch table in `src/syscall/mod.rs` of the RustOS
// kernel.

/// Read from a file descriptor.
pub const SYS_READ: u64 = 0;
/// Write to a file descriptor.
pub const SYS_WRITE: u64 = 1;
/// Open a path and return a file descriptor.
pub const SYS_OPEN: u64 = 2;
/// Close a file descriptor.
pub const SYS_CLOSE: u64 = 3;
/// Create a pipe: fills `pipefd[0]` (read end) and `pipefd[1]` (write end).
pub const SYS_PIPE: u64 = 22;
/// Duplicate a file descriptor to a specific number (`dup2`).
pub const SYS_DUP2: u64 = 33;
/// Execute an ELF binary (NUL-terminated path pointer in `rdi`).
pub const SYS_EXEC: u64 = 59;
/// Terminate the current process.
pub const SYS_EXIT: u64 = 60;
/// Wait for a child process to finish.
pub const SYS_WAITPID: u64 = 61;
/// Get the current working directory.
pub const SYS_GETCWD: u64 = 79;
/// Change the current working directory.
pub const SYS_CHDIR: u64 = 80;
/// Read directory entries (Linux `getdents64` number for compatibility).
pub const SYS_GETDENTS64: u64 = 217;

// ── Network syscall numbers (RustOS extensions; require `net` feature) ────────

/// Scan for visible 802.11 networks.
///
/// `rdi` = output buffer pointer, `rsi` = buffer length.
/// Returns bytes written (packed `WifiNetworkEntry` records) or negative error.
#[cfg(feature = "net")]
pub const SYS_WIFI_SCAN: u64 = 300;

/// Connect to an 802.11 network.
///
/// `rdi` = pointer to [`WifiConnectArgs`], `rsi` = struct size.
/// Returns 0 on success or a negative error code.
#[cfg(feature = "net")]
pub const SYS_WIFI_CONNECT: u64 = 301;

/// Disconnect from the current 802.11 network.
///
/// No arguments.  Returns 0 on success.
#[cfg(feature = "net")]
pub const SYS_WIFI_DISCONNECT: u64 = 302;

/// Query the current WiFi / IP status.
///
/// `rdi` = output buffer pointer, `rsi` = buffer length.
/// Returns bytes written (packed `WifiStatusEntry`) or negative error.
#[cfg(feature = "net")]
pub const SYS_WIFI_STATUS: u64 = 303;

/// Query per-interface IP configuration.
///
/// `rdi` = output buffer pointer, `rsi` = buffer length.
/// Returns bytes written (packed `IfaceInfo` records) or negative error.
#[cfg(feature = "net")]
pub const SYS_NET_IFCONFIG: u64 = 304;

/// Set the IP configuration of an interface.
///
/// `rdi` = pointer to `IfaceSetArgs`, `rsi` = struct size.
/// Returns 0 on success or a negative error code.
#[cfg(feature = "net")]
pub const SYS_NET_IFCONFIG_SET: u64 = 305;

/// Send a single ICMP echo request and wait for a reply.
///
/// `rdi` = destination IPv4 address (u32 big-endian), `rsi` = sequence number.
/// Returns round-trip time in microseconds or a negative error code.
#[cfg(feature = "net")]
pub const SYS_NET_PING: u64 = 306;

/// Query active TCP connections and UDP sockets.
///
/// `rdi` = output buffer pointer, `rsi` = buffer length.
/// Returns bytes written (packed `ConnEntry` records) or negative error.
#[cfg(feature = "net")]
pub const SYS_NET_STAT: u64 = 307;

/// Request a DHCP lease on the default interface.
///
/// No arguments.  Returns 0 on success or a negative error code.
#[cfg(feature = "net")]
pub const SYS_NET_DHCP: u64 = 308;

/// Query the routing table.
///
/// `rdi` = output buffer pointer, `rsi` = buffer length.
/// Returns bytes written (packed `RouteRecord` entries) or negative error.
#[cfg(feature = "net")]
pub const SYS_NET_ROUTES: u64 = 310;

// ── Open flags ────────────────────────────────────────────────────────────────

/// Open for reading only.
pub const O_RDONLY: u32 = 0;
/// Open for writing only.
pub const O_WRONLY: u32 = 1;
/// Open for reading and writing.
pub const O_RDWR: u32 = 2;

// ── Low-level syscall shim ────────────────────────────────────────────────────

/// Perform a raw RustOS syscall via `int 0x80`.
///
/// # ABI
/// | Register | Role |
/// |----------|------|
/// | `rax`    | Syscall number (in) / return value (out) |
/// | `rdi`    | Argument 0 |
/// | `rsi`    | Argument 1 |
/// | `rdx`    | Argument 2 |
///
/// # Safety
/// The caller must ensure the syscall number and all arguments are valid.
/// Passing invalid pointers or sizes is undefined behaviour.
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
#[inline]
pub fn sys_write(fd: u64, buf: &[u8]) -> i64 {
    unsafe { syscall(SYS_WRITE, fd, buf.as_ptr() as u64, buf.len() as u64) }
}

/// Read up to `buf.len()` bytes from `fd` into `buf`.
/// Returns the number of bytes read, or a negative error code.
#[inline]
pub fn sys_read(fd: u64, buf: &mut [u8]) -> i64 {
    unsafe { syscall(SYS_READ, fd, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

/// Terminate the process with the given exit code.
#[inline]
pub fn sys_exit(code: i64) -> ! {
    unsafe { syscall(SYS_EXIT, code as u64, 0, 0) };
    // The kernel never returns from SYS_EXIT.  This loop satisfies `-> !`
    // without requiring `unreachable_unchecked`.
    loop {
        core::hint::spin_loop();
    }
}

/// Open `path` (NUL-terminated byte slice).
/// Returns a non-negative fd on success, or a negative error code.
#[inline]
pub fn open(path: &[u8]) -> i64 {
    unsafe { syscall(SYS_OPEN, path.as_ptr() as u64, O_RDONLY as u64, 0) }
}

/// Open `path` (NUL-terminated byte slice) with `flags`.
/// Returns a non-negative fd on success, or a negative error code.
#[inline]
pub fn open_flags(path: &[u8], flags: u32) -> i64 {
    unsafe { syscall(SYS_OPEN, path.as_ptr() as u64, flags as u64, 0) }
}

/// Close `fd`.  Negative values are silently ignored.
#[inline]
pub fn close(fd: i64) {
    if fd >= 0 {
        unsafe { syscall(SYS_CLOSE, fd as u64, 0, 0) };
    }
}

/// Execute the ELF binary at `path` (NUL-terminated byte slice).
/// Returns the exit code on success, or a negative error code.
#[inline]
pub fn exec(path: &[u8]) -> i64 {
    unsafe { syscall(SYS_EXEC, path.as_ptr() as u64, 0, 0) }
}

/// Wait for a child process.  Returns the child's exit code, or a negative
/// error code.
#[inline]
pub fn waitpid(pid: i64) -> i64 {
    unsafe { syscall(SYS_WAITPID, pid as u64, 0, 0) }
}

/// Create a pipe.  On success, `pipefd[0]` is the read end and `pipefd[1]`
/// is the write end.  Returns 0 on success or a negative error code.
#[inline]
pub fn pipe(pipefd: &mut [i32; 2]) -> i64 {
    unsafe { syscall(SYS_PIPE, pipefd.as_mut_ptr() as u64, 0, 0) }
}

/// Duplicate `oldfd` to `newfd`, closing `newfd` first if necessary.
/// Returns `newfd` on success or a negative error code.
#[inline]
pub fn dup2(oldfd: i64, newfd: i64) -> i64 {
    unsafe { syscall(SYS_DUP2, oldfd as u64, newfd as u64, 0) }
}

/// Fill `buf` with the current working directory string (NUL-terminated).
/// Returns the number of bytes written on success, or a negative error code.
#[inline]
pub fn getcwd(buf: &mut [u8]) -> i64 {
    unsafe { syscall(SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64, 0) }
}

/// Change the working directory to `path` (NUL-terminated byte slice).
/// Returns 0 on success, or a negative error code.
#[inline]
pub fn chdir(path: &[u8]) -> i64 {
    unsafe { syscall(SYS_CHDIR, path.as_ptr() as u64, 0, 0) }
}

/// Read directory entries from `fd` into `buf`.
/// Returns the number of bytes written, 0 when exhausted, or a negative error
/// code.
#[inline]
pub fn getdents64(fd: i64, buf: &mut [u8]) -> i64 {
    unsafe {
        syscall(
            SYS_GETDENTS64,
            fd as u64,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
    }
}

/// Read a single byte from stdin (fd 0), busy-waiting until one is available.
#[inline]
pub fn read_byte() -> u8 {
    let mut b = [0u8; 1];
    loop {
        if sys_read(0, &mut b) > 0 {
            return b[0];
        }
        core::hint::spin_loop();
    }
}

// ── Network syscall wrappers (requires `net` feature) ─────────────────────────

/// Scan for visible WiFi networks.
/// Fills `buf` with packed [`WifiNetworkEntry`] records.
/// Returns the number of bytes written, or a negative error code.
#[cfg(feature = "net")]
#[inline]
pub fn sys_wifi_scan(buf: &mut [u8]) -> i64 {
    unsafe { syscall(SYS_WIFI_SCAN, buf.as_mut_ptr() as u64, buf.len() as u64, 0) }
}

/// Connect to an 802.11 network described by `args`.
/// Returns 0 on success or a negative error code.
#[cfg(feature = "net")]
#[inline]
pub fn sys_wifi_connect(args: &WifiConnectArgs) -> i64 {
    unsafe {
        syscall(
            SYS_WIFI_CONNECT,
            args as *const WifiConnectArgs as u64,
            core::mem::size_of::<WifiConnectArgs>() as u64,
            0,
        )
    }
}

/// Disconnect from the current WiFi network.
/// Returns 0 on success or a negative error code.
#[cfg(feature = "net")]
#[inline]
pub fn sys_wifi_disconnect() -> i64 {
    unsafe { syscall(SYS_WIFI_DISCONNECT, 0, 0, 0) }
}

/// Query current WiFi / IP status into `buf`.
/// Returns bytes written or a negative error code.
#[cfg(feature = "net")]
#[inline]
pub fn sys_wifi_status(buf: &mut [u8]) -> i64 {
    unsafe { syscall(SYS_WIFI_STATUS, buf.as_mut_ptr() as u64, buf.len() as u64, 0) }
}

/// Query interface IP configuration into `buf`.
/// Returns bytes written or a negative error code.
#[cfg(feature = "net")]
#[inline]
pub fn sys_net_ifconfig(buf: &mut [u8]) -> i64 {
    unsafe {
        syscall(
            SYS_NET_IFCONFIG,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
            0,
        )
    }
}

/// Set interface IP configuration from a serialised `IfaceSetArgs` byte slice.
/// Returns 0 on success or a negative error code.
#[cfg(feature = "net")]
#[inline]
pub fn sys_net_ifconfig_set(args: &[u8]) -> i64 {
    unsafe {
        syscall(
            SYS_NET_IFCONFIG_SET,
            args.as_ptr() as u64,
            args.len() as u64,
            0,
        )
    }
}

/// Send an ICMP echo to `dst_ip` (big-endian u32) with the given `seq`.
/// Returns RTT in microseconds or a negative error code.
#[cfg(feature = "net")]
#[inline]
pub fn sys_net_ping(dst_ip: u32, seq: u16) -> i64 {
    unsafe { syscall(SYS_NET_PING, dst_ip as u64, seq as u64, 0) }
}

/// Query active TCP/UDP connections into `buf`.
/// Returns bytes written or a negative error code.
#[cfg(feature = "net")]
#[inline]
pub fn sys_net_stat(buf: &mut [u8]) -> i64 {
    unsafe { syscall(SYS_NET_STAT, buf.as_mut_ptr() as u64, buf.len() as u64, 0) }
}

/// Request a DHCP lease.
/// Returns 0 on success or a negative error code.
#[cfg(feature = "net")]
#[inline]
pub fn sys_net_dhcp() -> i64 {
    unsafe { syscall(SYS_NET_DHCP, 0, 0, 0) }
}

/// Query the routing table into `buf`.
/// Returns bytes written or a negative error code.
#[cfg(feature = "net")]
#[inline]
pub fn sys_net_routes(buf: &mut [u8]) -> i64 {
    unsafe { syscall(SYS_NET_ROUTES, buf.as_mut_ptr() as u64, buf.len() as u64, 0) }
}

// ── Network ABI structs (requires `net` feature) ──────────────────────────────

/// Argument struct for [`sys_wifi_connect`] / [`SYS_WIFI_CONNECT`].
///
/// Packed as a C struct so the kernel can read it directly from the pointer
/// passed in `rdi`.
#[cfg(feature = "net")]
#[repr(C, packed)]
pub struct WifiConnectArgs {
    /// SSID length in bytes (0–32).
    pub ssid_len: u8,
    /// SSID bytes (up to 32 bytes, remainder zero-padded).
    pub ssid: [u8; 32],
    /// Security protocol — use the constants in the [`security_proto`] module
    /// (e.g. [`security_proto::WPA2`]).
    pub security: u8,
    /// Passphrase / key length in bytes.
    pub pass_len: u8,
    /// Passphrase / key bytes (up to 64 bytes, remainder zero-padded).
    pub pass: [u8; 64],
}

/// Security protocol discriminants for [`WifiConnectArgs::security`].
#[cfg(feature = "net")]
pub mod security_proto {
    /// Open network (no security).
    pub const OPEN: u8 = 0;
    /// WEP (RC4-based; legacy).
    pub const WEP: u8 = 1;
    /// WPA-Personal (TKIP).
    pub const WPA: u8 = 2;
    /// WPA2-Personal (CCMP/AES).
    pub const WPA2: u8 = 3;
    /// WPA3-Personal (SAE/Dragonfly).
    pub const WPA3: u8 = 4;
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

/// A stack-allocated byte buffer that implements [`fmt::Write`], flushing to
/// a file descriptor when full or explicitly flushed.
pub struct BufWriter<const N: usize> {
    buf: [u8; N],
    pos: usize,
    fd: u64,
}

impl<const N: usize> BufWriter<N> {
    /// Create a new, empty writer targeting `fd`.
    pub const fn new(fd: u64) -> Self {
        Self { buf: [0u8; N], pos: 0, fd }
    }

    /// Flush buffered bytes to the target file descriptor.
    pub fn flush(&mut self) {
        if self.pos > 0 {
            sys_write(self.fd, &self.buf[..self.pos]);
            self.pos = 0;
        }
    }
}

impl<const N: usize> fmt::Write for BufWriter<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if self.pos == N {
                self.flush();
            }
            self.buf[self.pos] = bytes[i];
            self.pos += 1;
            i += 1;
        }
        Ok(())
    }
}

/// Write formatted output to stdout (fd 1) using a 512-byte scratch buffer.
///
/// Prefer the [`print!`] / [`println!`] macros for convenience.
pub fn print_fmt(args: fmt::Arguments) {
    let mut w = BufWriter::<512>::new(1);
    let _ = w.write_fmt(args);
    w.flush();
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

/// Print formatted text to stdout (no trailing newline).
///
/// # Example
/// ```ignore
/// print!("x = {}", 42);
/// ```
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::print_fmt(::core::format_args!($($arg)*))
    };
}

/// Print formatted text to stdout with a trailing newline.
///
/// # Example
/// ```ignore
/// println!("Hello from RustOS!");
/// println!("x = {}", 42);
/// ```
#[macro_export]
macro_rules! println {
    () => { $crate::print("\n") };
    ($($arg:tt)*) => {{
        $crate::print_fmt(::core::format_args!($($arg)*));
        $crate::print("\n");
    }};
}

// ── Entry-point glue ──────────────────────────────────────────────────────────

extern "Rust" {
    /// User programs must define `fn main() -> i64`.  The return value is used
    /// as the process exit code.
    fn main() -> i64;
}

/// ELF entry point — zero the BSS segment, call `main`, then exit.
///
/// # Safety
/// Called by the kernel's ELF loader; the stack is initialised, but there is
/// no C runtime or TLS.  Do not use thread-locals or any library that assumes
/// them.
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    // Zero the BSS segment so that all `static` variables start at zero.
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
