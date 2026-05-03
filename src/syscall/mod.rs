//! Syscall interface — invoked via `int 0x80` from userspace or in-kernel processes.
//!
//! Register convention (matches Linux x86_64 for familiarity):
//!   rax = syscall number
//!   rdi = arg1, rsi = arg2, rdx = arg3
//!   Return value in rax (negative = error).

use alloc::{string::String, vec::Vec};
use core::arch::naked_asm;

/// Syscall numbers — must match rustos-rt/src/lib.rs.
/// Uses Linux-compatible numbering so rustos-rt programs feel familiar.
pub const SYS_READ: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_OPEN: u64 = 2;
pub const SYS_CLOSE: u64 = 3;
pub const SYS_EXEC: u64 = 59;
pub const SYS_EXIT: u64 = 60;
const MAX_CSTR_LEN: usize = 4096;

/// File descriptors
pub const FD_STDIN: u64 = 0;
pub const FD_STDOUT: u64 = 1;
pub const FD_STDERR: u64 = 2;

/// Holds the exit code of the last process that called sys_exit.
/// Reset to None before exec(), set by the handler.
pub static PROCESS_EXIT_CODE: spin::Mutex<Option<i64>> = spin::Mutex::new(None);

/// Raw int 0x80 syscall gate.
///
/// Implemented as a `#[naked]` function to prevent the compiler-generated
/// x86-interrupt prologue/epilogue from silently restoring rax (with the
/// original syscall number) before `iretq`, which would discard the return
/// value written by `dispatch()`.
///
/// The function:
///   1. Saves caller-saved registers (rax is the syscall nr — not saved in a
///      slot because we overwrite rax with the result on exit).
///   2. Translates the syscall register arguments to the SysV AMD64 ABI and
///      calls `dispatch(nr, a1, a2, a3)`.
///   3. Stashes dispatch's return value in r11 (caller-saved; acceptable to
///      clobber across an int 0x80 call).
///   4. Restores all other saved registers, places the return value in rax,
///      and executes `iretq`.
///
/// # Safety
///
/// Must only be called by the CPU as an IDT interrupt handler (registered via
/// `idt[0x80].set_handler_addr`).  Calling it directly from Rust code is
/// undefined behaviour.
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_handler() {
    naked_asm!(
        // Save caller-saved GPRs.  We do NOT push rax here; we will write
        // the return value into rax just before iretq instead.
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        // Translate syscall args → SysV AMD64 ABI for dispatch(nr,a1,a2,a3).
        // On entry: rax=nr, rdi=a1, rsi=a2, rdx=a3.
        "mov rcx, rdx",  // a3 → 4th arg (rcx)
        "mov rdx, rsi",  // a2 → 3rd arg (rdx)
        "mov rsi, rdi",  // a1 → 2nd arg (rsi)
        "mov rdi, rax",  // nr → 1st arg (rdi)
        "call {dispatch}",
        // rax = i64 return value from dispatch().
        // Stash it in r11 (caller-saved; clobbering it across int 0x80 is
        // acceptable).  Then skip the saved r11 slot on the stack (keeping
        // r11 = return value) and restore the remaining registers in reverse
        // push order.
        "mov r11, rax",  // stash return value in r11
        "add rsp, 8",    // skip saved r11 slot on stack
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        // Place return value in rax for userspace, then return.
        "mov rax, r11",
        "iretq",
        dispatch = sym dispatch,
    );
}

extern "C" fn dispatch(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    match nr {
        SYS_READ => sys_read(a1, a2 as *mut u8, a3 as usize),
        SYS_WRITE => sys_write(a1, a2 as *const u8, a3 as usize),
        SYS_OPEN => sys_open(a1 as *const u8),
        SYS_CLOSE => sys_close(a1 as i64),
        SYS_EXEC => sys_exec(a1 as *const u8),
        SYS_EXIT => {
            sys_exit(a1 as i64);
            0
        }
        300..=310 => {
            tcp_ip::kernel::dispatch_syscall(nr, a1, a2, a3)
                .unwrap_or(-38) // -ENOSYS if unrecognised within tcp-ip
        }
        nr => {
            if let Some(ret) = crate::net::dispatch_syscall(nr, a1, a2, a3) {
                ret
            } else {
                crate::serial_println!("[syscall] unknown nr={}", nr);
                -38 // -ENOSYS (function not implemented)
            }
        }
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
            // Non-blocking read for now; callers may poll until input arrives.
            None => break,
        }
    }
    n as i64
}

fn sys_exec(path_ptr: *const u8) -> i64 {
    if path_ptr.is_null() {
        return -22; // -EINVAL
    }
    let path = cstr_to_string(path_ptr);
    let Some(path) = path else {
        return -22;
    };
    if let Some(code) = crate::bin_commands::run_virtual_bin_command(&path) {
        return code;
    }
    let data = {
        let mut guard = crate::vfs::VFS.lock();
        let Some(vfs) = guard.as_mut() else {
            return -5; // -EIO
        };
        match vfs.read_file(&path) {
            Ok(d) => d,
            Err(_) => return -2, // -ENOENT
        }
    };
    crate::process::exec(&data).unwrap_or(-8)
}

fn sys_open(path_ptr: *const u8) -> i64 {
    if path_ptr.is_null() {
        return -22; // -EINVAL
    }
    let path = cstr_to_string(path_ptr);
    let Some(path) = path else {
        return -22;
    };
    if crate::bin_commands::is_virtual_bin_path(&path).is_some() {
        return 3;
    }
    let exists = crate::vfs::VFS
        .lock()
        .as_mut()
        .map(|vfs| vfs.exists(&path))
        .unwrap_or(false);
    if exists { 3 } else { -2 } // -ENOENT
}

fn sys_close(_fd: i64) -> i64 {
    0
}

fn cstr_to_string(ptr: *const u8) -> Option<String> {
    let mut bytes = Vec::new();
    let mut found_nul = false;
    for i in 0..MAX_CSTR_LEN {
        let b = unsafe { *ptr.add(i) };
        if b == 0 {
            found_nul = true;
            break;
        }
        bytes.push(b);
    }
    if !found_nul {
        return None;
    }
    String::from_utf8(bytes).ok()
}
