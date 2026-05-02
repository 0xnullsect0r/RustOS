//! Syscall interface — invoked via `int 0x80` from userspace or in-kernel processes.
//!
//! Register convention (matches Linux x86_64 for familiarity):
//!   rax = syscall number
//!   rdi = arg1, rsi = arg2, rdx = arg3
//!   Return value in rax (negative = error).

use x86_64::structures::idt::InterruptStackFrame;

/// Syscall numbers
pub const SYS_EXIT:  u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_READ:  u64 = 2;

/// File descriptors
pub const FD_STDIN:  u64 = 0;
pub const FD_STDOUT: u64 = 1;
pub const FD_STDERR: u64 = 2;

/// Holds the exit code of the last process that called sys_exit.
/// Reset to None before exec(), set by the handler.
pub static PROCESS_EXIT_CODE: spin::Mutex<Option<i64>> = spin::Mutex::new(None);

/// The raw interrupt handler registered for int 0x80.
/// We use the `x86-interrupt` calling convention which saves/restores all
/// caller-saved registers automatically.
pub extern "x86-interrupt" fn syscall_handler(stack_frame: InterruptStackFrame) {
    // Read registers saved by the CPU / interrupt prologue.
    // We retrieve rax/rdi/rsi/rdx via inline asm before the compiler clobbers them.
    let (nr, a1, a2, a3): (u64, u64, u64, u64);
    unsafe {
        core::arch::asm!(
            "mov {nr}, rax",
            "mov {a1}, rdi",
            "mov {a2}, rsi",
            "mov {a3}, rdx",
            nr = out(reg) nr,
            a1 = out(reg) a1,
            a2 = out(reg) a2,
            a3 = out(reg) a3,
            options(nostack, nomem),
        );
    }
    dispatch(nr, a1, a2, a3);
}

fn dispatch(nr: u64, a1: u64, a2: u64, a3: u64) {
    match nr {
        SYS_EXIT  => sys_exit(a1 as i64),
        SYS_WRITE => { sys_write(a1, a2 as *const u8, a3 as usize); }
        SYS_READ  => { sys_read(a1, a2 as *mut u8, a3 as usize); }
        _         => { crate::serial_println!("[syscall] unknown nr={}", nr); }
    }
}

fn sys_exit(code: i64) {
    crate::println!("\n[process exited with code {}]", code);
    *PROCESS_EXIT_CODE.lock() = Some(code);
    // Execution returns to the interrupt handler, which returns to the call
    // site in process::exec() — the process stack frame unwinds naturally.
}

fn sys_write(fd: u64, buf: *const u8, len: usize) {
    if buf.is_null() || len == 0 {
        return;
    }
    // Safety: the process is ring-0; buffer validity is caller's responsibility.
    let slice = unsafe { core::slice::from_raw_parts(buf, len) };
    if let Ok(s) = core::str::from_utf8(slice) {
        match fd {
            FD_STDOUT => crate::print!("{}", s),
            FD_STDERR => { crate::serial_print!("{}", s); }
            _ => {}
        }
    }
}

fn sys_read(fd: u64, buf: *mut u8, len: usize) -> usize {
    // Keyboard read is async; for simplicity return 0 (would block in real OS).
    let _ = (fd, buf, len);
    0
}
