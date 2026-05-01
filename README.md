# RustOS

A minimal x86_64 operating system kernel written in Rust, featuring an interactive shell and
an in-memory virtual filesystem.

Built on the foundation of [Philipp Oppermann's "Writing an OS in Rust"](https://os.phil-opp.com/)
tutorial series (through post-12), then extended with a modular architecture, VFS layer, and
a full interactive shell.

## Features

- **Bare-metal x86_64 kernel** — no OS underneath; boots via BIOS
- **VGA text output** with colour support and backspace handling
- **UART serial** for debugging output
- **GDT + IDT** — segmentation, interrupt/exception handlers
- **Memory paging** + **heap allocator** (fixed-size block allocator)
- **Async executor** — keyboard input handled via `async/await`
- **Interactive shell** with 16 built-in commands
- **In-memory RAM filesystem** — create, read, write, copy, move, delete files and directories

## Shell commands

| Command | Description |
|---------|-------------|
| `help` | List all commands |
| `echo <text>` | Print text to screen |
| `clear` | Clear the screen |
| `uname` | Show OS name and version |
| `color <fg> <bg>` | Change text colour (black/blue/green/cyan/red/magenta/yellow/white) |
| `pwd` | Print working directory |
| `ls [path]` | List directory contents |
| `cd <path>` | Change directory |
| `mkdir <path>` | Create directory |
| `rm <path>` | Remove file or empty directory |
| `cat <path>` | Print file contents |
| `write <path> <text>` | Write text to a file |
| `cp <src> <dst>` | Copy a file |
| `mv <src> <dst>` | Move / rename a file or directory |
| `meminfo` | Show heap usage statistics |
| `reboot` | Reboot the machine |

## Project structure

```
src/
├── main.rs               # Kernel entry point, spawns shell task
├── lib.rs                # Crate root, init(), test infrastructure
├── arch/x86_64/
│   ├── gdt.rs            # Global Descriptor Table
│   ├── interrupts.rs     # IDT, exception + IRQ handlers
│   └── memory/           # Page table init, frame allocators
├── drivers/
│   ├── vga.rs            # VGA text buffer driver
│   └── serial.rs         # UART 16550 serial driver
├── allocator/            # Heap allocator (fixed-size block)
├── task/                 # Async executor + keyboard stream
├── vfs/
│   ├── mod.rs            # VFS types (VfsError, DirEntry, NodeType)
│   └── ramfs.rs          # In-memory RAM filesystem
└── shell/
    ├── mod.rs            # Shell struct, line editor, command dispatch
    └── commands.rs       # All built-in command implementations
```

## Building and running

### Prerequisites

```sh
# Nightly Rust + required components
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
rustup component add llvm-tools-preview --toolchain nightly

# bootimage tool
cargo install bootimage

# QEMU (for running)
sudo apt install qemu-system-x86   # or equivalent for your distro
```

### Run in QEMU

```sh
cargo run
```

### Run tests

```sh
cargo test
```

### Build bootable binary

```sh
cargo bootimage
# Produces: target/x86_64-rustos/debug/bootimage-rustos.bin
```

## Releases

When a version tag (`v*`) is pushed, GitHub Actions automatically:
1. Builds the kernel with `cargo bootimage`
2. Wraps the binary in an El Torito BIOS-bootable ISO using `xorriso`
3. Uploads `rustos-<version>.iso` as a GitHub Release asset

## CI

Every push and pull request runs:
- `cargo check` — compilation check
- `cargo fmt --check` — formatting
- `cargo clippy` — lints
- `cargo test` — integration tests under QEMU
