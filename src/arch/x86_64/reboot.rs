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

// ACPI PM1a Control register (QEMU i440FX / q35 default ACPI IO base = 0x600).
// PM1a_CNT is at offset +4 → port 0x604.
// Writing SLP_EN (bit 13) | SLP_TYP=0 (bits 12:10=0b000) = 0x2000 triggers S5
// power-off under QEMU's _S5_ { 0, 0 } ACPI object.
const ACPI_PM1A_CNT_PORT: u16 = 0x604;
const ACPI_S5_SLEEP: u16 = 0x2000; // SLP_EN | SLP_TYP=S5 for QEMU

// Bochs / older QEMU (< 2.0) use a different ACPI IO base.
const BOCHS_ACPI_PM_PORT: u16 = 0xB004;

pub fn reboot() -> ! {
    crate::println!("Rebooting...");
    x86_64::instructions::interrupts::disable();

    pci_reset();
    i8042_reset();
    triple_fault();
}

/// Attempt an ACPI S5 power-off and spin forever if the hardware ignores it.
///
/// Works on QEMU (i440FX and q35 machines) and Bochs.  On bare metal the
/// firmware's ACPI FADT / _S5_ object may use different port/value pairs;
/// those require full ACPI table parsing which is beyond this stub.
pub fn shutdown() -> ! {
    crate::println!("Shutting down...");
    x86_64::instructions::interrupts::disable();

    // 1. QEMU ACPI PM1a_CNT shutdown (works on i440FX and q35).
    unsafe {
        Port::<u16>::new(ACPI_PM1A_CNT_PORT).write(ACPI_S5_SLEEP);
    }
    spin_after_reset_attempt();

    // 2. Bochs / very old QEMU ACPI shutdown register.
    unsafe {
        Port::<u16>::new(BOCHS_ACPI_PM_PORT).write(ACPI_S5_SLEEP);
    }
    spin_after_reset_attempt();

    // 3. Nothing worked — halt all CPUs so the machine is at least quiescent.
    loop {
        unsafe { core::arch::asm!("hlt", options(nostack, nomem, preserves_flags)) };
    }
}

fn pci_reset() {
    let mut reset_control = Port::<u8>::new(PCI_RESET_CONTROL_PORT);
    unsafe {
        reset_control.write(PCI_RESET_FULL);
    }
    spin_after_reset_attempt();
}

fn i8042_reset() {
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

fn triple_fault() -> ! {
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
