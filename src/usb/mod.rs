pub mod xhci;
pub mod mass_storage;

/// A block device that can read/write 512-byte sectors.
pub trait BlockDevice: Send {
    fn read_sectors(&mut self, lba: u64, count: u16) -> Option<alloc::vec::Vec<u8>>;
    fn sector_count(&self) -> u64;
}

/// Wraps the XHCI driver's device 0 as a BlockDevice.
pub struct XhciBlockDevice {
    pub ctrl: xhci::Xhci,
}

impl BlockDevice for XhciBlockDevice {
    fn read_sectors(&mut self, lba: u64, count: u16) -> Option<alloc::vec::Vec<u8>> {
        self.ctrl.read_sectors(lba, count)
    }
    fn sector_count(&self) -> u64 {
        self.ctrl.devices.first().map(|d| d.block_count).unwrap_or(0)
    }
}
