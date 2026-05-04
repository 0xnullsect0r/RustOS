# RustOS Development Guide

**Version**: 1.0  
**Last Updated**: 2026-05-04

This guide provides practical information for developers working on RustOS, including setup instructions, development workflows, debugging techniques, and contribution guidelines.

---

## Table of Contents

1. [Getting Started](#getting-started)
2. [Development Environment](#development-environment)
3. [Building RustOS](#building-rustos)
4. [Testing](#testing)
5. [Debugging](#debugging)
6. [Code Style and Conventions](#code-style-and-conventions)
7. [Contributing](#contributing)
8. [Common Development Tasks](#common-development-tasks)
9. [Troubleshooting](#troubleshooting)

---

## Getting Started

### Prerequisites

**Operating System**:
- Linux (recommended: Ubuntu 22.04+, Fedora 38+, Arch Linux)
- macOS (Intel or Apple Silicon with Rosetta 2)
- Windows (via WSL2)

**Required Tools**:
```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install nightly
rustup component add rust-src llvm-tools-preview --toolchain nightly
rustup default nightly

# bootimage tool
cargo install bootimage

# QEMU (for testing)
# Ubuntu/Debian:
sudo apt install qemu-system-x86 ovmf

# Fedora:
sudo dnf install qemu-system-x86 edk2-ovmf

# Arch Linux:
sudo pacman -S qemu edk2-ovmf

# macOS (via Homebrew):
brew install qemu
```

**Optional Tools** (recommended):
```bash
# GDB for debugging
sudo apt install gdb        # Linux
brew install gdb            # macOS

# Hex editor (for inspecting binaries)
sudo apt install xxd hexyl

# Disk utilities
sudo apt install parted e2fsprogs dosfstools
```

### Cloning the Repository

```bash
# Clone with submodules
git clone --recurse-submodules https://github.com/RustOS-Dev/RustOS.git
cd RustOS

# Or if already cloned, initialize submodules
git submodule update --init --recursive
```

### First Build

```bash
# Check that everything compiles
cargo check

# Build the kernel
cargo build

# Build bootable image
cargo bootimage

# Run in QEMU
cargo run
```

Expected output:
```
   Compiling rustos v0.1.0 (/path/to/RustOS)
    Finished dev [unoptimized + debuginfo] target(s) in 45.2s
     Running `bootimage runner target/x86_64-rustos/debug/bootimage-rustos.bin`
```

QEMU should open and display the RustOS boot sequence.

---

## Development Environment

### Recommended IDE Setup

**Visual Studio Code** with extensions:
- `rust-analyzer` - Rust language server
- `CodeLLDB` - Debugger
- `Even Better TOML` - TOML syntax highlighting
- `x86 and x86_64 Assembly` - ASM syntax highlighting

**VS Code Settings** (`.vscode/settings.json`):
```json
{
    "rust-analyzer.checkOnSave.allTargets": false,
    "rust-analyzer.cargo.target": "x86_64-rustos.json",
    "rust-analyzer.cargo.buildScripts.enable": true,
    "rust-analyzer.procMacro.enable": true,
    "editor.formatOnSave": true,
    "[rust]": {
        "editor.defaultFormatter": "rust-lang.rust-analyzer"
    }
}
```

**VS Code Tasks** (`.vscode/tasks.json`):
```json
{
    "version": "2.0.0",
    "tasks": [
        {
            "label": "build",
            "type": "shell",
            "command": "cargo build",
            "group": {
                "kind": "build",
                "isDefault": true
            }
        },
        {
            "label": "run",
            "type": "shell",
            "command": "cargo run",
            "group": "test"
        },
        {
            "label": "test",
            "type": "shell",
            "command": "cargo test",
            "group": "test"
        }
    ]
}
```

**Vim/Neovim** with plugins:
- `rust.vim` or `rust-tools.nvim`
- `nvim-lspconfig` with rust-analyzer
- `nvim-cmp` for completion

### Environment Variables

Useful environment variables for development:

```bash
# Enable colored cargo output
export CARGO_TERM_COLOR=always

# Increase Rust backtrace detail
export RUST_BACKTRACE=1  # or "full" for even more detail

# Use nightly by default
export RUSTUP_TOOLCHAIN=nightly

# Increase build parallelism
export CARGO_BUILD_JOBS=8  # adjust to your CPU core count
```

Add to `~/.bashrc` or `~/.zshrc` for persistence.

---

## Building RustOS

### Build Targets

**Development Build** (fast, with debug symbols):
```bash
cargo build
# Output: target/x86_64-rustos/debug/rustos
```

**Release Build** (optimized, smaller):
```bash
cargo build --release
# Output: target/x86_64-rustos/release/rustos
```

**Bootable Image** (includes bootloader):
```bash
cargo bootimage               # debug
cargo bootimage --release     # release
# Output: target/x86_64-rustos/debug/bootimage-rustos.bin
```

### Build System Details

**Custom Target** (`x86_64-rustos.json`):
- Bare-metal target (no OS)
- Soft-float (no FPU)
- Red zone disabled (required for interrupts)
- LLD linker

**build.rs Script**:
1. Updates git submodules (tcp-ip, rsh)
2. Builds tcp-ip userspace tools (wifi, ping, ifconfig, netstat)
3. Generates `net_bins.rs` with embedded ELF binaries
4. Sets rerun triggers for cargo

**Cargo.toml Configuration**:
```toml
[dependencies]
# Core dependencies
bootloader = { version = "0.11" }
spin = "0.9"
lazy_static = { version = "1.4", features = ["spin_no_std"] }
x86_64 = "0.15"

# Filesystem
fatfs = { version = "0.3", default-features = false }

# Async
futures-util = { version = "0.3", default-features = false, features = ["alloc"] }

# USB
xhci = "0.9"

# Submodules
tcp-ip = { path = "tcp-ip" }
```

### Cleaning Build Artifacts

```bash
# Clean all build artifacts
cargo clean

# Clean only debug build
rm -rf target/x86_64-rustos/debug

# Clean only submodule builds
rm -rf tcp-ip/target
rm -rf rsh/target
```

### Incremental Compilation

Incremental compilation is enabled by default in debug builds:

```toml
# Cargo.toml
[profile.dev]
incremental = true  # default
```

For faster rebuilds, consider using `sccache`:
```bash
cargo install sccache
export RUSTC_WRAPPER=sccache
```

---

## Testing

### Running Tests

**All Tests**:
```bash
cargo test
```

**Specific Test**:
```bash
cargo test --test test_name
```

**With Output**:
```bash
cargo test -- --nocapture
```

### Test Framework

RustOS uses a custom test framework that runs tests in QEMU:

```rust
// tests/my_test.rs
#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(rustos::test_runner)]
#![reexport_test_harness_main = "test_main"]

use rustos::{serial_print, serial_println};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    rustos::init();
    test_main();
    loop {}
}

#[test_case]
fn trivial_test() {
    serial_print!("trivial_test... ");
    assert_eq!(1 + 1, 2);
    serial_println!("[ok]");
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    rustos::test_panic_handler(info)
}
```

### Test Output

Tests write to serial port (COM1). QEMU is configured to write serial output to stdout.

Example output:
```
Running 3 tests
test_allocation... [ok]
test_vfs_mount... [ok]
test_syscall... [ok]
```

### Writing Good Tests

**Unit Tests** (in module):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test_case]
    fn test_parse_fat32() {
        let bpb = parse_bpb(&SAMPLE_BPB);
        assert_eq!(bpb.bytes_per_sector, 512);
    }
}
```

**Integration Tests** (in tests/ directory):
- Test kernel functionality from outside
- Each file is a separate binary
- Has its own test harness

**Test Guidelines**:
- Test one thing per test function
- Use descriptive test names
- Add comments explaining non-obvious test logic
- Clean up resources (close files, deallocate, etc.)
- Don't rely on test execution order

### Manual Testing in QEMU

```bash
# Run with serial output
cargo run

# Run with specific disk image
qemu-system-x86_64 \
    -drive format=raw,file=target/.../bootimage-rustos.bin \
    -drive if=none,id=usbdisk,file=test_disk.img,format=raw \
    -device qemu-xhci,id=xhci \
    -device usb-storage,bus=xhci.0,drive=usbdisk \
    -serial stdio
```

### Creating Test Disk Images

```bash
# Create FAT32 test disk
dd if=/dev/zero of=test_disk.img bs=1M count=32
mkfs.fat -F 32 test_disk.img

# Mount and add files
mkdir -p /tmp/test_mount
sudo mount -o loop test_disk.img /tmp/test_mount
sudo cp test_files/* /tmp/test_mount/
sudo umount /tmp/test_mount
```

---

## Debugging

### Serial Debugging

Serial output is your best friend for kernel debugging:

```rust
serial_println!("[debug] Entering function foo()");
serial_println!("[debug] Variable x = {:?}", x);
serial_println!("[debug] Pointer address: {:p}", ptr);
```

**Redirect serial to file**:
```bash
qemu-system-x86_64 ... -serial file:serial.log
```

### GDB Debugging

**Terminal 1** (QEMU with GDB stub):
```bash
qemu-system-x86_64 \
    -drive format=raw,file=target/.../bootimage-rustos.bin \
    -serial stdio \
    -s -S
# -s: GDB server on port 1234
# -S: Halt CPU at startup
```

**Terminal 2** (GDB):
```bash
gdb target/x86_64-rustos/debug/rustos

# Connect to QEMU
(gdb) target remote :1234

# Set breakpoint
(gdb) break kernel_main
(gdb) break src/main.rs:50

# Continue execution
(gdb) continue

# Step through code
(gdb) step
(gdb) next

# Inspect variables
(gdb) print my_variable
(gdb) info registers
(gdb) x/10i $rip  # Disassemble 10 instructions

# Backtrace
(gdb) bt
```

### QEMU Monitor

Access QEMU monitor for hardware inspection:

**Switch to monitor**: Press `Ctrl+Alt+2` (or `Ctrl+Alt+3` depending on config)

**Useful commands**:
```
(qemu) info registers      # Show CPU registers
(qemu) info pci            # Show PCI devices
(qemu) info mtree          # Show memory map
(qemu) x /10x 0x1000       # Dump memory at address
(qemu) xp /10x 0x1000      # Dump physical memory
```

**Switch back to serial**: Press `Ctrl+Alt+1`

### Panic Messages

When kernel panics, a message is printed:

```
PANIC in src/main.rs:100:5
  'assertion failed: x > 0'

Stack trace:
  0: rustos::panic_handler
  1: rustos::main::kernel_main
```

**Investigating Panics**:
1. Note the file and line number
2. Check the assertion/panic message
3. Review code around that line
4. Check serial output for preceding debug messages
5. Use GDB to inspect state at panic point

### Debugging Techniques

**Assertion Checks**:
```rust
assert!(ptr.is_not_null(), "Pointer is null");
assert_eq!(size, 512, "Unexpected size");
debug_assert!(expensive_check()); // Only in debug builds
```

**Conditional Compilation**:
```rust
#[cfg(debug_assertions)]
fn log_debug_info() {
    serial_println!("Debug info: ...");
}
```

**Memory Dump**:
```rust
fn dump_memory(addr: *const u8, len: usize) {
    serial_println!("Memory dump at {:p}:", addr);
    for i in 0..len {
        if i % 16 == 0 {
            serial_print!("\n{:04x}: ", i);
        }
        serial_print!("{:02x} ", unsafe { *addr.add(i) });
    }
    serial_println!();
}
```

### Common Issues

**Issue**: Page fault
- **Cause**: Accessing unmapped memory
- **Debug**: Check CR2 register (faulting address), verify page table mappings

**Issue**: Triple fault (QEMU exits immediately)
- **Cause**: Exception during exception handler
- **Debug**: Enable `-d int` flag in QEMU, check serial output

**Issue**: Hang (no output)
- **Cause**: Infinite loop or waiting for interrupt that never comes
- **Debug**: Use GDB to break and check current instruction

---

## Code Style and Conventions

### Formatting

Use `cargo fmt` to automatically format code:

```bash
# Format all code
cargo fmt

# Check formatting without making changes
cargo fmt -- --check
```

Configuration is in `rustfmt.toml` (if present).

### Linting

Use `cargo clippy` for lints:

```bash
# Run clippy
cargo clippy

# Fail on warnings (for CI)
cargo clippy -- -D warnings
```

Common clippy lints to follow:
- Avoid `unwrap()` in production code
- Use `?` for error propagation
- Prefer iterators over manual loops
- Use `const` for constants

### Naming Conventions

**Variables and Functions**: `snake_case`
```rust
let frame_allocator = BootInfoFrameAllocator::new();
fn init_heap() { }
```

**Types and Traits**: `PascalCase`
```rust
struct FrameBufferWriter { }
trait FileSystem { }
enum Color { }
```

**Constants**: `SCREAMING_SNAKE_CASE`
```rust
const HEAP_SIZE: usize = 16 * 1024 * 1024;
static ALLOCATOR: Mutex<Allocator> = Mutex::new(Allocator::new());
```

**Modules**: `snake_case`
```rust
mod mass_storage;
mod x86_64;
```

### Documentation

**Public Items**: Always document with `///`
```rust
/// Initializes the network stack.
///
/// # Requirements
/// - PCI subsystem must be initialized first
/// - BAR0 must be mappable
///
/// # Panics
/// Panics if no Intel AX210 device is found.
pub fn init() {
    // ...
}
```

**Module-Level Docs**: Use `//!`
```rust
//! USB XHCI host controller driver.
//!
//! This module implements a USB 3.0 XHCI driver supporting
//! bulk transfers for mass storage devices.
```

**Internal Comments**: Use `//` for implementation notes
```rust
// SAFETY: BAR0 is guaranteed to be mapped by the bootloader
unsafe {
    let ptr = bar0 as *mut u32;
}
```

### Error Handling

**Prefer Result over panic**:
```rust
// Good
pub fn open_file(path: &str) -> Result<File, FileError> {
    if !path.starts_with('/') {
        return Err(FileError::InvalidPath);
    }
    // ...
}

// Bad (don't panic in library code)
pub fn open_file(path: &str) -> File {
    assert!(path.starts_with('/'), "Path must be absolute");
    // ...
}
```

**Use ? operator**:
```rust
pub fn read_config() -> Result<Config, Error> {
    let file = open_file("/config.txt")?;
    let contents = file.read_to_string()?;
    Ok(parse_config(&contents)?)
}
```

**Custom Error Types**:
```rust
#[derive(Debug)]
pub enum FileSystemError {
    NotFound,
    PermissionDenied,
    IoError,
}
```

### Unsafe Code

**Always document why unsafe is needed**:
```rust
// SAFETY: The pointer is valid because:
// 1. It was allocated via DmaAllocator
// 2. The size matches the allocation size
// 3. The allocation is not deallocated until this function returns
unsafe {
    ptr::write_volatile(ptr, value);
}
```

**Minimize unsafe blocks**:
```rust
// Good: Small unsafe block
let value = unsafe { *ptr };
process_value(value);

// Bad: Large unsafe block
unsafe {
    let value = *ptr;
    let result = process_value(value);
    *output = result;
}
```

---

## Contributing

### Contribution Workflow

1. **Fork the repository** on GitHub

2. **Clone your fork**:
   ```bash
   git clone https://github.com/YOUR_USERNAME/RustOS.git
   cd RustOS
   git remote add upstream https://github.com/RustOS-Dev/RustOS.git
   ```

3. **Create a feature branch**:
   ```bash
   git checkout -b feature/my-feature
   ```

4. **Make your changes**:
   - Follow code style guidelines
   - Add tests for new functionality
   - Update documentation

5. **Test your changes**:
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   cargo run  # Manual testing
   ```

6. **Commit your changes**:
   ```bash
   git add .
   git commit -m "feat: add feature X"
   ```

   Use conventional commits:
   - `feat:` - New feature
   - `fix:` - Bug fix
   - `docs:` - Documentation changes
   - `test:` - Adding or updating tests
   - `refactor:` - Code refactoring
   - `chore:` - Build system, dependencies

7. **Push to your fork**:
   ```bash
   git push origin feature/my-feature
   ```

8. **Open a Pull Request** on GitHub

### PR Guidelines

**PR Title**: Use conventional commit format
```
feat: add Intel AX210 network driver
fix: resolve page fault in USB driver
docs: update README with new features
```

**PR Description** should include:
- Summary of changes
- Motivation for changes
- Testing performed
- Any breaking changes
- Related issues (if applicable)

**Example**:
```markdown
## Summary
Add support for Intel AX210 Wi-Fi 6E adapter.

## Motivation
Enable network connectivity for RustOS.

## Changes
- Added PCI device discovery for AX210
- Integrated tcp-ip submodule
- Added network syscalls (300-310)
- Added userspace tools (wifi, ping, etc.)

## Testing
- Tested in QEMU with emulated AX210
- Tested on real hardware (ThinkPad X1 Carbon Gen 10)
- All existing tests pass

## Breaking Changes
None
```

### Code Review Process

1. Maintainers will review your PR
2. Address any requested changes
3. Once approved, maintainers will merge

**Review Criteria**:
- Code follows style guidelines
- Tests pass
- Documentation is updated
- No breaking changes (or justified)
- Performance is acceptable

---

## Common Development Tasks

### Adding a New Driver

1. Create driver module:
   ```bash
   touch src/drivers/my_device.rs
   ```

2. Add to `src/lib.rs`:
   ```rust
   pub mod drivers {
       pub mod my_device;
       // ...
   }
   ```

3. Implement driver (see COPILOT_INSTRUCTIONS.md for template)

4. Add init call to `src/main.rs`:
   ```rust
   rustos::drivers::my_device::init();
   ```

5. Add tests:
   ```bash
   touch tests/test_my_device.rs
   ```

### Adding a New Syscall

1. Define syscall number in `src/syscall/mod.rs`:
   ```rust
   const SYS_MY_SYSCALL: u64 = 999;
   ```

2. Implement handler:
   ```rust
   fn sys_my_syscall(arg1: u64) -> i64 {
       // implementation
       0
   }
   ```

3. Add to dispatcher:
   ```rust
   match nr {
       999 => sys_my_syscall(a1),
       // ...
   }
   ```

4. Update rustos-rt (if needed for userspace)

### Adding a Shell Command

1. Add function to `src/shell/commands.rs`:
   ```rust
   pub fn cmd_mycommand(args: &[&str]) {
       println!("Hello from mycommand");
   }
   ```

2. Register command:
   ```rust
   match cmd {
       "mycommand" => cmd_mycommand(args),
       // ...
   }
   ```

### Updating Submodules

**Update to latest**:
```bash
git submodule update --remote tcp-ip rsh
git add tcp-ip rsh
git commit -m "chore: update submodules to latest"
```

**Make changes in submodule**:
```bash
cd tcp-ip
git checkout -b feature/my-change
# Make changes
git commit -am "feat: my changes"
git push origin feature/my-change
# Open PR in tcp-ip repo
```

---

## Troubleshooting

### Build Issues

**Error**: `error: linking with `rust-lld` failed`
- **Solution**: Clean and rebuild
  ```bash
  cargo clean
  cargo build
  ```

**Error**: `error: no default toolchain configured`
- **Solution**: Install Rust nightly
  ```bash
  rustup toolchain install nightly
  rustup default nightly
  ```

**Error**: `error: component 'rust-src' is missing`
- **Solution**: Install rust-src
  ```bash
  rustup component add rust-src
  ```

### Runtime Issues

**Blank screen after boot**:
- Check serial output for errors
- Verify framebuffer initialization
- Try VGA fallback (if on real hardware, boot via BIOS/CSM)

**Kernel panic immediately**:
- Check serial output for panic message
- Use GDB to inspect state
- Review recent changes

**USB device not detected**:
- Check PCI enumeration (`lspci` command in shell)
- Verify XHCI controller initialization
- Try different USB port

### Testing Issues

**Tests timeout**:
- Increase timeout in test runner
- Check if test is hanging in infinite loop
- Use serial output to see where it stops

**Tests fail in CI but pass locally**:
- Check CI logs for specific error
- Verify all dependencies are installed in CI
- May be timing-related (add delays)

---

## Resources

- [RustOS GitHub Repository](https://github.com/RustOS-Dev/RustOS)
- [Rust OS Development Tutorial](https://os.phil-opp.com/)
- [OSDev Wiki](https://wiki.osdev.org/)
- [Rust Embedded Book](https://rust-embedded.github.io/book/)

---

**Document Version**: 1.0  
**Last Updated**: 2026-05-04  
**Maintainer**: RustOS-Dev Team
