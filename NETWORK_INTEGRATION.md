# RustOS Network Stack Integration

## Overview

RustOS integrates the `tcp-ip` crate as a git submodule to provide:
- Intel AX210 Wi-Fi 6E driver (iwlmvm)
- 802.11 MAC layer (scan, connect, WEP/WPA/WPA2/WPA3)
- TCP/IP stack (ARP, IP, ICMP, UDP, TCP, DHCP)
- Userspace management tools (/bin/wifi, /bin/ping, /bin/ifconfig, /bin/netstat)

## Submodule Location

The `tcp-ip` submodule is located at the repository root:
```
RustOS/
├── tcp-ip/          ← tcp-ip submodule
├── rsh/             ← rsh submodule
├── src/
│   ├── net.rs       ← Kernel integration layer
│   ├── main.rs      ← Calls net::init() at boot
│   └── syscall/mod.rs ← Forwards syscalls 300-310
└── build.rs         ← Auto-updates submodule on build
```

## Automatic Submodule Updates

The `build.rs` script ensures the tcp-ip submodule is always at the latest version:

```rust
fn update_submodules(manifest_dir: &str) {
    let result = Command::new("git")
        .args([
            "submodule",
            "update",
            "--init",
            "--remote",
            "tcp-ip",
            "rsh",
        ])
        .current_dir(manifest_dir)
        .status();
    // ...
}
```

This runs automatically on every `cargo build`, ensuring you always have the latest network stack code.

## Kernel Integration API

The tcp-ip crate exports the following kernel-facing API:

### tcp_ip::kernel module

```rust
pub unsafe fn init(bar0: u64)
```
- Initialize the AX210 driver with the mapped BAR0 virtual address
- Must be called after PCI enumeration and BAR mapping
- Only call once during boot

```rust
pub fn is_active() -> bool
```
- Check if the driver initialized successfully
- Returns `true` if the device is ready

```rust
pub fn link_up() -> bool
```
- Check if Wi-Fi link is currently associated to an AP
- Returns `false` if disconnected or device not initialized

```rust
pub fn status_str() -> Option<&'static str>
```
- Get human-readable status string for display
- Returns `None` if device not initialized

```rust
pub fn dispatch_syscall(nr: u64, a1: u64, a2: u64, a3: u64) -> Option<i64>
```
- Handle network syscalls (300-310)
- Returns `Some(result)` if handled, `None` for unknown syscalls

## Initialization Sequence

The network stack initialization happens in `src/main.rs`:

```rust
fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    // 1. Basic kernel setup (GDT, IDT, heap, VFS)
    // ...
    
    // 2. Initialize network stack
    rustos::net::init();      // ← Discovers AX210 device
    init_usb_storage();       // ← Mount USB storage
    init_network();           // ← Calls tcp_ip::kernel::init()
    
    // 3. Enable interrupts and start shell
    // ...
}
```

### net::init() Implementation (src/net.rs)

```rust
pub fn init() {
    let devices = crate::pci::enumerate();
    
    if let Some(dev) = crate::pci::find_ax210(&devices) {
        // Enable Bus Master + Memory Space in PCI config
        crate::pci::enable_bus_master(dev.bus, dev.dev, dev.func);
        
        // Map BAR0 (needs at least 0x30_0000 bytes)
        let bar0_virt = dev.mmio_base(0);
        
        // Initialize the driver
        unsafe {
            tcp_ip::kernel::init(bar0_virt);
        }
        
        // Check status
        if tcp_ip::kernel::is_active() {
            // Driver is ready!
        }
    }
}
```

## Network Syscalls

The kernel forwards syscalls 300-310 to the tcp-ip stack:

| Number | Name | Description |
|--------|------|-------------|
| 300 | SYS_WIFI_SCAN | Scan for wireless networks |
| 301 | SYS_WIFI_CONNECT | Connect to a network (with PSK) |
| 302 | SYS_WIFI_DISCONNECT | Disconnect from current network |
| 303 | SYS_WIFI_STATUS | Query Wi-Fi connection status |
| 304 | SYS_NET_IFCONFIG | Query interface configuration |
| 305 | SYS_NET_IFCONFIG_SET | Set interface configuration |
| 306 | SYS_NET_PING | Send ICMP echo request |
| 307 | SYS_NET_STAT | Query TCP/UDP connection table |
| 308 | SYS_NET_DHCP | Request DHCP lease |
| 310 | SYS_NET_ROUTES | Query routing table |

### Syscall Dispatcher (src/syscall/mod.rs)

```rust
extern "C" fn dispatch(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    match nr {
        // ... other syscalls ...
        
        300..=310 => {
            tcp_ip::kernel::dispatch_syscall(nr, a1, a2, a3)
                .unwrap_or(-38) // -ENOSYS if not recognized
        }
        
        // ...
    }
}
```

## Shell Commands

The kernel shell provides built-in commands for network management:

### `net` command (src/shell/commands.rs)

```rust
fn cmd_net() {
    if let Some(s) = tcp_ip::kernel::status_str() {
        crate::println!("{}", s);
    } else {
        crate::println!("wlan0: driver not active (no AX210 found or init failed)");
    }
}
```

Shows the current network device status.

### `wifi` command

Built-in command that uses tcp-ip syscalls to scan, connect, and manage Wi-Fi.

### Userspace Tools

The tcp-ip submodule also builds standalone ELF binaries installed to `/bin/`:
- `/bin/wifi` - Full-featured Wi-Fi management
- `/bin/ping` - ICMP ping utility
- `/bin/ifconfig` - Network interface configuration
- `/bin/netstat` - TCP/UDP connection status

These are built by `build.rs` and embedded into the kernel image.

## Hardware Support

Currently supports Intel AX210-family Wi-Fi 6E adapters:

- **Vendor ID**: 0x8086 (Intel)
- **Device IDs**: 
  - 0x2725 (AX210 Typhoon Peak 2)
  - 0x51F0
  - 0x54F0
  - 0x7F70

The driver requires:
1. PCI Bus Master and Memory Space enabled (done by `enable_bus_master`)
2. BAR0 mapped into kernel virtual address space (at least 3 MiB)
3. DMA-capable memory allocator

## Requirements for tcp_ip::kernel::init()

Before calling `tcp_ip::kernel::init(bar0)`:

1. ✅ PCI subsystem must be initialized
2. ✅ Virtual memory mapper must be available
3. ✅ Heap allocator must be initialized
4. ✅ Device BAR0 must be mapped to virtual address space
5. ✅ Bus Master + Memory Space must be enabled in PCI config

The physical memory offset from bootloader must be stored in `PHYS_MEM_OFFSET` for BAR mapping.

## Troubleshooting

### Driver not initializing

Check serial output for:
```
[net] no Intel AX210 adapter found
```
→ No supported device detected on PCI bus

```
[net] AX210 BAR0 is zero — not assigned by firmware
```
→ BIOS/UEFI didn't assign resources to the device

```
[net] AX210 driver initialization failed
```
→ Firmware loading failed or device communication error

### Submodule not updating

If the build.rs submodule update fails, you'll see:
```
cargo:warning=git submodule update --remote exited with ...
```

Manually update with:
```bash
git submodule update --init --remote tcp-ip
```

## Development Workflow

### Updating to latest tcp-ip

The submodule updates automatically on every build. To manually update:

```bash
cd tcp-ip
git pull origin main
cd ..
git add tcp-ip
git commit -m "Update tcp-ip submodule to latest"
```

### Testing network features

1. Build and run RustOS in QEMU:
   ```bash
   cargo build --target x86_64-rustos.json
   ./run-qemu-uefi.sh
   ```

2. Check network status:
   ```
   > net
   ```

3. If AX210 is detected, test Wi-Fi:
   ```
   > wifi scan
   > wifi connect MyNetwork MyPassword
   > wifi status
   ```

## Architecture Notes

- The tcp-ip driver runs in the kernel context (no separate process)
- It uses DMA for packet I/O (requires Bus Master enabled)
- Firmware is embedded in the driver binary
- The stack is single-threaded (no concurrent access to NET_STACK)
- All network I/O is synchronous (no async/await)

## Future Enhancements

Potential improvements to the integration:

1. Support for additional Wi-Fi adapters (Intel AX211, Realtek chips)
2. Async network I/O with tokio/smol integration
3. Full IPv6 support
4. TLS/SSL for HTTPS
5. DNS client for hostname resolution
6. Multiple simultaneous TCP connections
7. UDP socket API for userspace
