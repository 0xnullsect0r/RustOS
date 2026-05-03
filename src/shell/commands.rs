//! Built-in shell command implementations.

use super::Shell;
use crate::drivers::vga::Color;
use crate::vfs::{NodeType, VFS};

/// Dispatch a parsed command name and argument list to the appropriate handler.
pub fn dispatch(shell: &mut Shell, cmd: &str, args: &[&str]) {
    match cmd {
        "help" => print_help(),
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
        "net" => cmd_net(),
        "exec" => cmd_exec(shell, args),
        "usbscan" => cmd_usbscan(),
        "reboot" => cmd_reboot(),
        "shutdown" => cmd_shutdown(),
        "lspci" => cmd_lspci(),
        "lsusb" => cmd_lsusb(),
        "lsblk" => cmd_lsblk(),
        "grep" => cmd_grep(shell, args),
        "ps" => cmd_ps(args),
        "wifi" => cmd_wifi(args),
        "ping" => cmd_ping(args),
        "ifconfig" => cmd_ifconfig(args),
        "netstat" => cmd_netstat(args),
        other => crate::println!("unknown command: '{}'. Type 'help' for a list.", other),
    }
}

pub fn print_help() {
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
    crate::println!("  net               - Show tcp-ip stack and WiFi status");
    crate::println!("  exec <path>       - Execute an ELF binary from the VFS");
    crate::println!("  usbscan           - Scan for newly plugged-in USB drives and mount them");
    crate::println!("  reboot            - Reboot the system");
    crate::println!("  shutdown          - Power off the system");
    crate::println!("  lspci             - List PCI devices");
    crate::println!("  lsusb             - List USB devices");
    crate::println!("  lsblk             - List block devices");
    crate::println!("  grep <pat> <file> - Search file for lines matching pattern");
    crate::println!("  ps [aux]          - List running processes");
    crate::println!("  wifi [status|scan|connect] - WiFi control");
    crate::println!("  ping <host>       - Test network connectivity");
    crate::println!("  ifconfig          - Show network interface configuration");
    crate::println!("  netstat           - Show active network connections");
    crate::println!();
    crate::println!("/bin commands:");
    for cmd in crate::bin_commands::virtual_bin_commands() {
        crate::println!("  /bin/{}", cmd);
    }
}

fn cmd_echo(args: &[&str]) {
    crate::println!("{}", args.join(" "));
}

fn cmd_clear() {
    // The VGA module owns the console compatibility API; it clears whichever
    // backend is active, including the UEFI framebuffer on real hardware.
    crate::drivers::vga::clear_screen();
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
    crate::drivers::vga::set_color(fg, bg);
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
    let vfs = VFS.lock();
    match vfs.as_ref() {
        Some(vfs) => {
            crate::println!("/  ({})", vfs.root_name());
            let m = vfs.list_mounts();
            for mp in &m {
                crate::println!("{}  (fat32)", mp);
            }
        }
        None => crate::println!("mount: VFS not initialised"),
    }
}

fn cmd_net() {
    crate::net::print_status();
}

fn cmd_exec(shell: &mut Shell, args: &[&str]) {
    let Some(&input) = args.first() else {
        crate::println!("Usage: exec <path>");
        return;
    };
    let path = shell.resolve_path(input);
    if let Some(code) = crate::bin_commands::run_virtual_bin_command(&path) {
        crate::println!("exec: process exited with code {}", code);
        return;
    }
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
    match crate::process::exec(&data) {
        Ok(code) => crate::println!("exec: process exited with code {}", code),
        Err(e) => crate::println!("exec: load error: {}", e),
    }
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
    crate::reboot::reboot();
}

fn cmd_shutdown() {
    crate::reboot::shutdown();
}

// ---------------------------------------------------------------------------
// lspci
// ---------------------------------------------------------------------------

fn pci_vendor_name(vendor: u16) -> &'static str {
    match vendor {
        0x1002 => "AMD/ATI",
        0x1022 => "AMD",
        0x1106 => "VIA Technologies",
        0x10B9 => "ALi",
        0x10DE => "NVIDIA Corp.",
        0x10EC => "Realtek Semiconductor",
        0x1234 => "QEMU/Bochs",
        0x1AF4 => "Virtio",
        0x1B21 => "ASMedia Technology",
        0x1B36 => "Red Hat (QEMU)",
        0x15AD => "VMware",
        0x15B7 => "SanDisk",
        0x80EE => "VirtualBox",
        0x8086 => "Intel Corp.",
        _ => "Unknown vendor",
    }
}

fn pci_class_name(class: u8, subclass: u8, prog_if: u8) -> &'static str {
    match (class, subclass, prog_if) {
        (0x00, 0x00, _) => "Non-VGA pre-PCI device",
        (0x00, 0x01, _) => "VGA pre-PCI device",
        (0x01, 0x00, _) => "SCSI controller",
        (0x01, 0x01, _) => "IDE controller",
        (0x01, 0x02, _) => "Floppy disk controller",
        (0x01, 0x05, _) => "ATA controller",
        (0x01, 0x06, 0x01) => "AHCI SATA controller",
        (0x01, 0x06, _) => "SATA controller",
        (0x01, 0x07, _) => "SAS controller",
        (0x01, 0x08, 0x02) => "NVMe controller",
        (0x01, 0x08, _) => "Mass storage controller",
        (0x02, 0x00, _) => "Ethernet controller",
        (0x02, 0x80, _) => "Network controller",
        (0x03, 0x00, _) => "VGA compatible controller",
        (0x03, 0x01, _) => "XGA controller",
        (0x03, 0x02, _) => "3D controller",
        (0x04, 0x00, _) => "Video device",
        (0x04, 0x01, _) => "Audio device",
        (0x04, 0x03, _) => "HD Audio controller",
        (0x05, 0x00, _) => "RAM memory",
        (0x06, 0x00, _) => "Host bridge",
        (0x06, 0x01, _) => "ISA bridge",
        (0x06, 0x02, _) => "EISA bridge",
        (0x06, 0x04, _) => "PCI-PCI bridge",
        (0x07, 0x00, _) => "Serial controller",
        (0x07, 0x01, _) => "Parallel controller",
        (0x08, 0x00, _) => "PIC",
        (0x08, 0x01, _) => "DMA controller",
        (0x08, 0x02, _) => "Timer",
        (0x08, 0x03, _) => "RTC",
        (0x09, 0x00, _) => "Keyboard controller",
        (0x09, 0x02, _) => "Mouse controller",
        (0x0B, 0x00, _) => "386 processor",
        (0x0C, 0x00, _) => "FireWire controller",
        (0x0C, 0x03, 0x00) => "UHCI USB controller",
        (0x0C, 0x03, 0x10) => "OHCI USB controller",
        (0x0C, 0x03, 0x20) => "EHCI USB2 controller",
        (0x0C, 0x03, 0x30) => "XHCI USB3 controller",
        (0x0C, 0x05, _) => "SMBus",
        (0x0C, 0x06, _) => "InfiniBand controller",
        (0x0D, 0x11, _) => "Bluetooth",
        (0x0D, 0x12, _) => "802.11 Wi-Fi",
        (0x12, 0x00, _) => "Processing accelerator",
        _ => "Unknown device",
    }
}

pub fn cmd_lspci() {
    let devices = crate::pci::enumerate();
    if devices.is_empty() {
        crate::println!("lspci: no PCI devices found");
        return;
    }
    for d in &devices {
        crate::println!(
            "{:02x}:{:02x}.{} {:04x}:{:04x}  {}  {}",
            d.bus,
            d.dev,
            d.func,
            d.vendor_id,
            d.device_id,
            pci_vendor_name(d.vendor_id),
            pci_class_name(d.class, d.subclass, d.prog_if),
        );
    }
}

// ---------------------------------------------------------------------------
// lsusb
// ---------------------------------------------------------------------------

pub fn cmd_lsusb() {
    let xhci = crate::usb::USB_XHCI.lock();
    match xhci.as_ref() {
        Some(ctrl) => {
            if ctrl.devices.is_empty() {
                crate::println!("lsusb: no USB devices found");
            } else {
                for (i, dev) in ctrl.devices.iter().enumerate() {
                    let total_mb = dev.block_count * dev.block_size as u64 / (1024 * 1024);
                    crate::println!(
                        "Bus 001 Device {:03}: USB Mass Storage  {} blocks × {} B  ({} MiB)",
                        i + 1,
                        dev.block_count,
                        dev.block_size,
                        total_mb,
                    );
                }
            }
        }
        None => crate::println!("lsusb: USB controller not initialized"),
    }
}

// ---------------------------------------------------------------------------
// lsblk
// ---------------------------------------------------------------------------

pub fn cmd_lsblk() {
    let xhci = crate::usb::USB_XHCI.lock();
    match xhci.as_ref() {
        Some(ctrl) => {
            if ctrl.devices.is_empty() {
                crate::println!("lsblk: no block devices found");
            } else {
                crate::println!("NAME     SIZE       TYPE");
                for (i, dev) in ctrl.devices.iter().enumerate() {
                    let total_bytes = dev.block_count * dev.block_size as u64;
                    let (size, unit) = if total_bytes >= 1024 * 1024 * 1024 {
                        (total_bytes / (1024 * 1024 * 1024), "GiB")
                    } else if total_bytes >= 1024 * 1024 {
                        (total_bytes / (1024 * 1024), "MiB")
                    } else {
                        (total_bytes / 1024, "KiB")
                    };
                    let name = if i == 0 {
                        alloc::string::String::from("usb")
                    } else {
                        alloc::format!("usb{}", i)
                    };
                    crate::println!("{:<8} {:>4} {}    usb-storage", name, size, unit);
                }
            }
        }
        None => crate::println!("lsblk: USB controller not initialized"),
    }
}

// ---------------------------------------------------------------------------
// grep
// ---------------------------------------------------------------------------

fn cmd_grep(shell: &mut Shell, args: &[&str]) {
    if args.len() < 2 {
        crate::println!("Usage: grep <pattern> <file>");
        return;
    }
    let pattern = args[0];
    let path = shell.resolve_path(args[1]);
    let data = {
        let result = VFS
            .lock()
            .as_mut()
            .and_then(|vfs| vfs.read_file(&path).ok());
        match result {
            Some(d) => d,
            None => {
                crate::println!("grep: {}: no such file", path);
                return;
            }
        }
    };
    let text = match core::str::from_utf8(&data) {
        Ok(s) => s,
        Err(_) => {
            crate::println!("grep: {}: binary file", path);
            return;
        }
    };
    let mut matched = 0usize;
    for line in text.lines() {
        if line.contains(pattern) {
            crate::println!("{}", line);
            matched += 1;
        }
    }
    if matched == 0 {
        crate::println!("grep: no match for '{}' in {}", pattern, path);
    }
}

// ---------------------------------------------------------------------------
// ps
// ---------------------------------------------------------------------------

pub fn cmd_ps(_args: &[&str]) {
    crate::println!("PID  STAT CMD");
    crate::println!("  0  S    [kernel]");
    let exec_rsp = crate::process::EXEC_LONGJMP_RSP.load(core::sync::atomic::Ordering::SeqCst);
    if exec_rsp != 0 {
        crate::println!("  1  R    [exec]");
    }
}

// ---------------------------------------------------------------------------
// wifi
// ---------------------------------------------------------------------------

pub fn cmd_wifi(args: &[&str]) {
    match args.first().copied().unwrap_or("status") {
        "status" | "" => {
            crate::println!("WiFi status:");
            crate::net::print_status();
        }
        "scan" => {
            crate::println!("Scanning for wireless networks...");
            crate::println!("(No 802.11 hardware found or driver not ready)");
        }
        "connect" => {
            if args.len() < 2 {
                crate::println!("Usage: wifi connect <ssid> [password]");
            } else {
                crate::println!("Connecting to '{}'...", args[1]);
                crate::println!("Error: WiFi hardware not initialised");
            }
        }
        "disconnect" => {
            crate::println!("Not connected.");
        }
        "help" | "--help" | "-h" => {
            crate::println!("Usage: wifi [status|scan|connect <ssid>|disconnect|help]");
            crate::println!("  status      Show WiFi adapter and connection status (default)");
            crate::println!("  scan        Scan for nearby wireless networks");
            crate::println!("  connect     Associate with an SSID");
            crate::println!("  disconnect  Disconnect from current network");
        }
        other => {
            crate::println!("wifi: unknown subcommand '{}'. Try 'wifi help'.", other);
        }
    }
}

// ---------------------------------------------------------------------------
// ping
// ---------------------------------------------------------------------------

pub fn cmd_ping(args: &[&str]) {
    let host = match args.first().copied() {
        Some(h) if !h.is_empty() => h,
        _ => {
            crate::println!("Usage: ping <host>");
            return;
        }
    };
    crate::println!("PING {} (network not yet available)", host);
    crate::println!("Note: A live TCP/IP stack requires initialised WiFi hardware.");
    crate::println!("      Use 'net' to view current network state.");
}

// ---------------------------------------------------------------------------
// ifconfig
// ---------------------------------------------------------------------------

pub fn cmd_ifconfig(_args: &[&str]) {
    crate::println!("lo        Link encap:Local Loopback");
    crate::println!("          inet addr:127.0.0.1  Mask:255.0.0.0");
    crate::println!("          UP LOOPBACK RUNNING  MTU:65536  Metric:1");
    crate::println!();
    crate::println!("wlan0     Link encap:Ethernet (802.11)");
    crate::println!("          Status: DOWN");
    crate::println!();
    crate::net::print_status();
}

// ---------------------------------------------------------------------------
// netstat
// ---------------------------------------------------------------------------

pub fn cmd_netstat(_args: &[&str]) {
    crate::println!("Active Internet connections");
    crate::println!("Proto  Local Address          Foreign Address        State");
    crate::println!("(no active connections)");
    crate::println!();
    crate::println!("Active UNIX domain sockets");
    crate::println!("(none)");
}
