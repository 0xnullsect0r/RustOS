# RustOS Shell Commands Reference

This document provides comprehensive documentation for all shell commands available in RustOS.

## Command Categories

- **File Operations**: `cat`, `ls`, `cp`, `mv`, `rm`, `write`
- **Directory Operations**: `cd`, `pwd`, `mkdir`, `rmdir`
- **Text Processing**: `echo`, `grep`
- **Storage**: `mount`, `umount`, `lsblk`, `usbscan`
- **Hardware**: `lspci`, `lsusb`, `ps`, `meminfo`
- **Networking**: `ping`, `ifconfig`, `netstat`, `wifi`, `net`
- **System**: `uname`, `clear`, `color`, `help`, `reboot`, `shutdown`, `exec`

## File Operations

### echo

Print text to standard output.

**Syntax:**
```
echo [-n] [-e] [text...]
```

**Flags:**
- `-n`: Do not output a trailing newline
- `-e`: Interpret escape sequences (`\n`, `\t`, `\r`, `\\`, etc.)

**Examples:**
```
echo "Hello, RustOS!"
echo -n "No newline"
echo -e "Line1\nLine2"
echo -ne "Text" "More text"
```

**Output Format:**
Text followed by newline (unless `-n` is specified)

**Known Limitations:**
- No support for `echo -E` (explicitly disable escape sequences)

---

### cat

Concatenate and display file contents.

**Syntax:**
```
cat [-n] [-A] file...
```

**Flags:**
- `-n`: Number all output lines
- `-A`: Show all characters (equivalent to `-vET`; shows tabs as `^I`, line endings as `$`)

**Examples:**
```
cat /etc/hosts
cat -n myfile.txt
cat file1.txt file2.txt
cat -A file.txt
```

**Output Format:**
File contents, one file at a time. Directories are rejected.

**Known Limitations:**
- Cannot read from stdin when no files specified (requires explicit file argument)
- Binary files may produce garbled output

---

### ls

List directory contents.

**Syntax:**
```
ls [-alh] [path...]
```

**Flags:**
- `-a`: Show all files (including those starting with `.`)
- `-l`: Long format (permissions, owner, size, date, name)
- `-h`: Human-readable sizes (K, M, G suffixes)

**Examples:**
```
ls
ls -l /
ls -a /boot
ls -lh /mnt
```

**Output Format:**
- Default: One filename per line, sorted
- Long format: `permissions owner size date time name`
  - Example: `-rw-r--r-- root 1024 Jan 1 12:00 myfile.txt`

**Known Limitations:**
- Permissions always show as `drwxr-xr-x` (directories) or `-rw-r--r--` (files)
- No support for `-R` (recursive)

---

### cp

Copy files or directories.

**Syntax:**
```
cp [-r] src... dst
```

**Flags:**
- `-r`: Recursively copy directories

**Examples:**
```
cp file1.txt file2.txt
cp -r dir1 dir2
cp file.txt /mnt/backup/
```

**Output Format:**
Success produces no output. Errors are reported.

**Known Limitations:**
- Does not preserve file timestamps or ownership
- Does not support `--` to end flags

---

### mv

Move or rename files and directories.

**Syntax:**
```
mv [-fn] src... dst
```

**Flags:**
- `-f`: Force (overwrite destination without prompt)
- `-n`: No-clobber (don't overwrite existing files)

**Examples:**
```
mv oldname.txt newname.txt
mv file.txt /mnt/backup/
mv -f file1 file2
```

**Output Format:**
Success produces no output. Errors are reported.

---

### rm

Remove files or directories.

**Syntax:**
```
rm [-rf] path...
```

**Flags:**
- `-r`: Recursively remove directories
- `-f`: Force (ignore nonexistent files, no prompts)

**Examples:**
```
rm file.txt
rm -rf directory/
rm -f missing.txt
```

**Output Format:**
Success produces no output. Errors are reported.

**Known Limitations:**
- Always non-interactive (no confirmation prompts regardless of flags)
- Cannot remove non-empty directories without `-r`

---

### write

Write text to a file.

**Syntax:**
```
write <path> <text>
```

**Parameters:**
- `path`: File path to create or overwrite
- `text`: Content to write (as a single argument; spaces within quotes preserved)

**Examples:**
```
write /tmp/myfile.txt "Hello, World!"
write /mnt/test.log "Log entry"
```

**Output Format:**
Success produces no output. Errors report: `write: <path>: <reason>`

**Known Limitations:**
- Text must be provided as a single argument; cannot read from stdin
- Always overwrites existing files (no append mode)
- Limited to 8.3 DOS filename length on FAT32

---

## Directory Operations

### cd

Change the current working directory.

**Syntax:**
```
cd [path]
```

**Parameters:**
- `path`: Directory to change to (default: `/` if omitted)

**Examples:**
```
cd /mnt
cd ..
cd
```

**Output Format:**
No output on success. Errors report: `cd: <path>: <reason>`

**Known Limitations:**
- No support for `~` (home directory)
- No support for `-` (previous directory)

---

### pwd

Print the current working directory.

**Syntax:**
```
pwd [-LP]
```

**Flags:**
- `-L`: Logical path (default; follows symlinks)
- `-P`: Physical path (resolves symlinks; not supported)

**Examples:**
```
pwd
pwd -L
```

**Output Format:**
The absolute path of the current working directory.

---

### mkdir

Create directories.

**Syntax:**
```
mkdir [-p] path...
```

**Flags:**
- `-p`: Create parent directories as needed

**Examples:**
```
mkdir newdir
mkdir -p /mnt/deep/nested/path
mkdir dir1 dir2 dir3
```

**Output Format:**
Success produces no output. Errors report: `mkdir: <path>: <reason>`

**Known Limitations:**
- Permissions always set to `drwxr-xr-x`
- No support for `-m` (explicit permissions)

---

## Text Processing

### grep

Search for patterns in files.

**Syntax:**
```
grep [-invrcl] pattern file...
```

**Flags:**
- `-i`: Ignore case
- `-n`: Print line numbers
- `-v`: Invert match (show non-matching lines)
- `-r`: Recursively search directories
- `-c`: Count matching lines only
- `-l`: List files with matches only

**Examples:**
```
grep "error" logfile.txt
grep -i "warning" /var/log/*
grep -n "TODO" code.rs
grep -v "^#" config.txt
grep -l "pattern" *.txt
```

**Output Format:**
- Default: `filename:line_number:matched_line` (if multiple files)
- With `-c`: `filename:count`
- With `-l`: `filename`

**Known Limitations:**
- No support for regex, only literal string matching
- No support for `-E` (extended regex) or `-P` (Perl regex)
- Pattern must be the first argument after flags

---

## Storage Operations

### mount

Show and manage filesystem mounts.

**Syntax:**
```
mount [-t type] [device] [directory]
```

**Flags:**
- `-t`: Filesystem type (e.g., `fat32`, `ramfs`)

**Examples:**
```
mount
mount -t fat32 /dev/sda2 /mnt
mount /dev/sda2 /media
```

**Output Format:**
- No arguments: List all mounts in format `device on directory type (flags)`
- Success: No output; mount added to VFS
- Errors: `mount: <path>: <reason>`

**Known Limitations:**
- Only FAT32 and RamFS supported
- Device names are examples; actual device detection depends on USB enumeration

---

### umount

Unmount a filesystem.

**Syntax:**
```
umount <path>
```

**Parameters:**
- `path`: Mount point or device to unmount

**Examples:**
```
umount /mnt
umount /media
```

**Output Format:**
Success produces no output. Errors report: `umount: <path>: <reason>`

**Known Limitations:**
- Cannot unmount while files are in use

---

### lsblk

List block devices.

**Syntax:**
```
lsblk [-f] [-l] [-o cols] [device]
```

**Flags:**
- `-f`: Show filesystems
- `-l`: List format (not tree)
- `-o`: Specify columns (e.g., `NAME,SIZE,TYPE`)

**Examples:**
```
lsblk
lsblk -f
lsblk -l /dev/sda
lsblk -o NAME,SIZE,TYPE
```

**Output Format:**
- Tree format (default): Indented tree of devices and partitions
- List format (`-l`): One device per line with columns

**Known Limitations:**
- Limited column support
- Partition detection depends on USB enumeration at boot

---

### usbscan

Scan for and enumerate USB devices.

**Syntax:**
```
usbscan
```

**Examples:**
```
usbscan
```

**Output Format:**
List of detected USB devices with vendor/product IDs and descriptions.

**Known Limitations:**
- Limited to USB mass storage devices
- Does not hot-plug devices; must be inserted before boot or scan

---

## Hardware Information

### lspci

List PCI devices.

**Syntax:**
```
lspci [-v] [-n] [-nn]
```

**Flags:**
- `-v`: Verbose (show additional information)
- `-n`: Numeric output (show IDs instead of names)
- `-nn`: Both numeric and names

**Examples:**
```
lspci
lspci -v
lspci -n
```

**Output Format:**
One device per line: `bus:device.function vendor:device <description>`

---

### lsusb

List USB devices.

**Syntax:**
```
lsusb [-v] [-t]
```

**Flags:**
- `-v`: Verbose
- `-t`: Tree format

**Examples:**
```
lsusb
lsusb -v
lsusb -t
```

**Output Format:**
- Default: One device per line
- Tree format: Hierarchical view

---

### ps

List running processes.

**Syntax:**
```
ps [aux]
```

**Parameters:**
- `aux`: (Optional) Show all processes with extended details

**Examples:**
```
ps
ps aux
```

**Output Format:**
- Default: `PID COMMAND`
- With `aux`: `USER PID PPID VSIZ RSS %MEM %CPU STAT START TIME COMMAND`

---

### meminfo

Display memory usage information.

**Syntax:**
```
meminfo
```

**Examples:**
```
meminfo
```

**Output Format:**
```
Heap memory info:
  Allocated: X bytes
  Total size: Y bytes
  Free: Z bytes
```

---

### uname

Print system information.

**Syntax:**
```
uname [-asnrvmpio]
```

**Flags:**
- `-a`: All information
- `-s`: Kernel name (default: `RustOS`)
- `-n`: Node name (`rustos`)
- `-r`: Release/version
- `-v`: Version information
- `-m`: Machine hardware name (`x86_64`)
- `-p`: Processor type
- `-i`: Hardware platform
- `-o`: Operating system name

**Examples:**
```
uname
uname -a
uname -m
```

**Output Format:**
Selected information, space-separated. Default (no flags) returns just kernel name.

---

## Networking

### ping

Test network connectivity.

**Syntax:**
```
ping <host>
```

**Parameters:**
- `host`: IP address or hostname to ping

**Examples:**
```
ping 8.8.8.8
ping 192.168.1.1
ping google.com
```

**Output Format:**
```
PING <host> with X bytes of data:
reply from <host>: bytes=X time=Y ms TTL=Z
...
```

**Known Limitations:**
- Limited to IPv4
- No support for options like `-c` (count) or `-t` (timeout)

---

### ifconfig

Show network interface configuration.

**Syntax:**
```
ifconfig
```

**Examples:**
```
ifconfig
```

**Output Format:**
```
wlan0: flags=<flags> mtu=1500
    inet <ip_address> netmask <netmask> broadcast <broadcast>
    hwaddr <mac_address>
    RX packets:X errors:Y dropped:Z
    TX packets:X errors:Y dropped:Z
```

**Known Limitations:**
- Read-only; cannot configure interfaces
- Limited to configured network adapters

---

### netstat

Show network statistics and active connections.

**Syntax:**
```
netstat
```

**Examples:**
```
netstat
```

**Output Format:**
```
Active Internet connections (tcp)
Proto Local Address    Foreign Address    State
tcp   <local>:<port>  <remote>:<port>   ESTABLISHED
...
```

---

### wifi

WiFi control and status.

**Syntax:**
```
wifi [status|scan|connect <ssid>]
```

**Subcommands:**
- `status`: Show WiFi status (no args)
- `scan`: Scan for available networks
- `connect <ssid>`: Connect to a network

**Examples:**
```
wifi status
wifi scan
wifi connect "MyNetwork"
```

**Output Format:**
- `status`: Device status, connected SSID, signal strength
- `scan`: List of networks with SSID and signal strength
- `connect`: Connection progress/result

**Known Limitations:**
- No WEP/WPA password handling in shell (requires pre-configuration)
- Limited to 802.11n/ax networks

---

### net

Show network stack status.

**Syntax:**
```
net
```

**Examples:**
```
net
```

**Output Format:**
Network stack status, driver information, connection state.

---

## System Control

### clear

Clear the screen.

**Syntax:**
```
clear
```

**Examples:**
```
clear
```

**Output Format:**
Clears framebuffer and resets cursor.

---

### color

Set terminal foreground and background colors.

**Syntax:**
```
color <foreground> <background>
```

**Parameters:**
- `foreground`: Color name (e.g., `red`, `green`, `white`)
- `background`: Color name

**Examples:**
```
color white black
color green black
color yellow red
```

**Supported Colors:**
`black`, `blue`, `green`, `cyan`, `red`, `magenta`, `brown`, `lightgray`, `darkgray`, `lightblue`, `lightgreen`, `lightcyan`, `lightred`, `lightmagenta`, `yellow`, `white`

---

### exec

Execute an ELF binary.

**Syntax:**
```
exec <path>
```

**Parameters:**
- `path`: Path to ELF executable

**Examples:**
```
exec /usr/bin/hello
exec /app/myprogram
```

**Output Format:**
Program output, followed by exit status: `[process exited with code X]`

**Known Limitations:**
- Cannot execute shell scripts
- Limited userspace integration (no environment variables, stdin/stdout)

---

### reboot

Reboot the system.

**Syntax:**
```
reboot
```

**Examples:**
```
reboot
```

**Output Format:**
System restarts immediately.

---

### shutdown

Power off the system.

**Syntax:**
```
shutdown
```

**Examples:**
```
shutdown
```

**Output Format:**
System powers off immediately.

---

## /bin Commands

Commands can also be invoked via `/bin/<command>` path format when used with the `exec` syscall or from userspace programs.

### Supported /bin Commands

```
/bin/echo
/bin/cat
/bin/ls
/bin/cd
/bin/mkdir
/bin/rm
/bin/cp
/bin/mv
/bin/write
/bin/grep
/bin/mount
/bin/umount
/bin/lspci
/bin/lsusb
/bin/lsblk
/bin/ping
/bin/wifi
/bin/ifconfig
/bin/netstat
/bin/ps
/bin/uname
/bin/help
/bin/clear
/bin/usbscan
/bin/reboot
/bin/shutdown
/bin/net
/bin/exec
/bin/meminfo
/bin/color
/bin/pwd
```

---

## Important Limitations

### No Pipe Support

Commands cannot be chained with pipes:
```
cat file.txt | grep pattern  # ✗ NOT SUPPORTED
```

**Workaround:** Use separate commands and temporary files:
```
cat file.txt > /tmp/output.txt
grep pattern /tmp/output.txt
```

### No Output Redirection

Standard output redirection is not supported:
```
echo "text" > file.txt  # ✗ NOT SUPPORTED
```

**Workaround:** Use the `write` command:
```
write file.txt "text"
```

### No Command Substitution

Command substitution is not supported:
```
echo $(whoami)  # ✗ NOT SUPPORTED
```

**Workaround:** Use separate commands and variable passing is not available.

### Limited Argument Expansion

- No glob expansion (`*`, `?`, `[...]`)
- No tilde expansion (`~`)
- No variable expansion (`$VAR`)
- Only explicit arguments supported

### No Job Control

- No background processes (`&`)
- No job listing/management (`jobs`, `fg`, `bg`)
- All commands run synchronously

---

## Exit Codes

Commands follow standard Unix conventions:
- `0`: Success
- `1`: General error
- `2`: Misuse (wrong arguments, flags)
- `127`: Command not found
- `255`: Fatal error

---

## File Naming Conventions

**FAT32 Limitations:**
- 8.3 DOS names only (8 characters + 3-character extension)
- Uppercase automatically converted
- Special characters not supported: `: * ? " < > |`

**RamFS:**
- Supports longer names (up to 255 characters)
- Case-sensitive names

---

## Performance Notes

- **I/O Intensive**: File operations on USB storage may be slow
- **Memory**: Stack-based buffers limited to 4 KiB for file I/O
- **Large Files**: Reading/writing 10+ MB files may consume significant memory

---

## Related Documentation

- [SYSCALLS.md](SYSCALLS.md) - System call reference
- [LIMITATIONS.md](LIMITATIONS.md) - Known limitations and workarounds
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - Common issues and solutions
