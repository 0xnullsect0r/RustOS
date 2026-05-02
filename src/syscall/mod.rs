//! Syscall interface — invoked via `int 0x80` from userspace or in-kernel processes.
//!
//! Register convention (matches Linux x86_64 for familiarity):
//!   rax = syscall number
//!   rdi = arg1, rsi = arg2, rdx = arg3
//!   Return value in rax (negative = error).

use x86_64::structures::idt::InterruptStackFrame;

/// Syscall numbers — must match rustos-rt/src/lib.rs.
/// Uses Linux-compatible numbering so rustos-rt programs feel familiar.
pub const SYS_READ: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_EXEC: u64 = 59;
pub const SYS_EXIT: u64 = 60;

/// File descriptors
pub const FD_STDIN: u64 = 0;
pub const FD_STDOUT: u64 = 1;
pub const FD_STDERR: u64 = 2;

/// Holds the exit code of the last process that called sys_exit.
/// Reset to None before exec(), set by the handler.
pub static PROCESS_EXIT_CODE: spin::Mutex<Option<i64>> = spin::Mutex::new(None);

/// The raw interrupt handler registered for int 0x80.
/// We use the `x86-interrupt` calling convention which saves/restores all
/// caller-saved registers automatically.
pub extern "x86-interrupt" fn syscall_handler(_stack_frame: InterruptStackFrame) {
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
    let ret = match nr {
        SYS_READ => sys_read(a1, a2 as *mut u8, a3 as usize),
        SYS_WRITE => sys_write(a1, a2 as *const u8, a3 as usize),
        SYS_EXEC => sys_exec(a1 as *const u8),
        SYS_EXIT => {
            sys_exit(a1 as i64);
            0
        }
        _ => {
            crate::serial_println!("[syscall] unknown nr={}", nr);
            -38 // -ENOSYS
        }
    };
    unsafe {
        core::arch::asm!(
            "mov rax, {ret}",
            ret = in(reg) ret as u64,
            options(nostack, nomem, preserves_flags),
        );
    }
}

fn sys_exit(code: i64) {
    crate::println!("\n[process exited with code {}]", code);
    *PROCESS_EXIT_CODE.lock() = Some(code);
    // Longjmp back to exec() — does not return if a process is active.
    if !crate::process::exit_process() {
        crate::serial_println!("[syscall] sys_exit with no active process context");
    }
}

fn sys_write(fd: u64, buf: *const u8, len: usize) -> i64 {
    if buf.is_null() || len == 0 {
        return 0;
    }
    // Safety: the process is ring-0; buffer validity is caller's responsibility.
    let slice = unsafe { core::slice::from_raw_parts(buf, len) };
    if let Ok(s) = core::str::from_utf8(slice) {
        match fd {
            FD_STDOUT => crate::print!("{}", s),
            FD_STDERR => {
                crate::serial_print!("{}", s);
            }
            _ => return -9, // -EBADF
        }
        len as i64
    } else {
        -22 // -EINVAL
    }
}

fn sys_read(fd: u64, buf: *mut u8, len: usize) -> i64 {
    if fd != FD_STDIN {
        return -9; // -EBADF
    }
    if buf.is_null() || len == 0 {
        return 0;
    }
    let out = unsafe { core::slice::from_raw_parts_mut(buf, len) };
    let mut n = 0usize;
    while n < out.len() {
        match crate::task::keyboard::read_input_byte() {
            Some(b) => {
                out[n] = b;
                n += 1;
                if b == b'\n' || b == b'\r' {
                    break;
                }
            }
            None => break,
        }
    }
    n as i64
}

fn sys_exec(path_ptr: *const u8) -> i64 {
    if path_ptr.is_null() {
        return -22; // -EINVAL
    }
    let path = unsafe { cstr_to_str(path_ptr) };
    let Some(path) = path else {
        return -22;
    };
    let data = {
        let mut guard = crate::vfs::VFS.lock();
        let Some(vfs) = guard.as_mut() else {
            return -5; // -EIO
        };
        match vfs.read_file(path) {
            Ok(d) => d,
            Err(_) => return -2, // -ENOENT
        }
    };
    crate::process::exec(&data).unwrap_or(-8)
}

unsafe fn cstr_to_str(ptr: *const u8) -> Option<&'static str> {
    let mut len = 0usize;
    while len < 4096 {
        if unsafe { *ptr.add(len) } == 0 {
            break;
        }
        len += 1;
    }
    if len == 4096 {
        return None;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    core::str::from_utf8(bytes).ok()
}
