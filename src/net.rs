//! Network stack integration — Wi-Fi driver and TCP/IP.
//!
//! Wraps the tcp-ip crate, which provides:
//! - Intel AX210 Wi-Fi 6E driver (iwlmvm)
//! - 802.11 MAC layer (scan, connect, WEP/WPA/WPA2/WPA3)
//! - TCP/IP stack (ARP, IP, ICMP, UDP, TCP, DHCP)
//! - Management binaries (/bin/wifi, /bin/ping, /bin/ifconfig, /bin/netstat)

use spin::Mutex;
use tcp_ip::driver::DriverError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WifiInitState {
    Inactive,
    Active,
    NoAx210,
    InvalidBar,
    DriverFailed(DriverError),
}

static WIFI_INIT_STATE: Mutex<WifiInitState> = Mutex::new(WifiInitState::Inactive);

fn init_state_message(state: WifiInitState) -> &'static str {
    match state {
        WifiInitState::Inactive => "wlan0: driver inactive (run 'wifi init')",
        WifiInitState::Active => "wlan0: driver active",
        WifiInitState::NoAx210 => {
            "wlan0: no AX210 adapter found (run 'lspci' to verify PCI detection)"
        }
        WifiInitState::InvalidBar => "wlan0: AX210 BAR0 is zero or invalid",
        WifiInitState::DriverFailed(err) => match err {
            DriverError::DeviceNotFound => "wlan0: AX210 device not found during driver init",
            DriverError::InvalidBar => "wlan0: AX210 BAR is invalid or unusable",
            DriverError::FirmwareFault => "wlan0: AX210 firmware setup failed",
            DriverError::Timeout => "wlan0: AX210 hardware timed out during initialization",
            DriverError::HardwareFault => "wlan0: AX210 hardware reported a fault",
            DriverError::BufferFull => "wlan0: AX210 driver buffer is full",
            DriverError::InvalidState => "wlan0: AX210 driver entered an invalid state",
            DriverError::BufferTooSmall => "wlan0: AX210 driver buffer is too small",
        },
    }
}

/// Initialize the network stack on demand from the shell.
///
/// Enumerates PCI devices, finds Intel AX210 (vendor 0x8086, device 0x2725/0x51F0/0x54F0/0x7F70),
/// enables Bus Master + Memory Space, maps BAR0, and hands control to tcp-ip crate.
pub fn init() -> Result<(), &'static str> {
    use core::sync::atomic::Ordering;

    if tcp_ip::kernel::is_active() {
        *WIFI_INIT_STATE.lock() = WifiInitState::Active;
        return Ok(());
    }

    crate::serial_println!("[net] manual WiFi initialization requested");

    let devices = crate::pci::enumerate();

    // Look for Intel AX210 Wi-Fi 6E adapter
    // Device IDs: 0x2725 (Typhoon Peak 2), 0x51F0, 0x54F0, 0x7F70
    let Some(dev) = crate::pci::find_ax210(&devices) else {
        *WIFI_INIT_STATE.lock() = WifiInitState::NoAx210;
        crate::serial_println!("[net] no Intel AX210 adapter found");
        return Err(init_state_message(WifiInitState::NoAx210));
    };

    crate::serial_println!(
        "[net] found Intel AX210 at {:02x}:{:02x}.{:x} (PCI {:04x}:{:04x})",
        dev.bus,
        dev.dev,
        dev.func,
        dev.vendor_id,
        dev.device_id
    );

    // Enable Bus Master (bit 2) and Memory Space (bit 1) in PCI command register
    // Required for DMA to work
    crate::pci::enable_bus_master(dev.bus, dev.dev, dev.func);

    // Get the physical address of BAR0
    let bar0_phys = dev.mmio_base(0);

    if bar0_phys == 0 {
        *WIFI_INIT_STATE.lock() = WifiInitState::InvalidBar;
        crate::serial_println!("[net] AX210 BAR0 is zero — not assigned by firmware");
        return Err(init_state_message(WifiInitState::InvalidBar));
    }

    // Map BAR0 physical address to kernel virtual address space
    let phys_offset = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let bar0_virt = phys_offset + bar0_phys;

    crate::serial_println!(
        "[net] AX210 BAR0 phys=0x{:x} virt=0x{:x}",
        bar0_phys,
        bar0_virt
    );

    // Initialize the tcp-ip driver
    if let Err(err) = unsafe { tcp_ip::kernel::init(bar0_virt) } {
        *WIFI_INIT_STATE.lock() = WifiInitState::DriverFailed(err);
        crate::serial_println!(
            "[net] AX210 driver initialization failed: {}",
            tcp_ip::kernel::driver_error_str(err)
        );
        return Err(init_state_message(WifiInitState::DriverFailed(err)));
    }

    *WIFI_INIT_STATE.lock() = WifiInitState::Active;
    crate::serial_println!("[net] AX210 driver initialized successfully");
    Ok(())
}

pub fn status_line() -> &'static str {
    if let Some(s) = tcp_ip::kernel::status_str() {
        s
    } else {
        init_state_message(*WIFI_INIT_STATE.lock())
    }
}

/// Print network status (called by 'net' shell command).
pub fn print_status() {
    crate::println!("{}", status_line());
}

/// Dispatch a network-related syscall (300-310).
///
/// Returns `Some(result)` if the syscall was handled by the network stack,
/// `None` if the syscall number is not a network syscall.
pub fn dispatch_syscall(nr: u64, a1: u64, a2: u64, a3: u64) -> Option<i64> {
    tcp_ip::kernel::dispatch_syscall(nr, a1, a2, a3)
}
