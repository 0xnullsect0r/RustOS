# RustOS Development Guide for AI Agents

**Last Updated**: 2026-05-04  
**Target Audience**: AI coding assistants (GitHub Copilot, Claude, etc.)

This document provides comprehensive guidance for AI agents working on the RustOS kernel project. It covers architecture, conventions, common tasks, and critical implementation details.

---

## Table of Contents

1. [Repository Overview](#repository-overview)
2. [Architecture](#architecture)
3. [Build System](#build-system)
4. [Development Workflow](#development-workflow)
5. [Key Subsystems](#key-subsystems)
6. [Coding Conventions](#coding-conventions)
7. [Common Tasks](#common-tasks)
8. [Testing and Debugging](#testing-and-debugging)
9. [Critical Implementation Details](#critical-implementation-details)
10. [Submodules](#submodules)
11. [Troubleshooting](#troubleshooting)

---

## Repository Overview

### Project Structure

```
RustOS/
├── .github/
│   └── workflows/          # CI/CD pipelines (check, fmt, clippy, test)
├── assets/
│   └── font8x16.bin        # Framebuffer bitmap font (95 ASCII chars)
├── crates/
│   └── rustos-rt/          # Userspace runtime library (minimal std replacement)
├── rsh/                    # Shell submodule (RustOS-Dev/rsh)
├── tcp-ip/                 # Network stack submodule (RustOS-Dev/tcp-ip)
├── src/
│   ├── main.rs             # Kernel entry point
│   ├── lib.rs              # Crate root, test harness
│   ├── arch/x86_64/        # Architecture-specific code
│   │   ├── gdt.rs          # Global Descriptor Table
│   │   ├── interrupts.rs   # IDT, exception/IRQ handlers
│   │   └── memory/         # Paging, frame allocators, DMA
│   ├── drivers/            # Device drivers
│   │   ├── framebuffer.rs  # UEFI GOP framebuffer (primary output)
│   │   ├── vga.rs          # VGA text mode (fallback)
│   │   └── serial.rs       # UART 16550 serial port
│   ├── allocator/          # Heap allocator (fixed-size block)
│   ├── task/               # Async executor, keyboard task
│   ├── pci/                # PCI bus enumeration
│   ├── usb/                # USB stack
│   │   ├── xhci/           # XHCI host controller driver
│   │   └── mass_storage/   # USB MSC (flash drives)
│   ├── fs/fat32/           # FAT32 filesystem
│   ├── vfs/                # Virtual filesystem layer
│   ├── net.rs              # Network stack integration (tcp-ip)
│   ├── process/            # ELF loader, process execution
│   ├── syscall/            # System call dispatcher
│   ├── shell/              # Built-in kernel shell
│   └── bin_commands.rs     # Virtual /bin commands
├── tests/                  # Integration tests
├── build.rs                # Build script (submodule updates, ELF embedding)
├── Cargo.toml              # Dependencies and build config
├── rust-toolchain          # Pins nightly Rust version
├── x86_64-rustos.json      # Custom target triple
└── run-qemu-uefi.sh        # QEMU test runner script

Key Files:
- README.md                 # User documentation
- ARCHITECTURE.md           # System design docs (create if missing)
- DEVELOPMENT.md            # Developer guide (create if missing)
- NETWORK_INTEGRATION.md    # TCP/IP integration details
- FRAMEBUFFER_IMPLEMENTATION.md # Framebuffer driver implementation
- COPILOT_INSTRUCTIONS.md   # This file
```

### Technology Stack

- **Language**: Rust nightly (see `rust-toolchain`)
- **Target**: Custom `x86_64-rustos.json` (bare-metal, no_std)
- **Bootloader**: `bootloader 0.11` (UEFI-capable BIOS bootloader)
- **Build System**: `cargo bootimage` (wraps bootloader + kernel)
- **Test Runner**: Custom test harness via QEMU
- **CI/CD**: GitHub Actions (fmt, clippy, check, test)

---

## Architecture

### Boot Sequence

```
UEFI Firmware
    ↓
Bootloader (bootloader 0.11)
    ├── Initialize GOP framebuffer
    ├── Setup page tables (identity map + offset map)
    ├── Load kernel ELF
    ├── Pass BootInfo struct
    └── Jump to kernel_main()
        ↓
Kernel Initialization (src/main.rs::kernel_main)
    ├── Disable interrupts (x86_64::instructions::interrupts::disable)
    ├── Initialize serial port (UART 16550)
    ├── Initialize framebuffer (if available from BootInfo)
    ├── rustos::init() [src/lib.rs]
    │   ├── GDT setup (arch/x86_64/gdt.rs)
    │   ├── IDT setup (arch/x86_64/interrupts.rs)
    │   ├── PIC initialization (8259 PIC)
    │   └── Enable interrupts
    ├── Initialize heap allocator
    ├── Initialize VFS (mount table)
    ├── Initialize PCI subsystem
    ├── Initialize USB (XHCI discovery and init)
    ├── Mount USB storage as root (/)
    ├── Initialize network stack (tcp-ip)
    ├── Enable interrupts
    ├── Start async executor
    └── Launch shell (/bin/rsh)
```

### Memory Layout

```
Virtual Memory Map (after bootloader setup):
0x0000_0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF  User space (not yet used)
0xFFFF_8000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF  Kernel space

Physical Memory Offset Mapping:
Physical 0x0000_0000 → Virtual PHYS_MEM_OFFSET + 0x0000_0000
(All physical memory accessible via offset)

Heap: Dynamically allocated via FixedSizeBlockAllocator
Stack: Per-process stack (kernel stack in high memory)
```

### Key Architectural Patterns

1. **Global Statics with Spinlocks**
   - Most subsystems use `lazy_static!` + `spin::Mutex`
   - Example: `VFS`, `USB_XHCI`, `FRAMEBUFFER_WRITER`
   - Always lock briefly, never hold locks across async points

2. **Async Executor**
   - Simple cooperative task executor (no preemption)
   - Used for keyboard input processing
   - Lives in `src/task/`

3. **Virtual Filesystem**
   - Central `VFS` struct holds mount table
   - Filesystems implement `FileSystem` trait
   - Files implement `File` trait
   - Paths start with `/`

4. **Syscall Interface**
   - Uses `int 0x80` software interrupt
   - Naked function `syscall_handler()` saves/restores registers
   - Dispatcher `dispatch()` returns i64 result
   - Syscall numbers: 0-99 (file I/O), 100-199 (process), 300-310 (network)

5. **Submodule Integration**
   - `rsh` and `tcp-ip` are git submodules at repo root
   - Auto-updated to latest by `build.rs` on every build
   - `tcp-ip` binaries (wifi, ping, etc.) embedded at build time

---

## Build System

### Prerequisites

```bash
# Rust toolchain
rustup toolchain install nightly
rustup component add rust-src llvm-tools-preview --toolchain nightly

# Build tools
cargo install bootimage

# Testing/running
sudo apt install qemu-system-x86 ovmf  # Linux
brew install qemu                      # macOS
```

### Build Commands

```bash
# Check compilation
cargo check

# Format code
cargo fmt

# Lint
cargo clippy

# Build kernel
cargo build                  # Debug
cargo build --release        # Release

# Build bootable image
cargo bootimage              # Creates target/.../bootimage-rustos.bin
cargo bootimage --release

# Run in QEMU
cargo run                    # Uses run-args from Cargo.toml

# Run tests
cargo test                   # Integration tests in QEMU
```

### Build Process (build.rs)

The `build.rs` script runs before compilation:

1. **Update Submodules**
   - Runs `git submodule update --init --remote tcp-ip rsh`
   - Pulls latest from default branches
   - Downgrades failures to warnings (for offline builds)

2. **Build tcp-ip Binaries**
   - Checks if `tcp-ip/target/x86_64-unknown-rustos/release/{wifi,ping,ifconfig,netstat}` exist
   - If not, runs `cargo build --release` in tcp-ip directory
   - These are userspace ELF binaries for network management

3. **Generate net_bins.rs**
   - Creates `$OUT_DIR/net_bins.rs` with:
     ```rust
     pub static WIFI_ELF: &[u8] = include_bytes!("path/to/wifi");
     pub static PING_ELF: &[u8] = include_bytes!("path/to/ping");
     // etc.
     ```
   - These are embedded into the kernel and exposed as `/bin/wifi`, etc.

4. **Rerun Triggers**
   - `build.rs` change
   - Submodule source changes
   - `.git/modules/{tcp-ip,rsh}/HEAD` changes

### Custom Target (x86_64-rustos.json)

Key settings:
- `"linker-flavor": "ld.lld"` - Use LLD linker
- `"panic-strategy": "abort"` - No unwinding
- `"disable-redzone": true` - Required for interrupt handlers
- `"features": "-mmx,-sse,+soft-float"` - No FPU in kernel
- `"os": "rustos"` - Custom OS identifier

---

## Development Workflow

### Making Changes

1. **Create a Branch**
   ```bash
   git checkout -b feature/my-feature
   ```

2. **Make Changes**
   - Follow coding conventions (see below)
   - Write documentation comments
   - Add tests if applicable

3. **Test Locally**
   ```bash
   cargo fmt
   cargo clippy
   cargo test
   cargo run  # Manual testing in QEMU
   ```

4. **Commit and Push**
   ```bash
   git add .
   git commit -m "feat: add feature X"
   git push origin feature/my-feature
   ```

5. **CI Will Run**
   - cargo check
   - cargo fmt --check
   - cargo clippy
   - cargo test

### Git Commit Conventions

Use conventional commits:
- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation only
- `style:` - Formatting, whitespace
- `refactor:` - Code restructuring
- `test:` - Adding tests
- `chore:` - Build system, dependencies

---

## Key Subsystems

### 1. Memory Management

**Location**: `src/arch/x86_64/memory/`

**Components**:
- `BootInfoFrameAllocator` - Frame allocator from bootloader memory map
- `OffsetPageTable` - Page table mapper
- `DmaAllocator` - Physically contiguous allocations for DMA

**Usage**:
```rust
// Map a physical address to virtual
let phys_addr = PhysAddr::new(0x1000);
let virt_addr = VirtAddr::new(PHYS_MEM_OFFSET + phys_addr.as_u64());

// Allocate DMA-capable memory
let dma_addr = DMA_ALLOCATOR.lock().alloc(4096);
```

**Critical**: Always use `PHYS_MEM_OFFSET` for physical-to-virtual translation.

### 2. Interrupts and Exceptions

**Location**: `src/arch/x86_64/interrupts.rs`

**IDT Entries**:
- 0-31: CPU exceptions (divide by zero, page fault, etc.)
- 32-47: IRQ handlers (timer, keyboard, etc.)
- 0x80: Syscall interrupt

**Adding an Interrupt Handler**:
```rust
// In interrupts.rs
extern "x86-interrupt" fn my_handler(_stack_frame: InterruptStackFrame) {
    // Handle interrupt
    unsafe { PICS.lock().notify_end_of_interrupt(IRQ_NUMBER); }
}

// In init_idt()
idt[IRQ_NUMBER + 32].set_handler_fn(my_handler);
```

### 3. Virtual Filesystem (VFS)

**Location**: `src/vfs/`

**Architecture**:
```
VFS (global singleton)
├── Mount Table: Vec<MountPoint>
│   ├── MountPoint { path: "/", filesystem: Fat32Filesystem }
│   ├── MountPoint { path: "/usb", filesystem: Fat32Filesystem }
│   └── MountPoint { path: "/bin", filesystem: VirtualBinFilesystem }
└── Methods: open(), read(), write(), list_dir(), etc.
```

**Usage**:
```rust
// Mount a filesystem
VFS.lock().mount("/usb", Box::new(filesystem));

// Open a file
let file = VFS.lock().open("/usb/hello.txt")?;

// Read file
let mut buffer = [0u8; 512];
file.lock().read(&mut buffer)?;
```

**Virtual /bin**:
- Defined in `src/bin_commands.rs`
- Maps command names to kernel functions
- Exposed as executable files in `/bin/`

### 4. USB Stack

**Location**: `src/usb/`

**Architecture**:
```
USB_XHCI (global XhciController)
├── PCI Device (vendor 0x8086, class 0x0C03)
├── MMIO Registers (BAR0)
├── Command Ring (TRBs)
├── Event Ring (TRBs)
├── Ports: Vec<XhciPort>
│   └── Port → Device → Endpoints
└── Transfer Descriptors (for bulk transfers)
```

**Mass Storage**:
- Uses Bulk-Only Transport (BOT) protocol
- SCSI commands (READ_10, WRITE_10, INQUIRY)
- Implements `BlockDevice` trait for VFS

**USB Initialization**:
1. PCI enumeration finds XHCI controller
2. Enable Bus Master + Memory Space
3. Map BAR0 to virtual memory
4. Initialize command/event rings
5. Start controller
6. Enumerate ports
7. Configure devices
8. Initialize mass storage driver

### 5. Network Stack (tcp-ip)

**Location**: `src/net.rs` (kernel integration), `tcp-ip/` (submodule)

**Integration Points**:
```rust
// Initialize (called from main.rs)
pub fn init() {
    // Find Intel AX210 on PCI bus
    // Enable Bus Master + Memory Space
    // Map BAR0
    unsafe { tcp_ip::kernel::init(bar0_virt); }
}

// Check status
pub fn print_status() {
    if let Some(s) = tcp_ip::kernel::status_str() {
        println!("{}", s);
    }
}

// Syscall dispatch (called from syscall/mod.rs)
pub fn dispatch_syscall(nr: u64, a1: u64, a2: u64, a3: u64) -> Option<i64> {
    tcp_ip::kernel::dispatch_syscall(nr, a1, a2, a3)
}
```

**Syscalls** (300-310):
- 300: WIFI_SCAN
- 301: WIFI_CONNECT
- 302: WIFI_DISCONNECT
- 303: WIFI_STATUS
- 304: NET_IFCONFIG
- 305: NET_IFCONFIG_SET
- 306: NET_PING
- 307: NET_STAT
- 308: NET_DHCP
- 310: NET_ROUTES

**Userspace Tools**:
- `/bin/wifi` - Wi-Fi management
- `/bin/ping` - ICMP echo
- `/bin/ifconfig` - Interface config
- `/bin/netstat` - Connection status

### 6. Framebuffer Driver

**Location**: `src/drivers/framebuffer.rs`

**Font**: `assets/font8x16.bin` (8x16 pixel bitmap font, 95 ASCII chars)

**Architecture**:
```
FRAMEBUFFER_WRITER (global Option<FrameBufferWriter>)
├── framebuffer: &'static mut [u8]  (raw pixel buffer)
├── info: FrameBufferInfo (width, height, stride, format)
├── x_pos, y_pos (character grid position)
└── foreground, background (Color)
```

**Output Routing**:
```
println!() → vga::_print()
    ├── Try FRAMEBUFFER_WRITER (if Some) → pixels
    ├── Fall back to VGA text mode → 0xb8000
    └── Always mirror to serial → COM1
```

**Initialization**:
```rust
// Early in kernel_main(), before any println!
if let Some(framebuffer) = boot_info.framebuffer.take() {
    unsafe { rustos::drivers::framebuffer::init(framebuffer); }
}
```

**Critical**: Initialize framebuffer **before** enabling interrupts to ensure early boot messages are visible.

### 7. Process Execution

**Location**: `src/process/`

**ELF Loader**:
- Parses ELF headers
- Maps PT_LOAD segments into page tables
- Handles overlapping segments (checks if already mapped)
- Jumps to entry point

**Syscalls**:
- SYS_READ (0)
- SYS_WRITE (1)
- SYS_OPEN (2)
- SYS_CLOSE (3)
- SYS_EXEC (59)
- SYS_EXIT (60)
- SYS_GETCWD (79)
- SYS_CHDIR (80)
- SYS_GETDENTS64 (217)

**Userspace Runtime**:
- `crates/rustos-rt/` provides `_start`, syscall wrappers
- Minimal std replacement (no heap allocation)
- Custom target JSON (`x86_64-unknown-rustos.json`)

---

## Coding Conventions

### Rust Style

Follow standard Rust conventions:
- Use `cargo fmt` (automatic formatting)
- `cargo clippy` should pass with no warnings
- Use meaningful variable names
- Write doc comments for public items

### Naming

- **Modules**: `snake_case` (e.g., `mass_storage`)
- **Types**: `PascalCase` (e.g., `XhciController`)
- **Functions**: `snake_case` (e.g., `init_usb_storage`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `PHYS_MEM_OFFSET`)
- **Global Statics**: `SCREAMING_SNAKE_CASE` (e.g., `USB_XHCI`, `VFS`)

### Unsafe Code

Always document why unsafe is needed:
```rust
// SAFETY: BAR0 is a valid MMIO region mapped by the bootloader.
// The address is guaranteed to be within the physical memory map.
unsafe {
    tcp_ip::kernel::init(bar0_virt);
}
```

### Error Handling

- Use `Result` for fallible operations
- Use `?` operator for error propagation
- Print descriptive error messages
- Don't use `.unwrap()` in production code (except in test code or after explicit checks)

### Logging

Use macros:
- `println!()` - User-visible output (goes to screen)
- `serial_println!()` - Debug output (goes to serial only)

Example:
```rust
serial_println!("[debug] Starting USB initialization");
println!("Initializing USB subsystem...");
```

### Documentation

Write doc comments for:
- All public functions
- All public types
- Complex internal functions
- Non-obvious algorithms

```rust
/// Initialize the network stack during kernel boot.
///
/// # Requirements
/// - PCI subsystem must be initialized
/// - Virtual memory manager must be available
///
/// # Panics
/// Panics if no Intel AX210 device is found.
pub fn init() {
    // ...
}
```

---

## Common Tasks

### Adding a New Driver

1. Create new module in `src/drivers/`
2. Implement initialization function
3. Add to `src/lib.rs` module list
4. Call init function from `main.rs`
5. Add tests in `tests/`

Example skeleton:
```rust
// src/drivers/my_device.rs
use spin::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref MY_DEVICE: Mutex<Option<MyDevice>> = Mutex::new(None);
}

pub struct MyDevice {
    // device state
}

impl MyDevice {
    pub fn new(/* params */) -> Self {
        Self { /* init */ }
    }
    
    pub fn read(&mut self) -> Result<u8, &'static str> {
        // implementation
    }
}

pub fn init() {
    let device = MyDevice::new();
    *MY_DEVICE.lock() = Some(device);
}
```

### Adding a New Syscall

1. Add syscall number constant to `src/syscall/mod.rs`
2. Add handler function `sys_my_syscall()`
3. Add match arm in `dispatch()`
4. Update `rustos-rt` with wrapper function (if needed)

```rust
// src/syscall/mod.rs

const SYS_MY_SYSCALL: u64 = 999;

fn sys_my_syscall(arg1: u64, arg2: u64) -> i64 {
    // implementation
    0 // success
}

// In dispatch()
match nr {
    // ...
    999 => sys_my_syscall(a1, a2),
    // ...
}
```

### Adding a Shell Command

1. Add function to `src/shell/commands.rs`
2. Add match arm in `execute_command()`
3. Optionally add to `/bin/` in `src/bin_commands.rs`

```rust
// src/shell/commands.rs

pub fn cmd_mycommand(args: &[&str]) {
    println!("My command output");
}

// In execute_command()
match cmd {
    // ...
    "mycommand" => cmd_mycommand(args),
    // ...
}
```

### Updating Submodules

Submodules auto-update on build, but to manually update:

```bash
# Update to latest
git submodule update --remote tcp-ip rsh

# Or update specific submodule
cd tcp-ip
git pull origin main
cd ..
git add tcp-ip
git commit -m "chore: update tcp-ip submodule"
```

### Adding a Test

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
    test_main();
    loop {}
}

#[test_case]
fn test_my_feature() {
    serial_print!("test_my_feature... ");
    assert_eq!(2 + 2, 4);
    serial_println!("[ok]");
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    rustos::test_panic_handler(info)
}
```

---

## Testing and Debugging

### Unit Tests

Run with:
```bash
cargo test
```

Tests run in QEMU with special test harness. Test output goes to serial.

### Integration Tests

Located in `tests/` directory. Each file is a separate test binary.

### QEMU Debugging

```bash
# Run with GDB stub
qemu-system-x86_64 \
    -drive format=raw,file=target/.../bootimage-rustos.bin \
    -serial stdio \
    -s -S  # GDB stub on port 1234, wait for connection

# In another terminal
gdb target/x86_64-rustos/debug/rustos
(gdb) target remote :1234
(gdb) break kernel_main
(gdb) continue
```

### Serial Output

Serial output is critical for debugging. Add debug prints:
```rust
serial_println!("[debug] value = {:?}", value);
```

Serial output always works, even if framebuffer fails.

### Common Issues and Solutions

**Issue**: Screen is blank after boot
- **Cause**: Framebuffer not initialized or failed
- **Solution**: Check serial output for errors, verify bootloader provides framebuffer

**Issue**: Kernel panics immediately
- **Cause**: Often memory-related (page fault, heap allocation failure)
- **Solution**: Check stack trace in serial output, verify memory initialization order

**Issue**: USB drive not detected
- **Cause**: XHCI controller not found or initialization failed
- **Solution**: Check `lspci` command output, verify PCI enumeration

**Issue**: Build.rs fails with git submodule error
- **Cause**: Offline or git not available
- **Solution**: Manually run `git submodule update --init --remote`, build will warn but continue

---

## Critical Implementation Details

### 1. Interrupt Handling

**Critical**: When writing interrupt handlers:
- Use `#[unsafe(naked)]` for syscall handler (saves all registers manually)
- Use `extern "x86-interrupt"` for exception handlers (compiler handles registers)
- Always call `PICS.lock().notify_end_of_interrupt(irq)`
- Never hold locks for long in interrupt context
- Never perform async operations in interrupt handlers

Example:
```rust
extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Brief work here
    unsafe {
        PICS.lock().notify_end_of_interrupt(32); // IRQ 0 (timer)
    }
}
```

### 2. Memory Mapping for Device Drivers

**Critical**: When mapping device MMIO:
- Always map with `PHYS_MEM_OFFSET`
- Ensure sufficient size (check device docs)
- Mark as uncacheable if needed
- Enable Bus Master for DMA devices

```rust
// Example: Mapping PCI BAR0
let bar0_phys = pci_device.read_bar(0);
let bar0_virt = PHYS_MEM_OFFSET + bar0_phys;
let bar0_size = 0x30_0000; // 3 MB for AX210

// Map pages (implementation depends on your page mapper)
map_physical_range(bar0_phys, bar0_size, PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
```

### 3. Syscall ABI

**Critical**: The syscall interface uses specific registers:
- **Entry**: `int 0x80` instruction
- **Syscall Number**: `rax`
- **Arguments**: `rdi`, `rsi`, `rdx`, `r10` (NOT `rcx`!)
- **Return**: `rax`

The naked function `syscall_handler()` must:
1. Save all caller-saved registers
2. Call `dispatch(rax, rdi, rsi, rdx)`
3. Store result in saved `rax`
4. Restore all registers
5. Return with `iretq`

### 4. Framebuffer Initialization Timing

**Critical**: Initialize framebuffer **before** `rustos::init()`:
```rust
pub fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    x86_64::instructions::interrupts::disable();
    
    // 1. Serial (no dependencies)
    unsafe { rustos::drivers::serial::init(); }
    
    // 2. Framebuffer (before interrupts)
    if let Some(framebuffer) = boot_info.framebuffer.take() {
        unsafe { rustos::drivers::framebuffer::init(framebuffer); }
    }
    
    // 3. Now interrupts (GDT, IDT, PIC)
    rustos::init();
    
    // 4. Rest of initialization...
}
```

**Why**: If interrupts are enabled before framebuffer init, an interrupt could trigger a `println!` that tries to use an uninitialized framebuffer.

### 5. ELF Segment Overlap Handling

**Critical**: ELF segments can share pages. When mapping segments:
```rust
// Check if page is already mapped
if mapper.translate_page(page).is_ok() {
    // Already mapped by previous segment, skip
    continue;
}

// Otherwise, map the page
mapper.map_to(page, frame, flags, &mut allocator)?;
```

**Why**: Segments like `.data` and `.bss` might overlap at page boundaries. Trying to remap causes a panic.

### 6. Heap Allocation Size

**Critical**: The heap must be large enough for framebuffer shadow buffer:
```rust
// src/allocator.rs
pub const HEAP_SIZE: usize = 16 * 1024 * 1024; // 16 MiB minimum
```

**Why**: For 1920x1080x4 bytes/pixel, framebuffer needs ~8 MiB. Other allocations need space too.

### 7. PCI Device Initialization

**Critical**: Always enable Bus Master and Memory Space:
```rust
// Read PCI command register (offset 0x04)
let command = pci_read_config(bus, dev, func, 0x04);

// Set Bus Master (bit 2) and Memory Space (bit 1)
let new_command = command | 0x06;
pci_write_config(bus, dev, func, 0x04, new_command);
```

**Why**: Without Bus Master, DMA doesn't work. Without Memory Space, MMIO doesn't work.

### 8. Submodule Build Order

**Critical**: `build.rs` must update submodules **before** cargo tries to build dependencies:
- Update submodules first thing in `build.rs`
- Fail gracefully if offline (emit warning, use stale code)
- Don't nest cargo invocations (causes lockfile contention)

---

## Submodules

### tcp-ip Submodule

**Repository**: https://github.com/RustOS-Dev/tcp-ip  
**Location**: `tcp-ip/`  
**Purpose**: Network stack (Intel AX210 driver, TCP/IP, Wi-Fi)

**Integration**:
- Kernel calls `tcp_ip::kernel::init(bar0)` in `src/net.rs`
- Syscalls 300-310 forwarded to `tcp_ip::kernel::dispatch_syscall()`
- Userspace binaries built and embedded by `build.rs`

**Auto-update**: Runs `git submodule update --init --remote tcp-ip` on every build.

### rsh Submodule

**Repository**: https://github.com/RustOS-Dev/rsh  
**Location**: `rsh/`  
**Purpose**: Userspace shell

**Integration**:
- Shell ELF embedded into kernel (future)
- Currently uses built-in kernel shell
- Planned: Launch rsh as userspace process

**Auto-update**: Runs `git submodule update --init --remote rsh` on every build.

### Working with Submodules

```bash
# Initial clone
git clone --recurse-submodules https://github.com/RustOS-Dev/RustOS

# Update all submodules
git submodule update --init --recursive --remote

# Update specific submodule
git submodule update --remote tcp-ip

# Make changes in submodule
cd tcp-ip
git checkout -b feature/my-change
# make changes, commit, push
git push origin feature/my-change
# open PR in tcp-ip repo

# Update RustOS to use new tcp-ip commit
cd ..
git add tcp-ip
git commit -m "chore: update tcp-ip submodule"
```

---

## Troubleshooting

### Build Fails with "submodule not initialized"

```bash
git submodule update --init --recursive
```

### Build Fails with "llvm-tools-preview not found"

```bash
rustup component add llvm-tools-preview --toolchain nightly
```

### QEMU Won't Start

Check:
- Is QEMU installed? (`qemu-system-x86_64 --version`)
- Is OVMF installed? (`ls /usr/share/OVMF/` or `/usr/share/edk2/`)
- Is `run-qemu-uefi.sh` executable? (`chmod +x run-qemu-uefi.sh`)

### Tests Hang in QEMU

- Check serial output for panic messages
- Increase timeout in test runner
- Try running single test: `cargo test --test test_name`

### Changes Not Visible After Rebuild

- Clean build: `cargo clean && cargo build`
- Submodules might be stale: `git submodule update --remote`

### Serial Output Not Showing

- Check QEMU args include `-serial stdio`
- Serial port initialized? Check `rustos::drivers::serial::init()` is called
- Redirected to file? Check `-serial file:serial.log`

---

## Resources

### Documentation
- [Writing an OS in Rust](https://os.phil-opp.com/) - Foundation tutorial
- [OSDev Wiki](https://wiki.osdev.org/) - Comprehensive OS development reference
- [UEFI Specification](https://uefi.org/specifications) - UEFI boot and protocols
- [Intel 64 and IA-32 Architectures Software Developer Manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)

### Rust Resources
- [Rust Embedded Book](https://rust-embedded.github.io/book/)
- [Rust Nomicon](https://doc.rust-lang.org/nomicon/) - Unsafe Rust
- [bootloader_api docs](https://docs.rs/bootloader_api/)

### Hardware Specs
- [XHCI Specification](https://www.intel.com/content/www/us/en/io/universal-serial-bus/extensible-host-controler-interface-usb-xhci.html)
- [USB Mass Storage Class Specification](https://www.usb.org/document-library/mass-storage-class-specification-overview-14)
- [PCI Local Bus Specification](https://pcisig.com/specifications)

---

## Getting Help

1. **Check Serial Output**: 90% of issues are debuggable via serial logs
2. **Read Error Messages**: Rust error messages are detailed and helpful
3. **Check Git History**: See how similar features were implemented
4. **Consult OSDev Wiki**: Most common issues are documented there
5. **Review bootloader_api docs**: For boot-related issues

---

**Document Version**: 1.0  
**Last Updated**: 2026-05-04  
**Maintainer**: RustOS-Dev Team
