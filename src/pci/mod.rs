//! PCI bus enumeration using Configuration Mechanism 1 (CF8/CFC port I/O).

use alloc::vec::Vec;
use x86_64::instructions::port::Port;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

// ---------------------------------------------------------------------------
// PCI Device descriptor
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PciDevice {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    pub bars: [u32; 6],
}

impl PciDevice {
    /// Read one of the six Base Address Registers as a 64-bit value.
    /// Returns `(base_address, is_64bit, is_prefetchable)`.
    pub fn bar64(&self, idx: usize) -> (u64, bool, bool) {
        let bar = self.bars[idx];
        let is_mmio = (bar & 1) == 0;
        let is_64bit = is_mmio && ((bar >> 1) & 0x3) == 2;
        let is_prefet = is_mmio && (bar & 0x8) != 0;
        if is_64bit && idx + 1 < 6 {
            let hi = self.bars[idx + 1] as u64;
            let lo = (bar & !0xF) as u64;
            (lo | (hi << 32), true, is_prefet)
        } else {
            ((bar & !0xF) as u64, false, is_prefet)
        }
    }

    /// Return the MMIO base address of BAR `idx` (ignoring type/size bits).
    pub fn mmio_base(&self, idx: usize) -> u64 {
        self.bar64(idx).0
    }
}

// ---------------------------------------------------------------------------
// Config-space access
// ---------------------------------------------------------------------------

fn config_address(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    0x8000_0000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset & 0xFC) as u32)
}

fn read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr = config_address(bus, dev, func, offset);
    unsafe {
        let mut addr_port: Port<u32> = Port::new(CONFIG_ADDRESS);
        let mut data_port: Port<u32> = Port::new(CONFIG_DATA);
        addr_port.write(addr);
        data_port.read()
    }
}

fn read16(bus: u8, dev: u8, func: u8, offset: u8) -> u16 {
    let val = read32(bus, dev, func, offset & 0xFC);
    let shift = (offset & 2) * 8;
    (val >> shift) as u16
}

fn read8(bus: u8, dev: u8, func: u8, offset: u8) -> u8 {
    let val = read32(bus, dev, func, offset & 0xFC);
    let shift = (offset & 3) * 8;
    (val >> shift) as u8
}

// ---------------------------------------------------------------------------
// Enumeration
// ---------------------------------------------------------------------------

/// Scan all PCI buses (0–255) and return every device found.
pub fn enumerate() -> Vec<PciDevice> {
    let mut devices = Vec::new();
    for bus in 0u8..=255 {
        for dev in 0u8..32 {
            scan_device(bus, dev, &mut devices);
        }
    }
    devices
}

fn scan_device(bus: u8, dev: u8, out: &mut Vec<PciDevice>) {
    let vendor = read16(bus, dev, 0, 0x00);
    if vendor == 0xFFFF {
        return; // no device
    }

    let header_type = read8(bus, dev, 0, 0x0E);
    let num_funcs = if header_type & 0x80 != 0 { 8 } else { 1 };

    for func in 0..num_funcs {
        let vendor = read16(bus, dev, func, 0x00);
        if vendor == 0xFFFF {
            continue;
        }
        let device_id = read16(bus, dev, func, 0x02);
        let class = read8(bus, dev, func, 0x0B);
        let subclass = read8(bus, dev, func, 0x0A);
        let prog_if = read8(bus, dev, func, 0x09);
        let revision = read8(bus, dev, func, 0x08);

        let mut bars = [0u32; 6];
        for (i, bar) in bars.iter_mut().enumerate() {
            *bar = read32(bus, dev, func, 0x10 + i as u8 * 4);
        }

        out.push(PciDevice {
            bus,
            dev,
            func,
            vendor_id: vendor,
            device_id,
            class,
            subclass,
            prog_if,
            revision,
            bars,
        });

        // Recurse into PCI-to-PCI bridges (class=0x06, subclass=0x04)
        if class == 0x06 && subclass == 0x04 {
            let secondary_bus = read8(bus, dev, func, 0x19);
            for d in 0u8..32 {
                scan_device(secondary_bus, d, out);
            }
        }
    }
}

/// Find the first XHCI host controller (class=0x0C, subclass=0x03, prog_if=0x30).
pub fn find_xhci(devices: &[PciDevice]) -> Option<&PciDevice> {
    devices
        .iter()
        .find(|d| d.class == 0x0C && d.subclass == 0x03 && d.prog_if == 0x30)
}

/// Find an Intel AX210-family Wi-Fi adapter.
///
/// Prefer known AX210/AX211 device IDs, but also accept Intel PCI functions
/// that identify as generic network controllers or wireless-class devices so
/// newer revisions are not silently missed.
pub fn find_ax210(devices: &[PciDevice]) -> Option<&PciDevice> {
    devices.iter().find(|d| {
        let known_ax210_id = matches!(d.device_id, 0x2725 | 0x51F0 | 0x54F0 | 0x7F70);
        let intel_network_controller = d.class == 0x02 && d.subclass == 0x80;
        let intel_wireless_controller = d.class == 0x0D && d.subclass == 0x11;
        d.vendor_id == 0x8086
            && (known_ax210_id || intel_network_controller || intel_wireless_controller)
    })
}

/// Enable Bus Master + Memory Space in the PCI command register (offset 0x04).
/// Must be called before any MMIO or DMA access to the device.
pub fn enable_bus_master(bus: u8, dev: u8, func: u8) {
    let addr = config_address(bus, dev, func, 0x04);
    unsafe {
        let mut addr_port: Port<u32> = Port::new(CONFIG_ADDRESS);
        let mut data_port: Port<u32> = Port::new(CONFIG_DATA);
        addr_port.write(addr);
        let val = data_port.read();
        // Bit 1 = Memory Space Enable, Bit 2 = Bus Master Enable
        addr_port.write(addr);
        data_port.write(val | 0b110);
    }
}
