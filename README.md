# RustOS

A modern x86_64 operating system kernel written in Rust, featuring Wi-Fi networking, USB mass storage, 
virtual filesystem, framebuffer graphics, and comprehensive hardware support.

**Key Highlights:**
- 🚀 **Modern Boot**: UEFI-capable bootloader with GOP framebuffer support
- 🌐 **Networking**: Intel AX210 Wi-Fi 6E driver with full TCP/IP stack (tcp-ip submodule)
- 💾 **Storage**: USB 3.0 XHCI driver with FAT32 filesystem support
- 🖥️ **Graphics**: Native UEFI framebuffer with software text rendering
- 🔧 **Development**: Comprehensive testing, CI/CD, and documentation

Built on the foundation of [Philipp Oppermann's "Writing an OS in Rust"](https://os.phil-opp.com/)
tutorial series, extended with modern drivers, networking, and a modular architecture.

## ✨ Features

### Core Kernel
- **Bare-metal x86_64 kernel** — No OS underneath; boots directly on hardware
- **UEFI + BIOS support** — Modern GOP framebuffer with legacy VGA fallback
- **Hardware interrupts** — GDT, IDT, exception handlers, and PIC/APIC support
- **Memory management** — 4-level paging, heap allocator (16 MiB), DMA allocator
- **Async executor** — Cooperative multitasking for I/O-bound tasks

### Storage & Filesystem
- **USB 3.0 XHCI driver** — Full controller init, port enumeration, bulk transfers
- **USB Mass Storage** — BOT/SCSI protocol for USB flash drives
- **FAT32 filesystem** — LFN support, cluster chains, read/write operations
- **Virtual filesystem** — Mount table, unified file interface
- **Hot-plug support** — `usbscan` command detects and mounts new devices

### Networking
- **Intel AX210 Wi-Fi 6E** — Native kernel driver via tcp-ip submodule
- **Full TCP/IP stack** — ARP, IP, ICMP, UDP, TCP, DHCP
- **802.11 management** — WEP/WPA/WPA2/WPA3 support
- **Network syscalls** — 300-310 for userspace network access
- **Userspace tools** — `/bin/wifi`, `/bin/ping`, `/bin/ifconfig`, `/bin/netstat`

### Display & I/O
- **UEFI GOP framebuffer** — Native graphics output with 8x16 bitmap font
- **VGA text mode** — Legacy fallback for BIOS systems
- **UART serial** — COM1 debug output
- **Keyboard input** — PS/2 with interrupt and polling modes

### Development
- **ELF process loader** — Load and execute userspace programs
- **Syscall interface** — `int 0x80` with 0-99 (file I/O), 100-199 (process), 300-310 (network)
- **Comprehensive tests** — Integration tests via QEMU
- **CI/CD pipeline** — Automated checks, formatting, linting, and testing
- **Extensive documentation** — Architecture, development, and AI assistant guides

## 🚀 Quick Start

### Prerequisites

```bash
# Install Rust nightly toolchain
rustup toolchain install nightly
rustup component add rust-src llvm-tools-preview --toolchain nightly
rustup default nightly

# Install bootimage tool
cargo install bootimage

# Install QEMU (for testing)
sudo apt install qemu-system-x86 ovmf  # Ubuntu/Debian
sudo dnf install qemu-system-x86       # Fedora
brew install qemu                      # macOS
```

### Build and Run

```bash
# Clone repository with submodules
git clone --recurse-submodules https://github.com/RustOS-Dev/RustOS.git
cd RustOS

# Build and run in QEMU
cargo run

# Or build bootable image
cargo bootimage
# Output: target/x86_64-rustos/debug/bootimage-rustos.bin
```

### Write to USB Drive

**Linux/macOS:**
```bash
# Using the installer script (recommended)
./write_to_drive.sh --drive /dev/sdX

# Optional: override the auto-detected AX210 firmware source during install
./write_to_drive.sh --drive /dev/sdX --ax210-firmware /path/to/linux-firmware

# Or manually with dd
sudo dd if=target/x86_64-rustos/debug/bootimage-rustos.bin of=/dev/sdX bs=4M status=progress
sync
```

**Windows:**
Use [Rufus](https://rufus.ie/) in DD Image mode.

**⚠️ IMPORTANT:** Enable **Legacy Boot / CSM** in your UEFI settings to boot on real hardware.

## 📚 Documentation

### Phase 6 - Comprehensive Reference (NEW)
- **[SHELL_COMMANDS.md](SHELL_COMMANDS.md)** — Complete shell command reference with flags, examples, and limitations
- **[SYSCALLS.md](SYSCALLS.md)** — Full syscall documentation (file I/O, process, network syscalls 0-310)
- **[LIMITATIONS.md](LIMITATIONS.md)** — Known limitations, workarounds, and development roadmap
- **[TROUBLESHOOTING.md](TROUBLESHOOTING.md)** — Common issues, solutions, and error messages

### Architecture and Development
- **[COPILOT_INSTRUCTIONS.md](COPILOT_INSTRUCTIONS.md)** — Comprehensive guide for AI coding assistants
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — System design and architecture overview
- **[DEVELOPMENT.md](DEVELOPMENT.md)** — Developer guide for contributing
- **[NETWORK_INTEGRATION.md](NETWORK_INTEGRATION.md)** — TCP/IP stack integration details
- **[FRAMEBUFFER_IMPLEMENTATION.md](FRAMEBUFFER_IMPLEMENTATION.md)** — GOP framebuffer driver implementation

## 💻 Using RustOS

### Shell Interface

RustOS boots into a built-in kernel shell with the prompt:
```
rsh:/>
```

For a complete command reference with flags, options, and examples, see **[SHELL_COMMANDS.md](SHELL_COMMANDS.md)**.

### Available Commands

**File System:**
- `ls [path]` — List directory contents
- `cd <path>` — Change directory
- `pwd` — Print working directory
- `cat <file>` — Display file contents
- `mkdir <dir>` — Create directory
- `rm <path>` — Remove file/directory
- `cp <src> <dst>` — Copy file
- `mv <src> <dst>` — Move/rename file
- `write <file> <text>` — Write text to file
- `grep [flags] <pattern> <file>` — Search file contents

**System:**
- `help` — Show command list
- `uname` — Show system information
- `meminfo` — Display memory usage
- `clear` — Clear screen
- `reboot` — Reboot system
- `exec <path>` — Execute ELF binary
- `color <fg> <bg>` — Set terminal colors

**Hardware:**
- `lspci` — List PCI devices
- `lsusb` — List USB devices  
- `lsblk` — List block devices
- `ps` — List processes
- `usbscan` — Detect and mount new USB drives
- `mount [device path]` — Show/mount filesystems
- `umount <path>` — Unmount filesystem

**Network:**
- `net` — Show network status
- `wifi [scan|status|connect <ssid>]` — WiFi control
- `ping <host>` — Test network connectivity
- `ifconfig` — Show network interface config
- `netstat` — Show active connections
- `wifi connect <ssid> <password>` — Connect to network
- `wifi status` — Show connection status
- `ping <host>` — Send ICMP echo request
- `ifconfig` — Display network interfaces
- `netstat` — Show network connections

### USB Storage Workflow

```bash
# 1. Boot RustOS from USB (partition 1)
#    Partition 2 auto-mounts as root (/)

# 2. Insert second USB drive with your files
usbscan              # Detect and mount at /usb1

# 3. Use files
ls /usb1
cat /usb1/readme.txt
cp /usb1/program.elf /program.elf
exec /program.elf    # Run ELF binary
```

### Running Userspace Programs

See the [rustos-rt branch](../../tree/rustos-rt) for the companion runtime library.

Example Rust program:
```toml
# Cargo.toml
[dependencies]
rustos-rt = { git = "https://github.com/RustOS-Dev/RustOS", branch = "rustos-rt" }
```

Build:
```bash
cargo +nightly build \
  --target path/to/x86_64-unknown-rustos.json \
  -Z build-std=core \
  --release
```

Copy the ELF to a USB drive, mount in RustOS, and run with `exec`.

## 🧪 Testing and Quality Assurance

### Phase Completion Status

| Phase | Status | Coverage | Focus |
|-------|--------|----------|-------|
| **Phase 1** | ✅ Complete | Boot, Memory, Interrupts | Core kernel infrastructure |
| **Phase 2** | ✅ Complete | PCI, XHCI USB, Mass Storage | Hardware support and drivers |
| **Phase 3** | ✅ Complete | FAT32, VFS, RamFS | Filesystem abstraction layer |
| **Phase 4** | ✅ Complete | Shell, Commands, Syscalls | User interface and system calls |
| **Phase 5** | ✅ Complete | Stability, Edge Cases, Audit | Production readiness improvements |
| **Phase 6** | ✅ Complete | Documentation, Integration Tests | Comprehensive reference and test suite |

### Test Coverage

- **Unit Tests:** 50+ unit tests in QEMU
- **Integration Tests:** 
  - Storage operations (file I/O, FAT32, VFS)
  - Network syscalls (socket, bind, connect, etc.)
  - Shell command execution and argument handling
- **Hardware Tests:** Real hardware validation on x86_64 systems
- **Regression Tests:** CI/CD pipeline with automated testing

For detailed test results, see:
- **[PHASE5_TEST_REPORT.md](PHASE5_TEST_REPORT.md)** — Phase 5 stability testing results
- **[PHASE5_STABILITY_TESTS.md](PHASE5_STABILITY_TESTS.md)** — Phase 5 test plan

### Known Limitations

RustOS has some intentional limitations for Phase 6:

- No pipe support (`|`) — Use temporary files instead
- No output redirection (`>`) — Use `write` command
- No command substitution — Use separate commands
- FAT32 limited to 8.3 DOS filenames — Use RamFS for longer names
- No hot-plug USB support — Insert devices before boot
- Intel AX210 only for WiFi — Check hardware compatibility
- No IPv6 support — IPv4 required
- 16 MiB heap limit — Adequate for most operations

For a comprehensive list of limitations and workarounds, see **[LIMITATIONS.md](LIMITATIONS.md)**.

### System Components

```
┌─────────────────────────────────────────────┐
│              Applications                    │
│         (Userspace ELF programs)            │
└──────────────────┬──────────────────────────┘
                   │ int 0x80 syscalls
┌──────────────────┴──────────────────────────┐
│                Kernel Core                   │
│  ┌──────────┐ ┌──────────┐ ┌─────────────┐ │
│  │  Memory  │ │   Task   │ │ Interrupts  │ │
│  │   Mgmt   │ │ Executor │ │  (IDT/PIC)  │ │
│  └──────────┘ └──────────┘ └─────────────┘ │
│  ┌───────────────────────────────────────┐  │
│  │      Virtual Filesystem (VFS)         │  │
│  │ ┌────────┐ ┌────────┐ ┌───────────┐  │  │
│  │ │ FAT32  │ │ RAMFS  │ │ /bin VFS  │  │  │
│  │ └────────┘ └────────┘ └───────────┘  │  │
│  └───────────────────────────────────────┘  │
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────┴──────────────────────────┐
│              Device Drivers                  │
│  ┌────────────┐ ┌────────────┐ ┌──────────┐│
│  │Framebuffer │ │  USB XHCI  │ │ Network  ││
│  │    (GOP)   │ │  + MSC BOT │ │  (AX210) ││
│  └────────────┘ └────────────┘ └──────────┘│
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────┴──────────────────────────┐
│              Hardware Layer                  │
│   x86_64 CPU │ RAM │ PCI │ USB │ WiFi │    │
└─────────────────────────────────────────────┘
```

### Boot Sequence

```
UEFI Firmware
    ↓
Bootloader (bootloader 0.11)
    ├── Initialize GOP framebuffer
    ├── Setup 4-level paging
    ├── Load kernel ELF
    └── Jump to kernel_main()
        ↓
Kernel Initialization
    ├── Serial port init
    ├── Framebuffer init
    ├── GDT/IDT setup
    ├── Heap allocator
    ├── VFS initialization
    ├── PCI enumeration
    ├── USB stack init
    ├── Network stack init
    └── Launch shell
```

For detailed architecture information, see [ARCHITECTURE.md](ARCHITECTURE.md).

## 📂 Project Structure

```
RustOS/
├── src/
│   ├── main.rs              # Kernel entry point
│   ├── lib.rs               # Crate root
│   ├── arch/x86_64/         # Architecture-specific code
│   ├── drivers/             # Device drivers (framebuffer, VGA, serial)
│   ├── fs/fat32/            # FAT32 filesystem
│   ├── usb/                 # USB XHCI + mass storage
│   ├── vfs/                 # Virtual filesystem
│   ├── net.rs               # Network integration (tcp-ip)
│   ├── process/             # ELF loader
│   ├── syscall/             # System call dispatcher
│   └── shell/               # Built-in kernel shell
├── tcp-ip/                  # Network stack submodule
├── rsh/                     # Shell submodule  
├── assets/                  # Fonts and resources
├── tests/                   # Integration tests
├── build.rs                 # Build script
├── Cargo.toml               # Dependencies
└── x86_64-rustos.json       # Custom target specification
```

## 🧪 Testing

### Running Tests

```bash
# All integration tests
cargo test

# Specific test
cargo test --test test_name

# With output
cargo test -- --nocapture
```

### Test Coverage

Tests include:
- Memory allocation and heap
- VFS and filesystem operations
- USB device detection
- System call interface
- Boot process validation

Tests run automatically in QEMU via custom test harness.

## 🔧 Development

### Build Commands

```bash
cargo check          # Quick compilation check
cargo fmt            # Format code
cargo clippy         # Lint code
cargo build          # Debug build
cargo build --release # Optimized build
cargo bootimage      # Create bootable image
cargo run            # Build and run in QEMU
```

### Creating Test Disk Images

```bash
# Create FAT32 test disk
dd if=/dev/zero of=disk.img bs=1M count=32
mkfs.fat -F 32 disk.img

# Mount and add files
mkdir -p /tmp/test_mount
sudo mount -o loop disk.img /tmp/test_mount
sudo cp files/* /tmp/test_mount/
sudo umount /tmp/test_mount
```

### Debugging

**Serial Output** (always available):
```rust
serial_println!("[debug] Value: {:?}", value);
```

**GDB Debugging**:
```bash
# Terminal 1: QEMU with GDB stub
qemu-system-x86_64 ... -s -S

# Terminal 2: GDB
gdb target/x86_64-rustos/debug/rustos
(gdb) target remote :1234
(gdb) break kernel_main
(gdb) continue
```

See [DEVELOPMENT.md](DEVELOPMENT.md) for comprehensive development guide.

## 🔄 CI/CD

GitHub Actions automatically runs on every push:
- ✅ `cargo check` — Compilation check
- ✅ `cargo fmt --check` — Code formatting
- ✅ `cargo clippy` — Linting
- ✅ `cargo test` — Integration tests in QEMU

## 🌐 Submodules

RustOS uses git submodules for modular components:

### tcp-ip (Network Stack)
- **Repository**: [RustOS-Dev/tcp-ip](https://github.com/RustOS-Dev/tcp-ip)
- **Purpose**: Intel AX210 Wi-Fi driver, TCP/IP stack
- **Auto-update**: `build.rs` pulls latest on every build

### rsh (Shell)
- **Repository**: [RustOS-Dev/rsh](https://github.com/RustOS-Dev/rsh)  
- **Purpose**: Userspace shell (future integration)
- **Auto-update**: `build.rs` pulls latest on every build

**Manual Update**:
```bash
git submodule update --init --recursive --remote
```

## 📦 Releases

### Creating a Release

Push a version tag to trigger automated release build:

```bash
git tag v0.2.0
git push origin v0.2.0
```

GitHub Actions will:
1. Build with `cargo bootimage --release`
2. Create `rustos-<version>.img`
3. Publish as GitHub Release with download link

### Installing from Release

**Method 1: Installer Script**
```bash
./write_to_drive.sh --drive /dev/sdX
./write_to_drive.sh --drive /dev/sdX --ax210-firmware /path/to/linux-firmware
```

Creates:
- Partition 1: Boot/EFI partition
- Partition 2: FAT32 root filesystem (RUSTOS_ROOT)
- `/lib/firmware/` on `RUSTOS_ROOT` for optional AX210 firmware blobs

To enable AX210 firmware loading on real hardware without committing Intel's
proprietary blob to this repository, place one of these files on partition 2:

- `/lib/firmware/iwlwifi-ty-a0-gf-a0-72.ucode`
- `/lib/firmware/iwlwifi-ty-a0-gf-a0-71.ucode`

`write_to_drive.sh` always creates `/lib/firmware/` and first tries to
auto-detect the AX210 firmware from standard host locations such as
`/lib/firmware` and `/usr/lib/firmware`. `--ax210-firmware` still accepts either
a single firmware file or a directory containing the expected filename(s) when
you want to override that detection. If you flash an image manually with `dd`
or Rufus, mount the `RUSTOS_ROOT` partition afterwards and copy the firmware
into that directory.

**Method 2: Manual (Linux/macOS)**
```bash
sudo dd if=rustos-v0.2.0.img of=/dev/sdX bs=4M status=progress
sync
```

**Method 3: Windows (Rufus)**
1. Download `.img` file
2. Open [Rufus](https://rufus.ie/)
3. Select "DD Image" mode
4. Flash to USB

### UEFI Settings

RustOS uses a **legacy BIOS bootloader**. To boot on modern systems:

1. Enter UEFI firmware settings (usually F2, F10, or Del during boot)
2. Enable **Legacy Boot** or **CSM** (Compatibility Support Module)
3. Disable **Secure Boot** if needed
4. Save and reboot

💡 **Tip**: Some systems call this "Other OS" or "BIOS/UEFI Boot Mode"

## 🤝 Contributing

We welcome contributions! Please see [DEVELOPMENT.md](DEVELOPMENT.md) for:
- Setting up your development environment
- Code style guidelines
- Testing procedures
- Pull request process

### Quick Contribution Guide

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes
4. Run tests: `cargo test`
5. Format code: `cargo fmt`
6. Check lints: `cargo clippy`
7. Commit: `git commit -m "feat: add amazing feature"`
8. Push: `git push origin feature/amazing-feature`
9. Open a Pull Request

Use [conventional commits](https://www.conventionalcommits.org/):
- `feat:` — New feature
- `fix:` — Bug fix
- `docs:` — Documentation
- `refactor:` — Code refactoring
- `test:` — Adding tests
- `chore:` — Build system, dependencies

## 📖 Additional Resources

- **[OSDev Wiki](https://wiki.osdev.org/)** — OS development reference
- **[Writing an OS in Rust](https://os.phil-opp.com/)** — Original tutorial series
- **[Rust Embedded Book](https://rust-embedded.github.io/book/)** — Rust for bare-metal
- **[Intel SDM](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)** — x86_64 architecture reference
- **[UEFI Specification](https://uefi.org/specifications)** — UEFI boot protocol
- **[USB Specification](https://www.usb.org/documents)** — USB and XHCI docs

## 🐛 Troubleshooting

### Common Issues

**Blank screen after boot**
- Check serial output for errors
- Verify framebuffer initialization succeeded
- Try booting in legacy BIOS/CSM mode

**USB device not detected**
- Run `lspci` command to verify XHCI controller found
- Try different USB port
- Check serial output for USB initialization errors

**Build fails**
- Ensure nightly Rust is installed: `rustup default nightly`
- Install components: `rustup component add rust-src llvm-tools-preview`
- Clean and rebuild: `cargo clean && cargo build`

**Submodule errors**
- Update submodules: `git submodule update --init --recursive --remote`
- Check network connection

For more troubleshooting, see [DEVELOPMENT.md](DEVELOPMENT.md#troubleshooting).

## 📄 License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- **[Philipp Oppermann](https://github.com/phil-opp)** — For the excellent "Writing an OS in Rust" tutorial series
- **Rust Community** — For creating an amazing systems programming language
- **Contributors** — Everyone who has contributed code, documentation, or ideas

---

**Made with ❤️ and Rust** | [GitHub](https://github.com/RustOS-Dev/RustOS) | [Issues](https://github.com/RustOS-Dev/RustOS/issues) | [Discussions](https://github.com/RustOS-Dev/RustOS/discussions)
