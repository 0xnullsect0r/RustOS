//! Syscall interface — invoked via `int 0x80` from userspace or in-kernel processes.
//!
//! Register convention (matches Linux x86_64 for familiarity):
//!   rax = syscall number
//!   rdi = arg1, rsi = arg2, rdx = arg3
//!   Return value in rax (negative = error).

pub mod fd_table;

use x86_64::structures::idt::InterruptStackFrame;

/// Syscall numbers — must match rustos-rt/src/lib.rs.
/// Uses Linux-compatible numbering so rustos-rt programs feel familiar.
pub const SYS_READ: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_OPEN: u64 = 2;
pub const SYS_CLOSE: u64 = 3;
pub const SYS_EXEC: u64 = 59;
pub const SYS_EXIT: u64 = 60;
pub const SYS_GETCWD: u64 = 79;
pub const SYS_CHDIR: u64 = 80;
pub const SYS_GETDENTS64: u64 = 217;

/// File descriptors
pub const FD_STDIN: u64 = 0;
pub const FD_STDOUT: u64 = 1;
pub const FD_STDERR: u64 = 2;

/// Holds the exit code of the last process that called sys_exit.
/// Reset to None before exec(), set by the handler.
pub static PROCESS_EXIT_CODE: spin::Mutex<Option<i64>> = spin::Mutex::new(None);

/// Current working directory for the running userspace process.
/// The kernel's exec command updates this; SYS_CHDIR modifies it in place.
pub static PROCESS_CWD: spin::Mutex<alloc::string::String> =
    spin::Mutex::new(alloc::string::String::new());

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
    match nr {
        SYS_READ => sys_read(a1, a2 as *mut u8, a3 as usize),
        SYS_WRITE => sys_write(a1, a2 as *const u8, a3 as usize),
        SYS_OPEN => sys_open(a1 as *const u8, a2 as usize),
        SYS_CLOSE => sys_close(a1 as i64),
        SYS_EXEC => sys_exec(a1 as *const u8, a2 as usize),
        SYS_EXIT => sys_exit(a1 as i64),
        SYS_GETCWD => sys_getcwd(a1 as *mut u8, a2 as usize),
        SYS_CHDIR => sys_chdir(a1 as *const u8, a2 as usize),
        SYS_GETDENTS64 => sys_getdents64(a1 as i64, a2 as *mut u8, a3 as usize),
        _ => {
            crate::serial_println!("[syscall] unknown nr={}", nr);
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

fn sys_write(fd: u64, buf: *const u8, len: usize) {
    if buf.is_null() || len == 0 {
        return;
    }
    // Safety: the process is ring-0; buffer validity is caller's responsibility.
    let slice = unsafe { core::slice::from_raw_parts(buf, len) };
    if let Ok(s) = core::str::from_utf8(slice) {
        match fd {
            FD_STDOUT => crate::print!("{}", s),
            FD_STDERR => {
                crate::serial_print!("{}", s);
            }
            _ => {}
        }
    }
}

/// Write a return value back into rax after the syscall returns.
/// We store it in a thread-local-ish static and read it back in the handler
/// via inline asm.  Since we are single-core and non-preemptible here this
/// is safe.
fn set_retval(val: i64) {
    unsafe {
        core::arch::asm!(
            "mov rax, {v}",
            v = in(reg) val,
            options(nostack, nomem),
        );
    }
}

fn sys_read(fd: u64, buf: *mut u8, len: usize) {
    if buf.is_null() || len == 0 {
        set_retval(0);
        return;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
    let n = match fd {
        FD_STDIN => crate::task::keyboard::read_stdin(slice) as i64,
        _ => fd_table::read(fd as i64, slice),
    };
    set_retval(n);
}

fn sys_open(path_ptr: *const u8, _flags: usize) {
    if path_ptr.is_null() {
        set_retval(-14); // EFAULT
        return;
    }
    // The path is NUL-terminated; find its length.
    let path_bytes = unsafe {
        let mut len = 0usize;
        let mut p = path_ptr;
        while *p != 0 {
            len += 1;
            p = p.add(1);
        }
        core::slice::from_raw_parts(path_ptr, len)
    };
    let path = match core::str::from_utf8(path_bytes) {
        Ok(s) => s,
        Err(_) => {
            set_retval(-22); // EINVAL
            return;
        }
    };
    set_retval(fd_table::open(path));
}

fn sys_close(fd: i64) {
    set_retval(fd_table::close(fd));
}

fn sys_exec(path_ptr: *const u8, _path_len: usize) {
    if path_ptr.is_null() {
        set_retval(-14); // EFAULT
        return;
    }
    // NUL-terminated path.
    let path_bytes = unsafe {
        let mut len = 0usize;
        let mut p = path_ptr;
        while *p != 0 {
            len += 1;
            p = p.add(1);
        }
        core::slice::from_raw_parts(path_ptr, len)
    };
    let path = match core::str::from_utf8(path_bytes) {
        Ok(s) => s,
        Err(_) => {
            set_retval(-22); // EINVAL
            return;
        }
    };

    // Read the ELF from the VFS.
    let data = {
        let result = crate::vfs::VFS
            .lock()
            .as_mut()
            .and_then(|v| v.read_file(path).ok());
        match result {
            Some(d) => d,
            None => {
                set_retval(-2); // ENOENT
                return;
            }
        }
    };

    match crate::process::exec(&data) {
        Ok(code) => set_retval(code),
        Err(_) => set_retval(-8), // ENOEXEC
    }
}

fn sys_getcwd(buf: *mut u8, len: usize) {
    if buf.is_null() || len == 0 {
        set_retval(-14); // EFAULT
        return;
    }
    let cwd = PROCESS_CWD.lock();
    let cwd_bytes = cwd.as_bytes();
    let n = cwd_bytes.len().min(len - 1);
    unsafe {
        core::ptr::copy_nonoverlapping(cwd_bytes.as_ptr(), buf, n);
        *buf.add(n) = 0;
    }
    set_retval(n as i64);
}

fn sys_chdir(path_ptr: *const u8, path_len: usize) {
    if path_ptr.is_null() {
        set_retval(-14); // EFAULT
        return;
    }
    let path_bytes = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
    // Strip trailing NUL if present.
    let path_bytes = match path_bytes.iter().position(|&b| b == 0) {
        Some(n) => &path_bytes[..n],
        None => path_bytes,
    };
    let path = match core::str::from_utf8(path_bytes) {
        Ok(s) => s,
        Err(_) => {
            set_retval(-22); // EINVAL
            return;
        }
    };

    // Verify the path is a directory in the VFS.
    let is_dir = crate::vfs::VFS
        .lock()
        .as_mut()
        .map(|v| v.is_dir(path))
        .unwrap_or(false);

    if is_dir {
        *PROCESS_CWD.lock() = alloc::string::String::from(path);
        set_retval(0);
    } else {
        set_retval(-2); // ENOENT
    }
}

fn sys_getdents64(fd: i64, buf: *mut u8, len: usize) {
    if buf.is_null() || len == 0 {
        set_retval(-14); // EFAULT
        return;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
    set_retval(fd_table::getdents64(fd, slice));
}

