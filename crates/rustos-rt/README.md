# rustos-rt

Rust userspace runtime for **RustOS** — the shared crate that lets you write
`no_std` programs that run inside the RustOS kernel.

Add it as a normal Cargo dependency; it supplies the ELF entry point (`_start`),
a complete set of syscall wrappers, `print!`/`println!` macros, and the target
spec needed to cross-compile for RustOS.

## What it provides

| Symbol | Description |
|--------|-------------|
| `_start` | ELF entry point; zeroes BSS, calls `main()`, then `sys_exit`. |
| `sys_write(fd, buf)` | Write bytes to a file descriptor. |
| `sys_read(fd, buf)` | Read bytes from a file descriptor. |
| `sys_exit(code)` | Terminate the process. |
| `open(path)` | Open a file (read-only). |
| `open_flags(path, flags)` | Open a file with explicit flags. |
| `close(fd)` | Close a file descriptor. |
| `exec(path)` | Execute an ELF binary. |
| `waitpid(pid)` | Wait for a child process. |
| `pipe(pipefd)` | Create a pipe. |
| `dup2(old, new)` | Duplicate a file descriptor. |
| `getcwd(buf)` | Get the current working directory. |
| `chdir(path)` | Change the working directory. |
| `getdents64(fd, buf)` | Read directory entries. |
| `read_byte()` | Read one byte from stdin (busy-wait). |
| `print(s)` / `println(s)` | Write a `&str` to stdout. |
| `print!(…)` / `println!(…)` | Formatted output macros (`core::fmt`). |
| `#[panic_handler]` | Calls `sys_exit(1)` on panic. |

Network syscall wrappers (`sys_wifi_scan`, `sys_wifi_connect`, etc.) are
available with the **`net`** feature — see [Network syscalls](#network-syscalls)
below.

## Syscall numbers

All syscalls use **`int 0x80`** with the following register ABI:

| Register | Meaning |
|----------|---------|
| `rax` | Syscall number (in) / return value (out) |
| `rdi` | Argument 0 |
| `rsi` | Argument 1 |
| `rdx` | Argument 2 |

### Standard syscalls

| Number | Constant | Description |
|--------|----------|-------------|
| 0 | `SYS_READ` | Read from fd |
| 1 | `SYS_WRITE` | Write to fd |
| 2 | `SYS_OPEN` | Open file |
| 3 | `SYS_CLOSE` | Close fd |
| 22 | `SYS_PIPE` | Create pipe |
| 33 | `SYS_DUP2` | Duplicate fd |
| 59 | `SYS_EXEC` | Execute ELF binary |
| 60 | `SYS_EXIT` | Exit process |
| 61 | `SYS_WAITPID` | Wait for child |
| 79 | `SYS_GETCWD` | Get working directory |
| 80 | `SYS_CHDIR` | Change directory |
| 217 | `SYS_GETDENTS64` | Read directory entries |

### Network syscalls (`net` feature)

| Number | Constant | Description |
|--------|----------|-------------|
| 300 | `SYS_WIFI_SCAN` | Scan for 802.11 networks |
| 301 | `SYS_WIFI_CONNECT` | Connect to a network |
| 302 | `SYS_WIFI_DISCONNECT` | Disconnect |
| 303 | `SYS_WIFI_STATUS` | Query WiFi/IP status |
| 304 | `SYS_NET_IFCONFIG` | Query interface config |
| 305 | `SYS_NET_IFCONFIG_SET` | Set interface config |
| 306 | `SYS_NET_PING` | ICMP echo |
| 307 | `SYS_NET_STAT` | Query active connections |
| 308 | `SYS_NET_DHCP` | Request DHCP lease |
| 310 | `SYS_NET_ROUTES` | Query routing table |

## Building a program

### Prerequisites

```
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
```

### Add as a dependency

```toml
# Cargo.toml
[dependencies]
rustos-rt = { git = "https://github.com/RustOS-Dev/rustos-rt" }

# For network tools, enable the `net` feature:
# rustos-rt = { git = "https://github.com/RustOS-Dev/rustos-rt", features = ["net"] }

[profile.release]
panic = "abort"
opt-level = "s"
strip = true
lto = true
codegen-units = 1

[profile.dev]
panic = "abort"
```

### Minimal example

```rust
// src/main.rs
#![no_std]
#![no_main]

use rustos_rt::sys_exit;

#[no_mangle]
fn main() -> i64 {
    rustos_rt::println!("Hello from RustOS!");
    sys_exit(0);
}
```

```bash
cargo +nightly build \
  --target path/to/x86_64-unknown-rustos.json \
  -Z build-std=core \
  --release
```

The resulting ELF lives at `target/x86_64-unknown-rustos/release/<name>`.

### Trying the bundled example

```bash
cd rustos-rt
cargo +nightly build --example hello \
  --target x86_64-unknown-rustos.json \
  -Z build-std=core \
  --release
```

## Running on RustOS

1. Format a USB flash drive as **FAT32**.
2. Copy your ELF onto it (e.g. `hello`).
3. Boot RustOS in QEMU with the USB drive attached:
   ```
   qemu-system-x86_64 ... \
     -device qemu-xhci,id=xhci \
     -drive if=none,id=usbdisk,file=disk.img,format=raw \
     -device usb-storage,bus=xhci.0,drive=usbdisk
   ```
4. From the shell:
   ```
   ls /usb
   exec /usb/hello
   ```

## Target JSON notes

The target spec (`x86_64-unknown-rustos.json`) sets:

* LLVM target: `x86_64-unknown-none`
* No SSE/MMX (kernel doesn't save FPU state for user processes yet)
* `rustc-abi`: `x86-softfloat`
* No red-zone (`disable-redzone: true`)
* Panic strategy: `abort`
* Linker: `rust-lld` with `-Trustos-link.x` (the bundled linker script)

The linker script (`rustos-link.x`) places the program at virtual address
`0x0040_0000` (4 MiB), which is above the kernel's identity-mapped region.

## Organisation repos using rustos-rt

| Repo | Purpose |
|------|---------|
| [`rsh`](https://github.com/RustOS-Dev/rsh) | RustOS shell (`/bin/rsh`) |
| [`tcp-ip`](https://github.com/RustOS-Dev/tcp-ip) | TCP/IP stack + WiFi CLI tools |