# RustOS

A minimal x86_64 operating system kernel written in Rust, featuring an interactive shell,
virtual filesystem, USB mass-storage support, and a Rust userspace runtime.

Built on the foundation of [Philipp Oppermann's "Writing an OS in Rust"](https://os.phil-opp.com/)
tutorial series (through post-12), then extended with a modular architecture, VFS layer,
XHCI USB driver, FAT32 filesystem, ELF process loader, and a full interactive shell.

## Features

- **Bare-metal x86_64 kernel** — no OS underneath; boots via BIOS (or USB)
- **VGA text output** with colour support and backspace handling
- **UART serial** for debugging output
- **GDT + IDT** — segmentation, interrupt/exception handlers
- **Memory paging** + **heap allocator** (fixed-size block allocator)
- **Async executor** — keyboard input handled via `async/await`
- **Interactive shell** with 19 built-in commands
- **VFS with mount table** — RamFs (root `/`) + FAT32 mounts side-by-side
- **In-memory RAM filesystem** — create, read, write, copy, move, delete files and directories
- **PCI enumeration** — finds XHCI host controllers on the PCI bus
- **XHCI USB 3.x driver** — full controller init, port enumeration, control + bulk transfers
- **USB Mass Storage (BOT/SCSI)** — reads sectors from USB flash drives
- **FAT32 read-only driver** — short names, long file names (LFN), cluster-chain traversal
- **Hot-plug USB** — `usbscan` command detects newly connected drives and mounts them
- **ELF process loader** — maps PT_LOAD segments and jumps to userspace entry points
- **`int 0x80` syscall interface** — `SYS_READ`, `SYS_WRITE`, `SYS_EXIT`, `SYS_OPEN`, `SYS_CLOSE`
- **[rustos-rt](../../tree/rustos-rt)** — companion Rust userspace runtime crate (separate branch)

## Shell commands

| Command | Description |
|---------|-------------|
| `help` | List all commands |
| `echo <text>` | Print text to screen |
| `clear` | Clear the screen |
| `uname` | Show OS name and version |
| `color <fg> <bg>` | Change text colour (black/blue/green/cyan/red/magenta/yellow/white/…) |
| `pwd` | Print working directory |
| `ls [path]` | List directory contents |
| `cd <path>` | Change directory |
| `mkdir <path>` | Create directory |
| `rm <path>` | Remove file or empty directory |
| `cat <path>` | Print file contents |
| `write <path> <text>` | Write text to a file (overwrites) |
| `cp <src> <dst>` | Copy a file (works across filesystems, e.g. `/usb` → `/`) |
| `mv <src> <dst>` | Move / rename a file or directory |
| `meminfo` | Show heap usage statistics |
| `mount` | Show mounted filesystems |
| `exec <path>` | Execute an ELF binary loaded from the VFS |
| `usbscan` | Scan for newly plugged-in USB drives and mount them |
| `reboot` | Reboot the machine |

## USB flash drive workflow

### Booting + using a data drive

```
# Boot RustOS from a USB stick.
# The boot drive is automatically enumerated and mounted at /usb.

# Plug in a second USB drive with your files, then in the shell:
usbscan               # detects the new drive, mounts it at /usb1

ls /usb1              # browse the FAT32 volume
cat /usb1/readme.txt  # read a file
cp /usb1/hello /hello # copy to the in-memory filesystem
exec /hello           # run an ELF binary
```

### Running a Rust userspace program on RustOS

See the **[`rustos-rt` branch](../../tree/rustos-rt)** for the companion runtime crate that
provides `_start`, `sys_write`, `sys_read`, `sys_exit`, the custom target JSON
(`x86_64-unknown-rustos.json`), and a linker script.

```toml
# Cargo.toml of your program
[dependencies]
rustos-rt = { git = "https://github.com/0xnullsect0r/RustOS", branch = "rustos-rt" }
```

```bash
cargo +nightly build \
  --target path/to/x86_64-unknown-rustos.json \
  -Z build-std=core \
  --release
```

Copy the resulting ELF to a FAT32 drive, mount it in RustOS, and run with `exec`.

## QEMU with USB

To test USB support in QEMU, first create a FAT32 disk image:

```sh
dd if=/dev/zero of=disk.img bs=1M count=32
mkfs.fat -F 32 disk.img
# optionally: mount and copy files
```

Then run:

```sh
cargo run   # uses the run-args in Cargo.toml automatically
```

The `[package.metadata.bootimage] run-args` in `Cargo.toml` already include:
```
-device qemu-xhci,id=xhci
-drive if=none,id=usbdisk,file=disk.img,format=raw
-device usb-storage,bus=xhci.0,drive=usbdisk
```

## Project structure

```
src/
├── main.rs               # Kernel entry point, spawns shell task
├── lib.rs                # Crate root, init(), test infrastructure
├── arch/x86_64/
│   ├── gdt.rs            # Global Descriptor Table
│   ├── interrupts.rs     # IDT, exception + IRQ handlers
│   └── memory/           # Page table init, frame allocators, DMA alloc
├── drivers/
│   ├── vga.rs            # VGA text buffer driver
│   └── serial.rs         # UART 16550 serial driver
├── allocator/            # Heap allocator (fixed-size block)
├── task/                 # Async executor + keyboard stream
├── pci/                  # PCI bus enumeration
├── usb/
│   ├── mod.rs            # BlockDevice trait, USB_XHCI global, mount helpers
│   ├── xhci/             # XHCI host controller driver (TRBs, rings, commands)
│   └── mass_storage/     # USB MSC BOT helper re-exports
├── fs/
│   └── fat32/            # FAT32 read-only driver (BPB, clusters, LFN)
├── vfs/
│   ├── mod.rs            # Filesystem trait, mount table, VFS global
│   └── ramfs.rs          # In-memory RAM filesystem
├── process/              # ELF loader, process exec
├── syscall/              # int 0x80 dispatcher
└── shell/
    ├── mod.rs            # Shell struct, line editor, command dispatch
    └── commands.rs       # All built-in command implementations

crates/
└── rustos-rt/            # Userspace Rust runtime (also on branch rustos-rt)
```

## Building and running

### Prerequisites

```sh
rustup toolchain install nightly
rustup component add rust-src llvm-tools-preview --toolchain nightly
cargo install bootimage
sudo apt install qemu-system-x86   # or equivalent
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

Write to a USB stick (Linux):
```sh
sudo dd if=target/x86_64-rustos/debug/bootimage-rustos.bin of=/dev/sdX bs=4M status=progress
```

## Releases

Push a version tag to trigger a GitHub Actions release:

```sh
git tag v0.1.0 && git push origin v0.1.0
```

GitHub Actions will:
1. Build with `cargo bootimage --release`
2. Wrap the binary in an El Torito BIOS-bootable ISO with `xorriso`
3. Publish `rustos-<version>.iso` as a GitHub Release asset

## CI

Every push and pull request runs:
- `cargo check` — compilation check
- `cargo fmt --check` — formatting
- `cargo clippy` — lints
- `cargo test` — integration tests under QEMU
