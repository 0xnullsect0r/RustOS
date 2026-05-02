pub mod frame_allocator;
pub use frame_allocator::{BootInfoFrameAllocator, EmptyFrameAllocator};

use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
        Size4KiB, mapper::MapToError,
    },
};

// ---------------------------------------------------------------------------
// Kernel-wide globals — set once by kernel_main before spawning the executor
// ---------------------------------------------------------------------------

pub static PHYS_MEM_OFFSET: AtomicU64 = AtomicU64::new(0);
pub static GLOBAL_MAPPER: Mutex<Option<OffsetPageTable<'static>>> = Mutex::new(None);
pub static GLOBAL_FRAME_ALLOC: Mutex<Option<BootInfoFrameAllocator>> = Mutex::new(None);

/// A zero-sized type that delegates to the global frame allocator.
/// Safe to pass to functions that require `&mut impl FrameAllocator`.
pub struct GlobalFrameAllocatorRef;

unsafe impl FrameAllocator<Size4KiB> for GlobalFrameAllocatorRef {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        GLOBAL_FRAME_ALLOC.lock().as_mut()?.allocate_frame()
    }
}

// ---------------------------------------------------------------------------
// DMA helpers
// ---------------------------------------------------------------------------

/// Allocate `size` bytes (aligned to `align`) backed by heap memory.
/// Returns `(virtual_ptr, physical_address)`.
/// Panics if the virtual address cannot be translated (should never happen for
/// memory allocated through our kernel heap).
pub fn dma_alloc(size: usize, align: usize) -> (*mut u8, u64) {
    use alloc::alloc::{Layout, alloc_zeroed};
    let layout = Layout::from_size_align(size, align.max(1)).unwrap();
    let ptr = unsafe { alloc_zeroed(layout) };
    assert!(!ptr.is_null(), "dma_alloc: allocation failed");
    let phys = virt_to_phys(ptr as u64).expect("dma_alloc: virt_to_phys failed");
    (ptr, phys)
}

/// Translate a kernel virtual address to the backing physical address by
/// walking the live page tables via the global mapper.
pub fn virt_to_phys(virt: u64) -> Option<u64> {
    use x86_64::structures::paging::Translate;
    let guard = GLOBAL_MAPPER.lock();
    guard
        .as_ref()?
        .translate_addr(VirtAddr::new(virt))
        .map(|p| p.as_u64())
}

/// Map a MMIO physical region into virtual address space and return the
/// virtual base address.  For QEMU the bootloader typically covers all
/// physical addresses with the `map_physical_memory` feature, so we first try
/// the fast path (`phys_mem_offset + phys_base`).  If those pages are not yet
/// present we fall back to an explicit `map_to` call.
pub fn map_mmio_region(phys_base: u64, size: usize) -> u64 {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let virt_base = offset + phys_base;
    let num_pages = size.div_ceil(4096);

    let mut mapper_guard = GLOBAL_MAPPER.lock();
    let mapper = mapper_guard
        .as_mut()
        .expect("map_mmio_region: mapper not initialized");

    for i in 0..num_pages as u64 {
        let virt = VirtAddr::new(virt_base + i * 4096);
        let phys = PhysAddr::new(phys_base + i * 4096);
        let page = Page::<Size4KiB>::containing_address(virt);
        let frame = PhysFrame::containing_address(phys);
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;

        match unsafe { mapper.map_to(page, frame, flags, &mut GlobalFrameAllocatorRef) } {
            Ok(flush) => flush.flush(),
            Err(MapToError::PageAlreadyMapped(_)) => {} // already mapped — fine
            Err(MapToError::ParentEntryHugePage) => {}  // covered by huge page — fine
            Err(e) => panic!("map_mmio_region: {:?}", e),
        }
    }

    virt_base
}

/// Map a set of virtual pages for a user process segment.
/// `virt_base` must be page-aligned; `size` is rounded up to the next page.
pub fn map_user_segment(virt_base: u64, size: usize) -> Result<(), MapToError<Size4KiB>> {
    use crate::serial_println;
    
    let num_pages = size.div_ceil(4096);
    let mut mapper_guard = GLOBAL_MAPPER.lock();
    let mapper = mapper_guard
        .as_mut()
        .ok_or(MapToError::FrameAllocationFailed)?;

    for i in 0..num_pages as u64 {
        let virt = VirtAddr::new(virt_base + i * 4096);
        let page = Page::<Size4KiB>::containing_address(virt);
        
        // Check if page is already mapped (can happen when ELF segments overlap at page boundaries)
        if mapper.translate_page(page).is_ok() {
            continue;
        }
        
        let frame = GlobalFrameAllocatorRef
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            match mapper.map_to(page, frame, flags, &mut GlobalFrameAllocatorRef) {
                Ok(flusher) => flusher.flush(),
                Err(e) => {
                    serial_println!("[memory] Failed to map page at 0x{:x}: {:?}", virt.as_u64(), e);
                    return Err(e);
                }
            }
        };
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Page table init (called once during boot)
// ---------------------------------------------------------------------------

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
