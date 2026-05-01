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
    use x86_64::VirtAddr;

    rustos::init();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init(&boot_info.memory_map)
    };
    allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");

    #[cfg(test)]
    test_main();

    let mut executor = Executor::new();
    executor.spawn(Task::new(shell_task()));
    executor.run();
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
