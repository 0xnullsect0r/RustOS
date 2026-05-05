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
static AX210_FIRMWARE_CACHE: Mutex<Option<&'static [u8]>> = Mutex::new(None);

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

fn ax210_phys_to_virt(phys: u64) -> u64 {
    use core::sync::atomic::Ordering;

    crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) + phys
}

fn register_ax210_platform_ops() {
    unsafe {
        tcp_ip::kernel::register_ax210_platform(Ax210PlatformOps {
            dma_alloc: ax210_dma_alloc,
            load_firmware: ax210_load_firmware,
            phys_to_virt: ax210_phys_to_virt,
        });
    }
}

/// Initialize the network stack on demand from the shell.
///
/// Registers RustOS host callbacks and lets `tcp-ip` own the AX210 manual-init flow.
pub fn init() -> Result<(), &'static str> {
    register_ax210_platform_ops();
    tcp_ip::kernel::manual_init()
}

pub fn status_line() -> &'static str {
    tcp_ip::kernel::status_line()
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
