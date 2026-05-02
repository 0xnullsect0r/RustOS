# rustos-rt

Rust userspace runtime for **RustOS** — the companion crate that lets you write
`no_std` programs that run inside the RustOS kernel.

## What it provides

| Symbol | Description |
|--------|-------------|
| `_start` | ELF entry point; zeroes BSS, calls `main()`, then `sys_exit`. |
| `sys_write(fd, buf)` | Write bytes to a file descriptor (1 = stdout). |
| `sys_read(fd, buf)` | Read bytes from a file descriptor (0 = stdin). |
| `sys_exit(code)` | Terminate the process. |
| `print(s)` / `println(s)` | Convenience wrappers over `sys_write`. |

All syscalls use **`int 0x80`** with the following register ABI:

| Register | Meaning |
|----------|---------|
| `rax` | Syscall number (in) / return value (out) |
| `rdi` | Argument 0 |
| `rsi` | Argument 1 |
| `rdx` | Argument 2 |

## Syscall numbers

| Number | Name | Description |
|--------|------|-------------|
| 0 | `SYS_READ` | Read from fd |
| 1 | `SYS_WRITE` | Write to fd |
| 2 | `SYS_OPEN` | Open file (path in rdi, flags in rsi) |
| 3 | `SYS_CLOSE` | Close fd |
| 60 | `SYS_EXIT` | Exit process |

## Building a program

### Prerequisites

```
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
```

### Example

```rust
// src/main.rs
#![no_std]
#![no_main]

use rustos_rt::{println, sys_exit};

#[no_mangle]
fn main() -> i64 {
    println("Hello from RustOS!");
    sys_exit(0);
}
```

```toml
# Cargo.toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2021"

[dependencies]
rustos-rt = { path = "path/to/crates/rustos-rt" }

[profile.release]
panic = "abort"
opt-level = "s"
strip = true
```

```bash
cargo +nightly build \
  --target path/to/crates/rustos-rt/x86_64-unknown-rustos.json \
  -Z build-std=core \
  --release
```

The resulting ELF lives at `target/x86_64-unknown-rustos/release/<name>`.

### Trying the bundled example

```bash
cd crates/rustos-rt
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
* No red-zone (`disable-redzone: true`)
* Panic strategy: `abort`
* Linker: `rust-lld` with `-Trustos-link.x` (the bundled linker script)

The linker script (`rustos-link.x`) places the program at virtual address
`0x0040_0000` (4 MiB), which is above the kernel's identity-mapped region.
