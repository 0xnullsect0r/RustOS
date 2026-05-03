# RustOS

A minimal x86_64 operating system kernel written in Rust, featuring a userspace shell (`rsh`),
virtual filesystem, USB mass-storage support, tcp-ip stack integration, and a Rust userspace runtime.

Built on the foundation of [Philipp Oppermann's "Writing an OS in Rust"](https://os.phil-opp.com/)
tutorial series (through post-12), then extended with a modular architecture, VFS layer,
XHCI USB driver, FAT32 filesystem, ELF process loader, and a userspace shell environment.

## Features

- **Bare-metal x86_64 kernel** — no OS underneath; boots via BIOS (or USB)
- **VGA text output** with colour support and backspace handling
- **UART serial** for debugging output
- **GDT + IDT** — segmentation, interrupt/exception handlers
- **Memory paging** + **heap allocator** (fixed-size block allocator)
- **Async executor** — keyboard input handled via `async/await`
- **Userspace shell (`rsh`)** launched from `/bin/rsh` at boot
- **VFS with mount table** — FAT32 storage partition as root `/` + additional FAT32 mounts
- **PCI enumeration** — finds XHCI host controllers on the PCI bus
- **XHCI USB 3.x driver** — full controller init, port enumeration, control + bulk transfers
- **USB Mass Storage (BOT/SCSI)** — reads and writes sectors on USB flash drives
- **FAT32 driver** — short names, long file names (LFN), cluster-chain traversal, file create/overwrite
- **Hot-plug USB** — `usbscan` command detects newly connected drives and mounts them
- **TCP/IP + WiFi integration** — `third_party/tcp-ip` submodule ABI hooks, AX210 discovery, and network syscalls
- **ELF process loader** — maps PT_LOAD segments and jumps to userspace entry points
- **`int 0x80` syscall interface** — `SYS_READ`, `SYS_WRITE`, `SYS_EXIT`, `SYS_OPEN`, `SYS_CLOSE`
- **[rustos-rt](../../tree/rustos-rt)** — companion Rust userspace runtime crate (separate branch)

## Shell

RustOS boots directly into the `/bin/rsh` console environment. The prompt is:

```console
rsh:/>
```

The `/bin` directory is a virtual command directory, so `ls /bin` shows the
commands available to `rsh`.

Legacy kernel commands are exposed as `/bin/*` executables for `rsh` execution:
`/bin/help`, `/bin/echo`, `/bin/clear`, `/bin/uname`, `/bin/color`, `/bin/pwd`,
`/bin/ls`, `/bin/cd`, `/bin/mkdir`, `/bin/rm`, `/bin/cat`, `/bin/write`,
`/bin/cp`, `/bin/mv`, `/bin/meminfo`, `/bin/mount`, `/bin/exec`, `/bin/usbscan`,
`/bin/reboot`, `/bin/rsh`, `/bin/net`.

The `net` command reports the pinned tcp-ip submodule and detected WiFi device.
The tcp-ip userspace tools (`wifi`, `ping`, `ifconfig`, `netstat`) are provided
by `third_party/tcp-ip` and can be built as RustOS ELFs and copied to `/bin`.

## USB flash drive workflow

### Booting + using a data drive

```
# Boot RustOS from a USB stick.
# The boot drive's partition 2 is mounted as root filesystem (/).
# Partition 1 is mounted at /usb.

# Plug in a second USB drive with your files, then in the shell:
usbscan               # detects the new drive, mounts it at /usb1

ls /usb1              # browse the FAT32 volume
cat /usb1/readme.txt  # read a file
cp /usb1/hello /hello # copy to root filesystem
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
├── main.rs               # Kernel entry point, initializes hardware and built-in shell
├── lib.rs                # Crate root, init(), test infrastructure
├── arch/x86_64/
│   ├── gdt.rs            # Global Descriptor Table
│   ├── interrupts.rs     # IDT, exception + IRQ handlers
│   └── memory/           # Page table init, frame allocators, DMA alloc
├── drivers/
│   ├── framebuffer.rs    # UEFI GOP framebuffer text output
│   ├── vga.rs            # VGA text buffer fallback
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
└── syscall/              # int 0x80 dispatcher

crates/
└── rustos-rt/            # Userspace Rust runtime (also on branch rustos-rt)

third_party/
├── rsh/                  # Shell submodule source (userspace program)
└── tcp-ip/               # TCP/IP + WiFi stack submodule source
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
sudo dd if=target/x86_64-rustos/debug/bootimage-rustos.bin of=/dev/sdX bs=4M status=progress && sync
```

> **Note:** After writing, `lsblk` will show your USB drive with **no partitions** — this is
> outdated for the release installer script. `write_to_drive.sh` now creates a second FAT32
> storage partition that occupies the remaining disk space and is used as root (`/`) in RustOS.

## Releases

Push a version tag to trigger a GitHub Actions release:

```sh
git tag v0.1.0 && git push origin v0.1.0
```

GitHub Actions will:
1. Build with `cargo bootimage --release`
2. Rename the output to `rustos-<version>.img`
3. Publish `rustos-<version>.img` as a GitHub Release asset (raw x86_64 BIOS disk image)

### Writing the release image to a USB drive

Use the local installer script (builds locally, then writes to USB):

```sh
./write_to_drive.sh --drive /dev/sdX
```

or write a release image manually:

**Linux / macOS:**
```sh
sudo dd if=rustos-v0.1.0.img of=/dev/sdX bs=4M status=progress && sync
```
Replace `/dev/sdX` with your USB device (e.g. `/dev/sdb`). **Do NOT use a partition**
(e.g. `/dev/sdb1`) — write to the whole device.

After `write_to_drive.sh` finishes, `lsblk` will show:
- partition 1 (boot/EFI)
- partition 2 (`RUSTOS_ROOT`, FAT32 storage/root filesystem)

**Windows (Rufus):** Select the `.img` file and choose **"DD Image"** write mode.

### BIOS / CSM requirement

This kernel uses a legacy BIOS bootloader. To boot it on real hardware:

- In your UEFI firmware settings, enable **Legacy Boot / CSM** (Compatibility Support Module).
- Systems set to **UEFI-only** mode will not boot this image.

### Test the release image in QEMU (no USB drive needed)

```sh
qemu-system-x86_64 \
  -drive format=raw,file=rustos-v0.1.0.img \
  -serial stdio
```

## CI

Every push and pull request runs:
- `cargo check` — compilation check
- `cargo fmt --check` — formatting
- `cargo clippy` — lints
- `cargo test` — integration tests under QEMU

## Submodules

Clone/update with submodules so `rsh` and `tcp-ip` are available:

```sh
git submodule update --init --recursive
```
