//! Network stack integration — Wi-Fi driver and TCP/IP.
//!
//! Wraps the tcp-ip crate, which provides:
//! - Intel AX210 Wi-Fi 6E driver (iwlmvm)
//! - 802.11 MAC layer (scan, connect, WEP/WPA/WPA2/WPA3)
//! - TCP/IP stack (ARP, IP, ICMP, UDP, TCP, DHCP)
//! - Management binaries (/bin/wifi, /bin/ping, /bin/ifconfig, /bin/netstat)

use alloc::{boxed::Box, format};
use spin::Mutex;

use tcp_ip::driver::DriverError;
use tcp_ip::driver::ax210::platform::{Ax210PlatformOps, DmaRegion};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WifiInitState {
    Inactive,
    Active,
    NoAx210,
    InvalidBar,
    DriverFailed(DriverError),
}

static WIFI_INIT_STATE: Mutex<WifiInitState> = Mutex::new(WifiInitState::Inactive);
static AX210_FIRMWARE_CACHE: Mutex<Option<&'static [u8]>> = Mutex::new(None);

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
            DriverError::FirmwareMissing => {
                "wlan0: AX210 firmware file is missing; install /lib/firmware/iwlwifi-ty-a0-gf-a0-72.ucode or /lib/firmware/iwlwifi-ty-a0-gf-a0-71.ucode"
            }
            DriverError::FirmwareFault => "wlan0: AX210 firmware setup failed",
            DriverError::FirmwareMissingIml => {
                "wlan0: AX210 firmware is missing the required initial-load memory (IML) blob"
            }
            DriverError::FirmwareMissingRuntimeSections => {
                "wlan0: AX210 firmware is missing the required runtime section layout"
            }
            DriverError::HardwareReadyTimeout => {
                "wlan0: AX210 timed out taking PCI/NIC ownership during initialization"
            }
            DriverError::MacClockTimeout => {
                "wlan0: AX210 timed out waiting for the MAC clock during initialization"
            }
            DriverError::FirmwareAliveTimeout => {
                "wlan0: AX210 firmware did not deliver an ALIVE notification on the boot RX ring"
            }
            DriverError::FirmwareAliveInvalid => {
                "wlan0: AX210 firmware delivered an ALIVE notification, but its runtime state was malformed or reported failure"
            }
            DriverError::HardwareFault => "wlan0: AX210 hardware reported a fault",
            DriverError::BufferFull => "wlan0: AX210 driver buffer is full",
            DriverError::InvalidState => "wlan0: AX210 driver entered an invalid state",
            DriverError::BufferTooSmall => "wlan0: AX210 driver buffer is too small",
        },
    }
}

fn ax210_dma_alloc(size: usize, align: usize) -> Result<DmaRegion, DriverError> {
    let (virt, phys) = crate::memory::dma_alloc(size, align);
    Ok(DmaRegion {
        virt,
        phys,
        len: size,
    })
}

fn ax210_load_firmware(primary: &str, fallback: &str) -> Result<&'static [u8], DriverError> {
    if let Some(bytes) = *AX210_FIRMWARE_CACHE.lock() {
        return Ok(bytes);
    }

    let mut guard = crate::vfs::VFS.lock();
    let vfs = guard.as_mut().ok_or(DriverError::FirmwareMissing)?;
    let primary_path = format!("/lib/firmware/{}", primary);
    let fallback_path = format!("/lib/firmware/{}", fallback);

    let bytes = match vfs.read_file(&primary_path) {
        Ok(bytes) => {
            crate::serial_println!("[net] loaded AX210 firmware from {}", primary_path);
            bytes
        }
        Err(primary_err) => match vfs.read_file(&fallback_path) {
            Ok(bytes) => {
                crate::serial_println!("[net] loaded AX210 firmware from {}", fallback_path);
                bytes
            }
            Err(fallback_err) => {
                if !vfs.exists("/lib/firmware") {
                    crate::serial_println!(
                        "[net] AX210 firmware directory /lib/firmware is missing on the root filesystem"
                    );
                }
                crate::serial_println!(
                    "[net] AX210 firmware not found (checked {}: {}, {}: {})",
                    primary_path,
                    primary_err,
                    fallback_path,
                    fallback_err
                );
                return Err(DriverError::FirmwareMissing);
            }
        },
    };

    let leaked = Box::leak(bytes.into_boxed_slice()) as &'static [u8];
    *AX210_FIRMWARE_CACHE.lock() = Some(leaked);
    Ok(leaked)
}

/// Initialize the network stack on demand from the shell.
///
/// Enumerates PCI devices, finds an Intel wireless controller compatible with
/// the AX210 path, enables Bus Master + Memory Space, maps BAR0, and hands
/// control to the tcp-ip crate.
pub fn init() -> Result<(), &'static str> {
    use core::sync::atomic::Ordering;

    if tcp_ip::kernel::is_active() {
        *WIFI_INIT_STATE.lock() = WifiInitState::Active;
        return Ok(());
    }

    crate::serial_println!("[net] manual WiFi initialization requested");

    unsafe {
        tcp_ip::kernel::register_ax210_platform(Ax210PlatformOps {
            dma_alloc: ax210_dma_alloc,
            load_firmware: ax210_load_firmware,
        });
    }

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
