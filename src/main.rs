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

/// Launches `/bin/rsh` as the init shell process and restarts it on exit.
fn launch_rsh() -> ! {
    let embedded_rsh = include_bytes!(env!("RSH_ELF_PATH"));
    rustos::serial_println!("[init] Launching /bin/rsh...");
    println!("RustOS v{} — launching /bin/rsh", env!("CARGO_PKG_VERSION"));
    
    const MAX_CONSECUTIVE_FAILURES: usize = 10;
    // Approximate delay: ~10-20ms on modern CPUs, prevents tight loop resource exhaustion
    const RETRY_DELAY_ITERATIONS: usize = 10_000_000;
    let mut consecutive_failures = 0;
    
    loop {
        let from_vfs = {
            let mut guard = rustos::vfs::VFS.lock();
            let Some(vfs) = guard.as_mut() else {
                println!("[init] VFS not initialized");
                rustos::hlt_loop();
            };
            vfs.read_file("/bin/rsh")
        };
        let data = match from_vfs {
            Ok(d) => d,
            Err(e) => {
                println!(
                    "[init] /bin/rsh missing in filesystem ({}), using embedded fallback",
                    e
                );
                embedded_rsh.to_vec()
            }
        };
        match rustos::process::exec(&data) {
            Ok(code) => {
                println!("[init] /bin/rsh exited with code {}", code);
                consecutive_failures = 0; // Reset on successful execution
            }
            Err(e) => {
                println!("[init] /bin/rsh failed to start: {}", e);
                
                // Mapping failures indicate critical errors that won't be fixed by retrying
                // (out of memory, corrupted ELF, or page table issues)
                if e.contains("mapping failed") {
                    println!("[init] Memory mapping failure indicates a critical system error");
                    println!("[init] This could be due to: insufficient memory, corrupted binary, or page table corruption");
                    println!("[init] Halting system - retry would not help");
                    rustos::hlt_loop();
                }
                
                consecutive_failures += 1;
                
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    println!("[init] Too many consecutive failures ({}), halting system", consecutive_failures);
                    rustos::hlt_loop();
                }
                
                // Add a delay to prevent tight loop that could exhaust resources
                println!("[init] Waiting before retry... ({}/{})", consecutive_failures, MAX_CONSECUTIVE_FAILURES);
                for _ in 0..RETRY_DELAY_ITERATIONS {
                    core::hint::spin_loop();
                }
            }
        }
        println!("[init] restarting /bin/rsh");
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
