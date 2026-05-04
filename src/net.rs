//! Network stack integration — Wi-Fi driver and TCP/IP.
//!
//! Wraps the tcp-ip crate, which provides:
//! - Intel AX210 Wi-Fi 6E driver (iwlmvm)
//! - 802.11 MAC layer (scan, connect, WEP/WPA/WPA2/WPA3)
//! - TCP/IP stack (ARP, IP, ICMP, UDP, TCP, DHCP)
//! - Management binaries (/bin/wifi, /bin/ping, /bin/ifconfig, /bin/netstat)

/// Initialize the network stack during kernel boot.
///
/// Must be called after:
/// 1. PCI subsystem is initialized
/// 2. Virtual memory manager can map MMIO regions
///
/// Enumerates PCI devices, finds Intel AX210 (vendor 0x8086, device 0x2725/0x51F0/0x54F0/0x7F70),
/// enables Bus Master + Memory Space, maps BAR0, and hands control to tcp-ip crate.
pub fn init() {
    crate::serial_println!("[net] initializing network stack");
    
    let devices = crate::pci::enumerate();
    
    // Look for Intel AX210 Wi-Fi 6E adapter
    // Device IDs: 0x2725 (Typhoon Peak 2), 0x51F0, 0x54F0, 0x7F70
    if let Some(dev) = crate::pci::find_ax210(&devices) {
        crate::serial_println!(
            "[net] found Intel AX210 at {:02x}:{:02x}.{:x} (PCI {:04x}:{:04x})",
            dev.bus, dev.dev, dev.func, dev.vendor_id, dev.device_id
        );
        
        // Enable Bus Master (bit 2) and Memory Space (bit 1) in PCI command register
        // Required for DMA to work
        crate::pci::enable_bus_master(dev.bus, dev.dev, dev.func);
        
        // Map BAR0 (at least 0x30_0000 bytes) into kernel virtual address space
        let bar0_virt = dev.mmio_base(0);
        
        if bar0_virt == 0 {
            crate::serial_println!("[net] AX210 BAR0 is zero — not assigned by firmware");
            return;
        }
        
        crate::serial_println!("[net] AX210 BAR0 mapped at 0x{:x}", bar0_virt);
        
        // Initialize the tcp-ip driver
        unsafe {
            tcp_ip::kernel::init(bar0_virt);
        }
        
        if tcp_ip::kernel::is_active() {
            crate::serial_println!("[net] AX210 driver initialized successfully");
        } else {
            crate::serial_println!("[net] AX210 driver initialization failed");
        }
    } else {
        crate::serial_println!("[net] no Intel AX210 adapter found");
    }
}

/// Print network status (called by 'net' shell command).
pub fn print_status() {
    if let Some(s) = tcp_ip::kernel::status_str() {
        crate::println!("{}", s);
    } else {
        crate::println!("wlan0: no AX210-family WiFi device detected");
    }
}

/// Dispatch a network-related syscall (300-310).
///
/// Returns `Some(result)` if the syscall was handled by the network stack,
/// `None` if the syscall number is not a network syscall.
pub fn dispatch_syscall(nr: u64, a1: u64, a2: u64, a3: u64) -> Option<i64> {
    tcp_ip::kernel::dispatch_syscall(nr, a1, a2, a3)
}
