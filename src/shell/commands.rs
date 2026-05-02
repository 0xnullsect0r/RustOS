//! Built-in shell command implementations.

use super::Shell;
use crate::drivers::vga::Color;
use crate::vfs::{NodeType, VFS};

/// Dispatch a parsed command name and argument list to the appropriate handler.
pub fn dispatch(shell: &mut Shell, cmd: &str, args: &[&str]) {
    match cmd {
        "help" => cmd_help(),
        "echo" => cmd_echo(args),
        "clear" => cmd_clear(),
        "uname" => cmd_uname(),
        "color" => cmd_color(shell, args),
        "pwd" => cmd_pwd(shell),
        "ls" => cmd_ls(shell, args),
        "cd" => cmd_cd(shell, args),
        "mkdir" => cmd_mkdir(shell, args),
        "rm" => cmd_rm(shell, args),
        "cat" => cmd_cat(shell, args),
        "write" => cmd_write(shell, args),
        "cp" => cmd_cp(shell, args),
        "mv" => cmd_mv(shell, args),
        "meminfo" => cmd_meminfo(),
        "mount" => cmd_mount(),
        "exec" => cmd_exec(shell, args),
        "usbscan" => cmd_usbscan(),
        "reboot" => cmd_reboot(),
        other => crate::println!("unknown command: '{}'. Type 'help' for a list.", other),
    }
}

fn cmd_help() {
    crate::println!("Built-in commands:");
    crate::println!("  help              - Show this help message");
    crate::println!("  echo <text>       - Print text to the screen");
    crate::println!("  clear             - Clear the screen");
    crate::println!("  uname             - Show OS and architecture info");
    crate::println!("  color <fg> <bg>   - Set text colors (e.g. color yellow black)");
    crate::println!("  pwd               - Print the current working directory");
    crate::println!("  ls [path]         - List directory contents");
    crate::println!("  cd <path>         - Change the current directory");
    crate::println!("  mkdir <path>      - Create a new directory");
    crate::println!("  rm <path>         - Remove a file or empty directory");
    crate::println!("  cat <path>        - Print the contents of a file");
    crate::println!("  write <path> <t>  - Write text to a file (overwrites)");
    crate::println!("  cp <src> <dst>    - Copy a file");
    crate::println!("  mv <src> <dst>    - Move or rename a file/directory");
    crate::println!("  meminfo           - Show heap memory information");
    crate::println!("  mount             - Show mounted filesystems");
    crate::println!("  exec <path>       - Execute an ELF binary from the VFS");
    crate::println!("  usbscan           - Scan for newly plugged-in USB drives and mount them");
    crate::println!("  reboot            - Reboot the system");
}

fn cmd_echo(args: &[&str]) {
    crate::println!("{}", args.join(" "));
}

fn cmd_clear() {
    crate::drivers::vga::WRITER.lock().clear_screen();
}

fn cmd_uname() {
    crate::println!("RustOS v{}", env!("CARGO_PKG_VERSION"));
    crate::println!("Architecture: x86_64  Mode: Long mode (64-bit)");
    crate::println!("Compiled with Rust nightly (no_std, custom bootloader)");
}

fn parse_color(s: &str) -> Option<Color> {
    match s {
        "black" => Some(Color::Black),
        "blue" => Some(Color::Blue),
        "green" => Some(Color::Green),
        "cyan" => Some(Color::Cyan),
        "red" => Some(Color::Red),
        "magenta" => Some(Color::Magenta),
        "brown" => Some(Color::Brown),
        "lightgray" => Some(Color::LightGray),
        "darkgray" => Some(Color::DarkGray),
        "lightblue" => Some(Color::LightBlue),
        "lightgreen" => Some(Color::LightGreen),
        "lightcyan" => Some(Color::LightCyan),
        "lightred" => Some(Color::LightRed),
        "pink" => Some(Color::Pink),
        "yellow" => Some(Color::Yellow),
        "white" => Some(Color::White),
        _ => None,
    }
}

fn cmd_color(shell: &mut Shell, args: &[&str]) {
    if args.len() < 2 {
        crate::println!("Usage: color <fg> <bg>");
        crate::println!("Colors: black blue green cyan red magenta brown lightgray");
        crate::println!(
            "        darkgray lightblue lightgreen lightcyan lightred pink yellow white"
        );
        return;
    }
    let fg = match parse_color(args[0]) {
        Some(c) => c,
        None => {
            crate::println!("Unknown color: '{}'", args[0]);
            return;
        }
    };
    let bg = match parse_color(args[1]) {
        Some(c) => c,
        None => {
            crate::println!("Unknown color: '{}'", args[1]);
            return;
        }
    };
    shell.fg_color = fg;
    shell.bg_color = bg;
    crate::drivers::vga::WRITER.lock().set_color(fg, bg);
}

fn cmd_pwd(shell: &Shell) {
    crate::println!("{}", shell.cwd);
}

fn cmd_ls(shell: &mut Shell, args: &[&str]) {
    let input = args.first().copied().unwrap_or("");
    let path = shell.resolve_path(input);
    let result = VFS.lock().as_mut().and_then(|vfs| vfs.list_dir(&path).ok());
    match result {
        Some(mut entries) => {
            entries.sort_by(|a, b| a.name.cmp(&b.name));
            for e in &entries {
                match e.node_type {
                    NodeType::Directory => crate::println!("{}/", e.name),
                    NodeType::File => crate::println!("{}", e.name),
                }
            }
        }
        None => crate::println!("ls: {}: not found or error", path),
    }
}

fn cmd_cd(shell: &mut Shell, args: &[&str]) {
    let input = args.first().copied().unwrap_or("/");
    let path = shell.resolve_path(input);
    let is_dir = VFS
        .lock()
        .as_mut()
        .map(|vfs| vfs.is_dir(&path))
        .unwrap_or(false);
    if is_dir {
        shell.cwd = path;
    } else {
        crate::println!("cd: {}: not a directory", path);
    }
}

fn cmd_mkdir(shell: &mut Shell, args: &[&str]) {
    let Some(&input) = args.first() else {
        crate::println!("Usage: mkdir <path>");
        return;
    };
    let path = shell.resolve_path(input);
    let result = VFS.lock().as_mut().map(|vfs| vfs.mkdir(&path));
    match result {
        Some(Ok(())) => {}
        Some(Err(e)) => crate::println!("mkdir: {}", e),
        None => crate::println!("mkdir: VFS not initialised"),
    }
}

fn cmd_rm(shell: &mut Shell, args: &[&str]) {
    let Some(&input) = args.first() else {
        crate::println!("Usage: rm <path>");
        return;
    };
    let path = shell.resolve_path(input);
    let result = VFS.lock().as_mut().map(|vfs| vfs.remove(&path));
    match result {
        Some(Ok(())) => {}
        Some(Err(e)) => crate::println!("rm: {}", e),
        None => crate::println!("rm: VFS not initialised"),
    }
}

fn cmd_cat(shell: &mut Shell, args: &[&str]) {
    let Some(&input) = args.first() else {
        crate::println!("Usage: cat <path>");
        return;
    };
    let path = shell.resolve_path(input);
    let result = VFS
        .lock()
        .as_mut()
        .and_then(|vfs| vfs.read_file(&path).ok());
    match result {
        Some(data) => match core::str::from_utf8(&data) {
            Ok(s) => crate::println!("{}", s),
            Err(_) => {
                for &b in &data {
                    if b.is_ascii() {
                        crate::print!("{}", b as char);
                    } else {
                        crate::print!("?");
                    }
                }
                crate::println!();
            }
        },
        None => crate::println!("cat: {}: not found or error", path),
    }
}

fn cmd_write(shell: &mut Shell, args: &[&str]) {
    if args.len() < 2 {
        crate::println!("Usage: write <path> <text>");
        return;
    }
    let path = shell.resolve_path(args[0]);
    let text = args[1..].join(" ");
    let result = VFS
        .lock()
        .as_mut()
        .map(|vfs| vfs.write_file(&path, text.as_bytes()));
    match result {
        Some(Ok(())) => {}
        Some(Err(e)) => crate::println!("write: {}", e),
        None => crate::println!("write: VFS not initialised"),
    }
}

fn cmd_cp(shell: &mut Shell, args: &[&str]) {
    if args.len() < 2 {
        crate::println!("Usage: cp <src> <dst>");
        return;
    }
    let src = shell.resolve_path(args[0]);
    let dst = shell.resolve_path(args[1]);
    let result = VFS.lock().as_mut().map(|vfs| vfs.copy(&src, &dst));
    match result {
        Some(Ok(())) => {}
        Some(Err(e)) => crate::println!("cp: {}", e),
        None => crate::println!("cp: VFS not initialised"),
    }
}

fn cmd_mv(shell: &mut Shell, args: &[&str]) {
    if args.len() < 2 {
        crate::println!("Usage: mv <src> <dst>");
        return;
    }
    let src = shell.resolve_path(args[0]);
    let dst = shell.resolve_path(args[1]);
    let result = VFS.lock().as_mut().map(|vfs| vfs.rename(&src, &dst));
    match result {
        Some(Ok(())) => {}
        Some(Err(e)) => crate::println!("mv: {}", e),
        None => crate::println!("mv: VFS not initialised"),
    }
}

fn cmd_meminfo() {
    crate::println!("Heap start: 0x{:016x}", crate::allocator::HEAP_START);
    crate::println!(
        "Heap size:  {} KiB ({} bytes)",
        crate::allocator::HEAP_SIZE / 1024,
        crate::allocator::HEAP_SIZE,
    );
}

fn cmd_mount() {
    let mounts = VFS.lock().as_ref().map(|vfs| vfs.list_mounts());
    match mounts {
        Some(m) if !m.is_empty() => {
            crate::println!("/  (ramfs)");
            for mp in &m {
                crate::println!("{}  (fat32)", mp);
            }
        }
        _ => crate::println!("/  (ramfs)  [no additional mounts]"),
    }
}

fn cmd_exec(shell: &mut Shell, args: &[&str]) {
    let Some(&input) = args.first() else {
        crate::println!("Usage: exec <path>");
        return;
    };
    let path = shell.resolve_path(input);
    let data = {
        let result = VFS
            .lock()
            .as_mut()
            .and_then(|vfs| vfs.read_file(&path).ok());
        match result {
            Some(d) => d,
            None => {
                crate::println!("exec: {}: not found", path);
                return;
            }
        }
    };

    // Initialise the process CWD to match the kernel shell's current directory.
    *crate::syscall::PROCESS_CWD.lock() = shell.cwd.clone();

    // Drain accumulated scancodes so the kernel shell doesn't replay keystrokes
    // that the user typed while the process was running.
    crate::task::keyboard::drain_scancode_queue();

    match crate::process::exec(&data) {
        Ok(code) => crate::println!("exec: process exited with code {}", code),
        Err(e) => crate::println!("exec: load error: {}", e),
    }

    // Discard any bytes buffered for stdin that the process did not consume,
    // and any scancodes accumulated during execution.
    crate::task::keyboard::drain_stdin();
    crate::task::keyboard::drain_scancode_queue();

    // Close any file descriptors the process left open.
    crate::syscall::fd_table::close_all();
}

fn cmd_usbscan() {
    crate::println!("Scanning USB ports for new devices...");
    let new_devs = crate::usb::scan_and_mount();
    if new_devs == 0 {
        crate::println!("usbscan: no new USB storage devices found");
    } else {
        crate::println!("usbscan: {} new device(s) mounted", new_devs);
    }
}

fn cmd_reboot() {
    crate::println!("Rebooting...");
    unsafe {
        x86_64::instructions::interrupts::disable();
        let mut port: x86_64::instructions::port::Port<u8> =
            x86_64::instructions::port::Port::new(0x64);
        port.write(0xFE_u8);
    }
    crate::hlt_loop();
}
