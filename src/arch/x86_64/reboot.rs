use core::sync::atomic::Ordering;
use x86_64::{
    VirtAddr,
    instructions::{
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

    pci_reset();
    i8042_reset();
    triple_fault();
}

/// Attempt an ACPI S5 power-off.
///
/// Strategy (tried in order):
///  1. Parse the real ACPI FADT to find PM1a/PM1b control ports, then try all
///     common SLP_TYP values for S5 (covers most real x86 hardware).
///  2. QEMU i440FX / q35 fixed port 0x604.
///  3. Bochs / old QEMU fixed port 0xB004.
///  4. HLT loop — machine is at least quiescent.
pub fn shutdown() -> ! {
    crate::println!("Shutting down...");
    x86_64::instructions::interrupts::disable();

    // 1. Try ACPI-discovered PM1 control ports (real hardware + modern QEMU)
    acpi_shutdown();

    // 2. QEMU i440FX / q35 fallback
    unsafe { Port::<u16>::new(0x0604).write(0x2000) };
    spin_short();

    // 3. Bochs / very old QEMU
    unsafe { Port::<u16>::new(0xB004).write(0x2000) };
    spin_short();

    // 4. Nothing worked
    loop {
        unsafe { core::arch::asm!("hlt", options(nostack, nomem, preserves_flags)) };
    }
}

// ---------------------------------------------------------------------------
// ACPI FADT-based shutdown
// ---------------------------------------------------------------------------

/// Walk RSDP → RSDT/XSDT → FADT, read PM1a/PM1b control block ports, then
/// issue SLP_EN with the common SLP_TYP_S5 candidates until the machine halts.
fn acpi_shutdown() {
    let phys_off = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if phys_off == 0 {
        return;
    }

    // --- Locate RSDP ---
    let rsdp = match find_rsdp(phys_off) {
        Some(r) => r,
        None => return,
    };

    // --- Locate FADT ---
    let (pm1a_port, pm1b_port) = match find_pm1_ports(phys_off, rsdp) {
        Some(p) => p,
        None => return,
    };

    // SLP_EN (bit 13) | SLP_TYP (bits 12:10).
    // SLP_TYP for S5 varies by firmware; try the most common values first:
    //   5 (0b101) — most Intel real-hardware systems
    //   7 (0b111) — many AMD and other systems
    //   6, 4, 3, 2, 1, 0 — descending fallback; 0 is QEMU's _S5_ { 0, 0 }
    for slp_typ in [5u16, 7, 6, 4, 3, 2, 1, 0] {
        let val: u16 = 0x2000 | (slp_typ << 10);
        unsafe {
            Port::<u16>::new(pm1a_port).write(val);
        }
        spin_short();
        if pm1b_port != 0 {
            unsafe {
                Port::<u16>::new(pm1b_port).write(val);
            }
            spin_short();
        }
    }
}

/// Read a physical address as a `*const u8` using the PHYS_MEM_OFFSET mapping.
///
/// # Safety
/// Caller must ensure `phys` is a valid physical address and `phys_off` is the
/// bootloader-established physical-memory offset.
unsafe fn phys_ptr(phys_off: u64, phys: u64) -> *const u8 {
    (phys_off + phys) as *const u8
}

/// Read a little-endian u16 from a byte slice at `offset`.
#[allow(dead_code)]
fn r16(buf: &[u8], offset: usize) -> u16 {
    if offset + 2 > buf.len() {
        return 0;
    }
    u16::from_le_bytes([buf[offset], buf[offset + 1]])
}

/// Read a little-endian u32 from a byte slice at `offset`.
fn r32(buf: &[u8], offset: usize) -> u32 {
    if offset + 4 > buf.len() {
        return 0;
    }
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

/// Read a little-endian u64 from a byte slice at `offset`.
fn r64(buf: &[u8], offset: usize) -> u64 {
    if offset + 8 > buf.len() {
        return 0;
    }
    u64::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
        buf[offset + 4],
        buf[offset + 5],
        buf[offset + 6],
        buf[offset + 7],
    ])
}

/// Read up to `len` bytes from physical address `phys` into a stack buffer.
/// Returns a slice of the filled portion.
fn read_phys(phys_off: u64, phys: u64, buf: &mut [u8]) -> &[u8] {
    let len = buf.len();
    unsafe {
        let src = phys_ptr(phys_off, phys);
        core::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), len);
    }
    buf
}

// RSDP structure size (ACPI 2.0+ = 36 bytes; 1.0 = 20 bytes)
const RSDP_V1_SIZE: usize = 20;
const RSDP_V2_SIZE: usize = 36;

/// Represents the parts of RSDP we care about.
struct Rsdp {
    revision: u8,
    rsdt_phys: u32,
    xsdt_phys: u64,
}

/// Search for the RSDP signature "RSD PTR " in:
///  - EBDA (first 1 KiB of the segment pointed to by [0x40E])
///  - BIOS ROM area 0x000E0000–0x000FFFFF
fn find_rsdp(phys_off: u64) -> Option<Rsdp> {
    // EBDA segment pointer is at physical 0x040E (16-bit segment × 16 = address)
    let ebda_seg = unsafe {
        let p = phys_ptr(phys_off, 0x040E) as *const u16;
        core::ptr::read_unaligned(p)
    };
    let ebda_base = (ebda_seg as u64) << 4;

    // Scan EBDA first 1 KiB, then BIOS ROM
    for &(start, end) in &[
        (ebda_base, ebda_base + 1024),
        (0x000E_0000u64, 0x000F_FFFFu64),
    ] {
        let mut addr = start;
        while addr + RSDP_V1_SIZE as u64 <= end {
            let mut buf = [0u8; RSDP_V2_SIZE];
            let slice = read_phys(phys_off, addr, &mut buf);
            if &slice[..8] == b"RSD PTR " && rsdp_checksum_ok(slice) {
                let rev = slice[15];
                let rsdt = r32(slice, 16);
                let xsdt = if rev >= 2 && slice.len() >= RSDP_V2_SIZE {
                    r64(slice, 24)
                } else {
                    0
                };
                return Some(Rsdp {
                    revision: rev,
                    rsdt_phys: rsdt,
                    xsdt_phys: xsdt,
                });
            }
            addr += 16; // RSDP is always 16-byte aligned
        }
    }
    None
}

fn rsdp_checksum_ok(buf: &[u8]) -> bool {
    let len = buf.len().min(RSDP_V1_SIZE);
    let sum: u8 = buf[..len].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    sum == 0
}

/// Walk RSDT or XSDT to find the FADT ("FACP") table, then read the
/// PM1a and PM1b control block port numbers.
/// Returns `(pm1a_port, pm1b_port)` where pm1b_port may be 0 if absent.
fn find_pm1_ports(phys_off: u64, rsdp: Rsdp) -> Option<(u16, u16)> {
    // Prefer XSDT (64-bit pointers) when available
    if rsdp.revision >= 2 && rsdp.xsdt_phys != 0 {
        find_pm1_in_sdt(phys_off, rsdp.xsdt_phys, true)
    } else if rsdp.rsdt_phys != 0 {
        find_pm1_in_sdt(phys_off, rsdp.rsdt_phys as u64, false)
    } else {
        None
    }
}

/// Read an SDT header (36 bytes) and iterate its table pointers to find FACP.
fn find_pm1_in_sdt(phys_off: u64, sdt_phys: u64, xsdt: bool) -> Option<(u16, u16)> {
    let mut hdr = [0u8; 36];
    read_phys(phys_off, sdt_phys, &mut hdr);

    let table_len = r32(&hdr, 4) as usize;
    if table_len < 36 {
        return None;
    }
    let entries_bytes = table_len - 36;
    let ptr_size: usize = if xsdt { 8 } else { 4 };
    let entry_count = entries_bytes / ptr_size;

    for i in 0..entry_count {
        let entry_off = 36 + i * ptr_size;

        // Read one entry worth of data from the SDT
        let entry_phys: u64 = if xsdt {
            let mut buf = [0u8; 8];
            read_phys(phys_off, sdt_phys + entry_off as u64, &mut buf);
            r64(&buf, 0)
        } else {
            let mut buf = [0u8; 4];
            read_phys(phys_off, sdt_phys + entry_off as u64, &mut buf);
            r32(&buf, 0) as u64
        };

        if entry_phys == 0 {
            continue;
        }

        // Read SDT entry signature
        let mut sig = [0u8; 4];
        read_phys(phys_off, entry_phys, &mut sig);
        if &sig == b"FACP" {
            return read_fadt_pm1(phys_off, entry_phys);
        }
    }
    None
}

/// Read PM1a_CNT_BLK (offset 64) and PM1b_CNT_BLK (offset 68) from FADT.
/// These are I/O port addresses (truncated to u16 since PM ports are in the
/// first 64 KiB of I/O space).
fn read_fadt_pm1(phys_off: u64, fadt_phys: u64) -> Option<(u16, u16)> {
    // FADT is at minimum 116 bytes for ACPI 1.0; PM1 blocks are at 64/68
    let mut fadt = [0u8; 116];
    read_phys(phys_off, fadt_phys, &mut fadt);

    let pm1a = r32(&fadt, 64);
    let pm1b = r32(&fadt, 68);

    if pm1a == 0 {
        return None;
    }

    // PM1 I/O addresses are in the lower 16-bit I/O space
    Some((pm1a as u16, pm1b as u16))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn pci_reset() {
    let mut reset_control = Port::<u8>::new(PCI_RESET_CONTROL_PORT);
    unsafe {
        reset_control.write(PCI_RESET_FULL);
    }
    spin_short();
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
    spin_short();
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

/// Short busy-wait to give hardware time to act on a reset/shutdown write.
fn spin_short() {
    for _ in 0..500_000 {
        unsafe { core::arch::asm!("pause", options(nostack, nomem, preserves_flags)) };
    }
}
