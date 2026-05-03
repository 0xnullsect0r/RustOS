use x86_64::{
    VirtAddr,
    instructions::{
        hlt,
        port::Port,
        tables::{DescriptorTablePointer, lidt},
    },
};

const PCI_RESET_CONTROL_PORT: u16 = 0x0cf9;
const PCI_RESET_FULL: u8 = 0x06;
const I8042_STATUS_PORT: u16 = 0x64;
const I8042_RESET_COMMAND: u8 = 0xfe;
const I8042_INPUT_BUFFER_FULL: u8 = 0x02;

pub fn reboot() -> ! {
    crate::println!("Rebooting...");
    x86_64::instructions::interrupts::disable();

    unsafe {
        pci_reset();
        i8042_reset();
        triple_fault();
    }
}

unsafe fn pci_reset() {
    let mut reset_control = Port::<u8>::new(PCI_RESET_CONTROL_PORT);
    unsafe {
        reset_control.write(PCI_RESET_FULL);
    }
    spin_after_reset_attempt();
}

unsafe fn i8042_reset() {
    let mut status = Port::<u8>::new(I8042_STATUS_PORT);
    for _ in 0..100_000 {
        let busy = unsafe { status.read() } & I8042_INPUT_BUFFER_FULL != 0;
        if !busy {
            break;
        }
        core::hint::spin_loop();
    }

    let mut command = Port::<u8>::new(I8042_STATUS_PORT);
    unsafe {
        command.write(I8042_RESET_COMMAND);
    }
    spin_after_reset_attempt();
}

unsafe fn triple_fault() -> ! {
    let empty_idt = DescriptorTablePointer {
        limit: 0,
        base: VirtAddr::new(0),
    };
    unsafe {
        lidt(&empty_idt);
        core::arch::asm!("int3", options(noreturn));
    }
}

fn spin_after_reset_attempt() {
    for _ in 0..1_000_000 {
        hlt();
    }
}
