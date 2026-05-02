//! Minimal ELF64 loader for executing programs stored in the VFS.
//!
//! Processes run at ring 0 (same privilege as the kernel).  There is no
//! address-space isolation — the kernel and the program share one page table.
//! The process entry point is called as a regular function; it returns when
//! the program calls `sys_exit` (via `int 0x80`) or when `_start` returns.
//!
//! # Process termination
//! When the process calls `sys_exit`, the kernel's syscall handler invokes
//! `exit_process()`, which performs a longjmp back to exec().  This is
//! necessary because the process `_start` diverges after sys_exit — returning
//! normally from the interrupt would re-enter the process's `loop {}`.
//!
//! # Virtual address layout
//! ```text
//! 0x0040_0000  process code/data segments (from ELF)
//! 0x0080_0000  process stack  (64 KiB, grows down)
//! ```

use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};

/// Saved kernel RSP for longjmp when the running process calls sys_exit.
///
/// Points to a slot on the kernel stack (pushed by exec()) that holds the
/// return address for label `9f` inside exec().  Zero = no process active.
pub static EXEC_LONGJMP_RSP: AtomicU64 = AtomicU64::new(0);

/// Called by `syscall::sys_exit` to return control to exec().
///
/// Restores the kernel stack saved by exec() and jumps to the return label
/// inside exec(). Does NOT return if a longjmp context is present; returns
/// `false` if no process is currently running.
pub fn exit_process() -> bool {
    let saved_rsp = EXEC_LONGJMP_RSP.swap(0, Ordering::SeqCst);
    if saved_rsp == 0 {
        return false;
    }
    // Re-enable interrupts (int 0x80 may have cleared IF) and longjmp.
    unsafe {
        core::arch::asm!(
            "sti",
            "mov rsp, {rsp}",
            "ret",
            rsp = in(reg) saved_rsp,
            options(noreturn),
        );
    }
}

// ---------------------------------------------------------------------------
// ELF64 constants & structures
// ---------------------------------------------------------------------------

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const PT_LOAD: u32 = 1;

#[repr(C, packed)]
struct Elf64Hdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C, packed)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

const STACK_BASE: u64 = 0x0080_0000;
const STACK_SIZE: usize = 64 * 1024; // 64 KiB

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load and execute an ELF64 binary contained in `data`.
///
/// Returns `Ok(exit_code)` on clean exit, or `Err(description)` on load error.
pub fn exec(data: &[u8]) -> Result<i64, String> {
    // ---- validate ELF header ------------------------------------------------
    if data.len() < core::mem::size_of::<Elf64Hdr>() {
        return Err(String::from("file too small"));
    }
    let hdr = unsafe { &*(data.as_ptr() as *const Elf64Hdr) };
    if hdr.e_ident[..4] != ELF_MAGIC {
        return Err(String::from("not an ELF file"));
    }
    if hdr.e_ident[4] != 2 {
        return Err(String::from("not a 64-bit ELF"));
    }
    if hdr.e_machine != 62 {
        return Err(String::from("not x86_64"));
    }

    // ---- map PT_LOAD segments -----------------------------------------------
    let phoff = hdr.e_phoff as usize;
    let phnum = hdr.e_phnum as usize;
    let phentsz = hdr.e_phentsize as usize;

    for i in 0..phnum {
        let off = phoff + i * phentsz;
        if off + core::mem::size_of::<Elf64Phdr>() > data.len() {
            return Err(String::from("program header out of bounds"));
        }
        let ph = unsafe { &*(data.as_ptr().add(off) as *const Elf64Phdr) };
        if ph.p_type != PT_LOAD || ph.p_memsz == 0 {
            continue;
        }

        let vaddr = ph.p_vaddr;
        let file_size = ph.p_filesz;
        let mem_size = ph.p_memsz;
        let file_offset = ph.p_offset;
        let page_start = vaddr & !0xFFF;
        let page_end = (vaddr + mem_size + 0xFFF) & !0xFFF;
        let size = (page_end - page_start) as usize;

        crate::memory::map_user_segment(page_start, size)
            .map_err(|_| String::from("segment mapping failed (see serial for details)"))?;

        // Copy file data into the freshly mapped virtual memory
        let file_start = file_offset as usize;
        let file_end = file_start + file_size as usize;
        if file_end > data.len() {
            return Err(String::from("segment file data out of bounds"));
        }

        unsafe {
            let dst = vaddr as *mut u8;
            core::ptr::copy_nonoverlapping(data.as_ptr().add(file_start), dst, file_size as usize);

            // Zero BSS (mem_size > file_size)
            if mem_size > file_size {
                core::ptr::write_bytes(
                    dst.add(file_size as usize),
                    0,
                    (mem_size - file_size) as usize,
                );
            }
        }
    }

    // ---- set up stack -------------------------------------------------------
    crate::memory::map_user_segment(STACK_BASE, STACK_SIZE)
        .map_err(|_| String::from("stack mapping failed (see serial for details)"))?;
    let stack_top = STACK_BASE + STACK_SIZE as u64;

    // ---- clear previous exit code / longjmp context ------------------------
    *crate::syscall::PROCESS_EXIT_CODE.lock() = None;
    EXEC_LONGJMP_RSP.store(0, Ordering::SeqCst);

    // ---- call entry point ---------------------------------------------------
    let entry = hdr.e_entry;
    let longjmp_ptr = &EXEC_LONGJMP_RSP as *const AtomicU64 as usize;
    unsafe {
        core::arch::asm!(
            // Save kernel rsp in r15 (callee-saved; survives the call).
            "mov r15, rsp",
            // Push the address of label 9 onto the kernel stack, then save
            // that rsp as the longjmp target.  sys_exit will do:
            //   mov rsp, saved_rsp ; ret  → pops label9 addr → jumps to 9:
            "lea rax, [rip + 9f]",
            "push rax",
            "mov qword ptr [{ptr}], rsp",
            // Switch to process stack and call entry.
            "mov rsp, {stack}",
            "push 0",               // fake return address for _start
            "call {entry}",
            // Label 9: reached via (a) _start returning naturally or
            //          (b) sys_exit longjmp.  Restore kernel rsp either way.
            "9:",
            "mov rsp, r15",
            ptr   = in(reg) longjmp_ptr,
            stack = in(reg) stack_top,
            entry = in(reg) entry,
            out("r15") _,
            out("rax") _,
            options(nostack),
        );
    }

    // ---- collect exit code --------------------------------------------------
    let code = crate::syscall::PROCESS_EXIT_CODE.lock().unwrap_or(0);
    Ok(code)
}
