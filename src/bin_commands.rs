use crate::vfs::{NodeType, VFS};

const VIRTUAL_BIN_COMMANDS: &[&str] = &[
    "help", "echo", "clear", "uname", "color", "pwd", "ls", "cd", "mkdir", "rm", "cat", "write",
    "cp", "mv", "meminfo", "mount", "exec", "usbscan", "reboot",
];

pub fn is_virtual_bin_path(path: &str) -> Option<&str> {
    let cmd = path.strip_prefix("/bin/")?;
    if cmd.is_empty() || cmd.contains('/') {
        return None;
    }
    if VIRTUAL_BIN_COMMANDS.contains(&cmd) {
        Some(cmd)
    } else {
        None
    }
}

pub fn run_virtual_bin_command(path: &str) -> Option<i64> {
    let cmd = is_virtual_bin_path(path)?;
    let code = match cmd {
        "help" => cmd_help(),
        "echo" => cmd_echo(),
        "clear" => cmd_clear(),
        "uname" => cmd_uname(),
        "pwd" => cmd_pwd(),
        "ls" => cmd_ls(),
        "meminfo" => cmd_meminfo(),
        "mount" => cmd_mount(),
        "usbscan" => cmd_usbscan(),
        "reboot" => cmd_reboot(),
        "color" => {
            crate::println!("Usage: /bin/color <fg> <bg>");
            2
        }
        "cd" => {
            crate::println!("Usage: /bin/cd <path>");
            2
        }
        "mkdir" => {
            crate::println!("Usage: /bin/mkdir <path>");
            2
        }
        "rm" => {
            crate::println!("Usage: /bin/rm <path>");
            2
        }
        "cat" => {
            crate::println!("Usage: /bin/cat <path>");
            2
        }
        "write" => {
            crate::println!("Usage: /bin/write <path> <text>");
            2
        }
        "cp" => {
            crate::println!("Usage: /bin/cp <src> <dst>");
            2
        }
        "mv" => {
            crate::println!("Usage: /bin/mv <src> <dst>");
            2
        }
        "exec" => {
            crate::println!("Usage: /bin/exec <path>");
            2
        }
        _ => 127,
    };
    Some(code)
}

fn cmd_help() -> i64 {
    crate::println!("RustOS /bin commands:");
    for cmd in VIRTUAL_BIN_COMMANDS {
        crate::println!("  /bin/{}", cmd);
    }
    0
}

fn cmd_echo() -> i64 {
    crate::println!();
    0
}

fn cmd_clear() -> i64 {
    crate::drivers::vga::WRITER.lock().clear_screen();
    0
}

fn cmd_uname() -> i64 {
    crate::println!("RustOS v{}", env!("CARGO_PKG_VERSION"));
    crate::println!("Architecture: x86_64  Mode: Long mode (64-bit)");
    0
}

fn cmd_pwd() -> i64 {
    crate::println!("/");
    0
}

fn cmd_ls() -> i64 {
    let result = VFS.lock().as_mut().and_then(|vfs| vfs.list_dir("/").ok());
    match result {
        Some(mut entries) => {
            entries.sort_by(|a, b| a.name.cmp(&b.name));
            for e in &entries {
                match e.node_type {
                    NodeType::Directory => crate::println!("{}/", e.name),
                    NodeType::File => crate::println!("{}", e.name),
                }
            }
            0
        }
        None => {
            crate::println!("ls: /: not found or error");
            1
        }
    }
}

fn cmd_meminfo() -> i64 {
    crate::println!("Heap start: 0x{:016x}", crate::allocator::HEAP_START);
    crate::println!(
        "Heap size:  {} KiB ({} bytes)",
        crate::allocator::HEAP_SIZE / 1024,
        crate::allocator::HEAP_SIZE,
    );
    0
}

fn cmd_mount() -> i64 {
    let mounts = VFS.lock().as_ref().map(|vfs| vfs.list_mounts());
    match mounts {
        Some(m) if !m.is_empty() => {
            crate::println!("/  (root fs)");
            for mp in &m {
                crate::println!("{}  (fat32)", mp);
            }
        }
        _ => crate::println!("/  (root fs)  [no additional mounts]"),
    }
    0
}

fn cmd_usbscan() -> i64 {
    crate::println!("Scanning USB ports for new devices...");
    let new_devs = crate::usb::scan_and_mount();
    if new_devs == 0 {
        crate::println!("usbscan: no new USB storage devices found");
    } else {
        crate::println!("usbscan: {} new device(s) mounted", new_devs);
    }
    0
}

fn cmd_reboot() -> ! {
    crate::println!("Rebooting...");
    unsafe {
        x86_64::instructions::interrupts::disable();
        let mut port: x86_64::instructions::port::Port<u8> =
            x86_64::instructions::port::Port::new(0x64);
        port.write(0xFE_u8);
    }
    crate::hlt_loop();
}
