pub mod mass_storage;
pub mod xhci;

use alloc::boxed::Box;
use spin::Mutex;

// ---------------------------------------------------------------------------
// BlockDevice trait
// ---------------------------------------------------------------------------

/// A block device that can read 512-byte sectors.
pub trait BlockDevice: Send {
    fn read_sectors(&mut self, lba: u64, count: u16) -> Option<alloc::vec::Vec<u8>>;
    fn sector_count(&self) -> u64;
}

// ---------------------------------------------------------------------------
// Global XHCI controller
// ---------------------------------------------------------------------------

/// The global XHCI host controller.  Initialised once in `kernel_main` and
/// kept alive for the lifetime of the kernel so that hot-plug rescans and
/// ongoing FAT32 I/O can reach it.
pub static USB_XHCI: Mutex<Option<xhci::Xhci>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Index-based block device wrapper
// ---------------------------------------------------------------------------

/// A `BlockDevice` backed by a specific device index in `USB_XHCI`.
///
/// Storing only the index means many `XhciBlockDevice`s can coexist and each
/// transparently delegates to the shared, global XHCI controller.
pub struct XhciBlockDevice {
    pub dev_idx: usize,
}

impl BlockDevice for XhciBlockDevice {
    fn read_sectors(&mut self, lba: u64, count: u16) -> Option<alloc::vec::Vec<u8>> {
        USB_XHCI
            .lock()
            .as_mut()?
            .read_sectors_dev(self.dev_idx, lba, count)
    }

    fn sector_count(&self) -> u64 {
        USB_XHCI
            .lock()
            .as_ref()
            .and_then(|x| x.devices.get(self.dev_idx))
            .map(|d| d.block_count)
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Mount helpers (called from kernel_main and from the `usbscan` shell command)
// ---------------------------------------------------------------------------

/// Try to mount USB devices starting from `from_idx` as FAT32 volumes.
///
/// Device 0 → `/usb`, device 1 → `/usb1`, device 2 → `/usb2`, …
///
/// Safe to call while `USB_XHCI` is **not** locked; the function locks it
/// briefly only to read the device count, then releases it before doing any
/// FAT32 I/O (which re-acquires the lock per-sector).
pub fn mount_storage_devices(from_idx: usize) {
    let device_count = USB_XHCI
        .lock()
        .as_ref()
        .map(|x| x.devices.len())
        .unwrap_or(0);

    for dev_idx in from_idx..device_count {
        let block_dev = Box::new(XhciBlockDevice { dev_idx });

        match crate::fs::fat32::Fat32Fs::new(block_dev) {
            Some(fat32) => {
                let mount_path = if dev_idx == 0 {
                    alloc::string::String::from("/usb")
                } else {
                    alloc::format!("/usb{}", dev_idx)
                };
                let mut vfs = crate::vfs::VFS.lock();
                if let Some(vfs) = vfs.as_mut() {
                    vfs.mount(&mount_path, Box::new(crate::vfs::Fat32Mount(fat32)));
                    crate::println!("[usb] FAT32 volume mounted at {}", mount_path);
                }
            }
            None => {
                crate::serial_println!("[usb] device {} has no FAT32 volume", dev_idx);
            }
        }
    }
}

/// Scan all XHCI ports for newly connected devices, enumerate them, and
/// mount any FAT32 volumes found.
///
/// Returns the number of new storage devices that were successfully mounted.
/// Designed to be called from the `usbscan` shell command.
pub fn scan_and_mount() -> usize {
    // Phase 1: scan ports inside the XHCI lock, collect new device count.
    let before = USB_XHCI
        .lock()
        .as_ref()
        .map(|x| x.devices.len())
        .unwrap_or(0);
    {
        let mut xhci = USB_XHCI.lock();
        if let Some(x) = xhci.as_mut() {
            x.scan_new_ports();
        }
    } // USB_XHCI lock released here

    // Phase 2: mount any newly discovered devices (acquires USB_XHCI briefly
    // per-sector, never while VFS is held).
    let after = USB_XHCI
        .lock()
        .as_ref()
        .map(|x| x.devices.len())
        .unwrap_or(0);
    if after > before {
        mount_storage_devices(before);
    }
    after - before
}
