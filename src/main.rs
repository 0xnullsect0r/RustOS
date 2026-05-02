#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(rustos::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use rustos::println;
use rustos::task::{Task, executor::Executor};
use bootloader::{BootInfo, entry_point};
use core::panic::PanicInfo;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    use rustos::allocator;
    use rustos::memory::{self, BootInfoFrameAllocator};
    use core::sync::atomic::Ordering;
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

/// Scan PCI for an XHCI controller, initialise it, and mount any FAT32 volume at `/usb`.
fn init_usb_storage() {
    use rustos::pci;
    use rustos::usb::xhci::Xhci;
    use rustos::vfs::{VFS, Fat32Mount};
    use rustos::fs::fat32::Fat32Fs;
    use rustos::usb::XhciBlockDevice;
    use alloc::boxed::Box;

    let devices = pci::enumerate();
    let dev = match pci::find_xhci(&devices) {
        Some(d) => d,
        None => {
            rustos::serial_println!("[usb] no XHCI controller found");
            return;
        }
    };

    let ctrl = match Xhci::init(&dev) {
        Some(c) => c,
        None => {
            rustos::serial_println!("[usb] XHCI init failed");
            return;
        }
    };

    if ctrl.devices.is_empty() {
        rustos::serial_println!("[usb] no USB storage devices enumerated");
        return;
    }

    rustos::serial_println!("[usb] {} device(s) found", ctrl.devices.len());

    let block_dev = XhciBlockDevice { ctrl };
    let fat32 = match Fat32Fs::new(Box::new(block_dev)) {
        Some(fs) => fs,
        None => {
            rustos::serial_println!("[usb] no FAT32 volume found on USB device");
            return;
        }
    };
    let mut vfs = VFS.lock();
    if let Some(vfs) = vfs.as_mut() {
        vfs.mount("/usb", Box::new(Fat32Mount(fat32)));
        println!("[usb] FAT32 volume mounted at /usb");
    }
}

async fn shell_task() {
    use rustos::task::keyboard::ScancodeStream;
    use rustos::shell::Shell;
    use futures_util::stream::StreamExt;
    use pc_keyboard::{DecodedKey, HandleControl, Keyboard, ScancodeSet1, layouts};

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
    println!("  v{}  --  type 'help' for a list of commands", env!("CARGO_PKG_VERSION"));
    println!();
    shell.print_prompt();

    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
                if let DecodedKey::Unicode(c) = key {
                    shell.handle_char(c);
                }
            }
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
