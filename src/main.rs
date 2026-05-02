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

    // Force early initialization of serial port (before enabling interrupts)
    // The serial port uses lazy_static, which initializes on first access.
    // If initialization happens AFTER interrupts are enabled, and an interrupt
    // fires during the lazy_static setup (before the Mutex is fully initialized),
    // the interrupt handler might try to acquire the partially-initialized Mutex,
    // causing a deadlock or undefined behavior. By forcing initialization here
    // while interrupts are still disabled, we ensure thread-safe access later.
    rustos::serial_println!("[kernel] Serial initialized");

    rustos::init();

    // Initialize framebuffer early if available (before println!)
    if let Some(framebuffer) = boot_info.framebuffer.take() {
        rustos::serial_println!("[kernel] Framebuffer available, initializing...");
        unsafe {
            rustos::drivers::framebuffer::init(framebuffer);
        }
        rustos::serial_println!("[kernel] Framebuffer initialized successfully");
        // Clear screen and show boot message
        println!("\n=== RustOS Kernel Initializing ===\n");
    } else {
        rustos::serial_println!("[kernel] No framebuffer available, using VGA fallback");
    }

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
    install_rsh_binary();

    // Probe PCI for XHCI and mount USB FAT32
    init_usb_storage();
    rustos::task::keyboard::init();

    #[cfg(test)]
    test_main();

    launch_rsh();
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

fn install_rsh_binary() {
    let rsh = include_bytes!(env!("RSH_ELF_PATH"));
    let mut guard = rustos::vfs::VFS.lock();
    let Some(vfs) = guard.as_mut() else {
        println!("[init] VFS not initialized; cannot install /bin/rsh");
        return;
    };
    match vfs.mkdir("/bin") {
        Ok(()) | Err(rustos::vfs::VfsError::AlreadyExists) => {}
        Err(e) => {
            println!("[init] failed to create /bin: {}", e);
            return;
        }
    }
    if let Err(e) = vfs.write_file("/bin/rsh", rsh) {
        println!("[init] failed to install /bin/rsh: {}", e);
    }
}

/// Runs the built-in kernel shell using the framebuffer output.
/// This provides a simple command interface without launching an external process.
fn launch_rsh() -> ! {
    rustos::serial_println!("[init] Starting built-in kernel shell...");
    println!("\n=== RustOS Built-in Shell ===");
    println!("Type 'help' for available commands\n");
    
    let mut shell = rustos::shell::Shell::new();
    shell.print_prompt();
    
    loop {
        // Wait for keyboard input
        if let Some(byte) = rustos::task::keyboard::read_input_byte() {
            if let Ok(c) = core::str::from_utf8(&[byte]) {
                if let Some(ch) = c.chars().next() {
                    shell.handle_char(ch);
                }
            }
        } else {
            // No input available, yield CPU
            x86_64::instructions::hlt();
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
