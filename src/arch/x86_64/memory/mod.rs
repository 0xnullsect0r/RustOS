pub mod frame_allocator;
pub use frame_allocator::{BootInfoFrameAllocator, EmptyFrameAllocator};

use x86_64::{
    VirtAddr,
    structures::paging::{OffsetPageTable, PageTable},
};

/// Initialize a new OffsetPageTable.
///
/// # Safety
/// The caller must guarantee that the complete physical memory is mapped to
/// virtual memory at `physical_memory_offset`, and that this function is only
/// called once.
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    unsafe {
        let level_4_table = active_level_4_table(physical_memory_offset);
        OffsetPageTable::new(level_4_table, physical_memory_offset)
    }
}

unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    unsafe { &mut *page_table_ptr }
}
