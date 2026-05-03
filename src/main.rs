#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(rustos::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader_api::config::Mapping;
use bootloader_api::{BootInfo, BootloaderConfig, entry_point};
use core::panic::PanicInfo;
use rustos::memory::PHYS_MEM_OFFSET;
use rustos::println;

const BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    use core::sync::atomic::Ordering;
    use rustos::allocator;
    use rustos::memory::{self, BootInfoFrameAllocator};
    use x86_64::VirtAddr;

    // Real firmware may leave IF set. Keep hardware IRQs off until RustOS has
    // loaded its own IDT and initialized every interrupt handler dependency.
    x86_64::instructions::interrupts::disable();

    // Take over the bootloader-provided framebuffer before any normal console
    // output. On real UEFI laptops, legacy VGA and COM1 serial hardware may be
    // absent, so the GOP framebuffer is the first reliable kernel diagnostic.
    if let Some(framebuffer) = boot_info.framebuffer.take() {
        unsafe {
            rustos::drivers::framebuffer::init(framebuffer);
        }
        println!("\n=== RustOS Kernel Initializing ===\n");
        println!("[kernel] UEFI framebuffer initialized");
    } else {
        rustos::drivers::serial::init();
        rustos::serial_println!("[kernel] No framebuffer available, using serial/VGA fallback");
    }

    // Load GDT/IDT and program the PIC now, but do not enable interrupts until
    // memory, heap, VFS, USB probing, and the keyboard queue are initialized.
    rustos::init_without_interrupts();

    let phys_mem_offset = VirtAddr::new(
        boot_info
            .physical_memory_offset
            .into_option()
            .expect("physical memory mapping not configured"),
    );

    // Store globals for use by drivers and process loader
    PHYS_MEM_OFFSET.store(
        boot_info.physical_memory_offset.into_option().unwrap(),
        Ordering::Relaxed,
    );
    {
        let mapper = unsafe { memory::init(phys_mem_offset) };
        *memory::GLOBAL_MAPPER.lock() = Some(mapper);
    }
    {
        let fa = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_regions) };
        *memory::GLOBAL_FRAME_ALLOC.lock() = Some(fa);
    }

    // Init heap using global mapper + global frame allocator
    {
        let mut mg = memory::GLOBAL_MAPPER.lock();
        let mapper = mg.as_mut().unwrap();
        allocator::init_heap(mapper, &mut memory::GlobalFrameAllocatorRef)
            .expect("heap initialization failed");
    }

    // Initialise VFS
    rustos::vfs::init();

    // Probe PCI for XHCI and mount USB FAT32
    init_usb_storage();
    rustos::task::keyboard::init();
    rustos::enable_interrupts();

    #[cfg(test)]
    test_main();

    // This enters the interactive shell loop and never returns.
    launch_kernel_shell();
}

/// Scan PCI for an XHCI controller, initialise it, store it in the global
/// `USB_XHCI`, and mount any FAT32 volumes found on connected drives.
fn init_usb_storage() {
    use rustos::pci;
    use rustos::usb::USB_XHCI;
    use rustos::usb::xhci::Xhci;

    let devices = pci::enumerate();
    let dev = match pci::find_xhci(&devices) {
        Some(d) => d,
        None => {
            rustos::serial_println!("[usb] no XHCI controller found");
            return;
        }
    };

    let ctrl = match Xhci::init(dev) {
        Some(c) => c,
        None => {
            rustos::serial_println!("[usb] XHCI init failed");
            return;
        }
    };

    let found = ctrl.devices.len();
    rustos::serial_println!("[usb] XHCI ready, {} device(s) enumerated", found);

    // Store the controller globally so ongoing I/O and hot-plug rescans work.
    *USB_XHCI.lock() = Some(ctrl);

    if found == 0 {
        rustos::serial_println!("[usb] no USB storage devices found at boot");
        return;
    }

    if !rustos::usb::mount_boot_storage_root() {
        rustos::serial_println!("[usb] no boot storage partition mounted as root");
    }

    // Mount all found devices under /usb* (device 0 → /usb, device 1 → /usb1, …).
    rustos::usb::mount_storage_devices(0);
}

/// Runs the built-in kernel shell using the framebuffer output.
/// This provides a simple command interface without launching an external process.
fn launch_kernel_shell() -> ! {
    rustos::serial_println!("[init] Starting built-in kernel shell...");
    println!("\n=== RustOS Built-in Shell ===");
    println!("Type 'help' for available commands\n");

    let mut shell = rustos::shell::Shell::new();
    shell.print_prompt();

    loop {
        // Wait for keyboard input
        if let Some(byte) = rustos::task::keyboard::read_input_byte() {
            shell.handle_char(byte as char);
        } else if rustos::task::keyboard::interrupt_input_observed() {
            x86_64::instructions::hlt();
        } else {
            // Before seeing any IRQ1 traffic, do not halt: some real laptops
            // expose PS/2-compatible keyboard data only through polling after
            // UEFI handoff. Once an IRQ arrives, the branch above can hlt.
            core::hint::spin_loop();
        }
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    rustos::hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    rustos::test_panic_handler(info)
}

#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1);
}
