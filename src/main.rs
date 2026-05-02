#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(rustos::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader::{BootInfo, entry_point};
use core::panic::PanicInfo;
use rustos::println;
use rustos::task::{Task, executor::Executor};

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    use core::sync::atomic::Ordering;
    use rustos::allocator;
    use rustos::memory::{self, BootInfoFrameAllocator};
    use x86_64::VirtAddr;

    rustos::init();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);

    // Store globals for use by drivers and process loader
    memory::PHYS_MEM_OFFSET.store(boot_info.physical_memory_offset, Ordering::Relaxed);
    {
        let mapper = unsafe { memory::init(phys_mem_offset) };
        *memory::GLOBAL_MAPPER.lock() = Some(mapper);
    }
    {
        let fa = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };
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

    #[cfg(test)]
    test_main();

    let mut executor = Executor::new();
    executor.spawn(Task::new(shell_task()));
    executor.run();
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

    // Mount all found devices (device 0 → /usb, device 1 → /usb1, …).
    rustos::usb::mount_storage_devices(0);
}

async fn shell_task() {
    use futures_util::stream::StreamExt;
    use pc_keyboard::{DecodedKey, HandleControl, Keyboard, ScancodeSet1, layouts};
    use rustos::shell::Shell;
    use rustos::task::keyboard::ScancodeStream;

    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );
    let mut shell = Shell::new();

    println!("  ____             _    ___  ____");
    println!(" |  _ \\ _   _ ___| |_ / _ \\/ ___|");
    println!(" | |_) | | | / __| __| | | \\___ \\");
    println!(" |  _ <| |_| \\__ \\ |_| |_| |___) |");
    println!(" |_| \\_\\\\__,_|___/\\__|\\___/|____/");
    println!();
    println!(
        "  v{}  --  type 'help' for a list of commands",
        env!("CARGO_PKG_VERSION")
    );
    println!();
    shell.print_prompt();

    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode)
            && let Some(key) = keyboard.process_keyevent(key_event)
            && let DecodedKey::Unicode(c) = key
        {
            shell.handle_char(c);
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
