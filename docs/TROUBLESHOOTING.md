# RustOS Troubleshooting Guide

This guide provides solutions for common issues encountered when using RustOS.

## Booting and Installation

### Issue: RustOS fails to boot from USB

**Symptoms:**
- Computer boots to BIOS/UEFI menu instead of RustOS
- USB device not recognized
- "Boot failed" message

**Solutions:**

1. **Verify USB is in boot order:**
   - Enter BIOS/UEFI setup (usually F2, F10, or DEL during boot)
   - Navigate to Boot Order or Boot Priority
   - Move USB device to first position
   - Save and exit

2. **Check USB drive format:**
   ```bash
   # Verify the bootloader was written correctly
   lsblk
   sudo hexdump -C /dev/sdX | head -20
   # Should show "EB 3C" at offset 0 (UEFI boot code)
   ```

3. **Re-write bootloader:**
   ```bash
   # Use the write_to_drive.sh script
   ./write_to_drive.sh --drive /dev/sdX
   
   # Or manually with dd:
   sudo dd if=target/x86_64-rustos/debug/bootimage-rustos.bin \
       of=/dev/sdX bs=4M status=progress
   sudo sync
   ```

4. **Try BIOS mode instead of UEFI:**
   - Some systems prefer BIOS boot
   - Disable "Secure Boot" and "Fast Boot"
   - Select "Legacy BIOS Boot" in UEFI menu

5. **Check USB power:**
   - Some USB devices may require more power
   - Try a USB 2.0 port or external USB hub
   - Use a different USB drive

### Issue: Partitions not recognized

**Symptoms:**
- Bootable partition exists but `lsblk` doesn't show secondary partition
- `mount` command fails to find `/dev/sda2`

**Solutions:**

1. **Create FAT32 partition on second USB:**
   ```bash
   # Use the partition creation script
   ./add-fat32-partition.sh --drive /dev/sdX
   
   # Or manually:
   sudo fdisk /dev/sdX
   # Create new partition (n), make bootable (a), write (w)
   
   sudo mkfs.fat -F 32 /dev/sdX2
   ```

2. **Check partition table:**
   ```bash
   sudo fdisk -l /dev/sdX
   # Should show at least 2 partitions
   ```

3. **Mount the partition manually:**
   ```bash
   # Boot RustOS, then at the shell:
   mount /dev/sda2 /mnt
   ls /mnt
   ```

---

## USB Device Detection

### Issue: USB drives not detected with `lsblk`

**Symptoms:**
- `lsblk` shows no USB devices
- `usbscan` command returns no results
- Cannot access files on inserted USB drive

**Solutions:**

1. **Insert USB before boot:**
   - RustOS only enumerates USB devices at boot
   - Hot-plugging is not yet supported
   - Always insert USB drives before powering on

2. **Use `usbscan` command:**
   ```bash
   usbscan
   # Lists detected USB devices with vendor/product IDs
   ```

3. **Check XHCI controller:**
   ```bash
   lspci | grep -i "usb"
   # Should show "USB controller" entries
   ```

4. **Verify FAT32 partition:**
   - Use `lsblk -f` to see filesystem types
   - Partition must be FAT32 (Type 0c or 0e)
   - RamFS or ext4 partitions won't be detected

5. **Try different USB ports:**
   - Some ports may share controllers
   - If one port fails, try others
   - Avoid USB hubs if possible

### Issue: `mount` command fails with "Invalid filesystem"

**Symptoms:**
- `mount /dev/sda2 /mnt` returns error
- Error: "Invalid filesystem" or "No space left on device"

**Solutions:**

1. **Check filesystem type:**
   ```bash
   lsblk -f
   # Partition should show "vfat" or "fat32"
   ```

2. **Verify FAT32 is properly formatted:**
   ```bash
   # From Linux host:
   sudo mkfs.fat -F 32 /dev/sdX2
   # Then reboot and try mounting again
   ```

3. **Check disk space:**
   ```bash
   # On Linux host:
   sudo fdisk -l /dev/sdX
   # Partition size should be > 0 MB
   ```

4. **Try mounting to different path:**
   ```bash
   # Some paths may have restrictions
   mkdir -p /media
   mount /dev/sda2 /media
   ```

---

## File Operations

### Issue: Cannot write to mounted FAT32 partition

**Symptoms:**
- File creation succeeds but files don't persist
- Files appear empty after reread
- `write` command shows success but no output

**Solutions:**

1. **Check available disk space:**
   ```bash
   meminfo  # Shows heap memory available
   # Large files may fail silently due to memory constraints
   ```

2. **Use shorter filenames:**
   ```bash
   # FAT32 limited to 8.3 DOS names
   write /mnt/test.txt "data"     # ✓ Works
   write /mnt/verylongfile.txt    # ✗ May fail
   ```

3. **Ensure mount point exists:**
   ```bash
   mkdir -p /mnt
   mount /dev/sda2 /mnt
   ls /mnt  # Should show existing files
   ```

4. **Try RamFS for testing:**
   ```bash
   # RamFS is more stable for initial testing
   write /test.txt "Hello"
   cat /test.txt
   ```

### Issue: `grep` not finding patterns

**Symptoms:**
- `grep pattern file.txt` returns nothing
- Pattern is definitely in the file
- Same command works in Linux

**Solutions:**

1. **Note RustOS grep limitations:**
   - Only literal string matching (no regex)
   - Case-sensitive by default
   - Use `-i` flag for case-insensitive search

2. **Use correct flag syntax:**
   ```bash
   # Correct:
   grep -i "PATTERN" file.txt
   
   # Wrong:
   grep "PATTERN" file.txt  # Case-sensitive!
   ```

3. **Check file content first:**
   ```bash
   cat file.txt  # Verify pattern is actually there
   grep "substring" file.txt
   ```

4. **Use exact substring:**
   ```bash
   # Pattern must be exact substring
   grep "test" file.txt  # OK
   grep "test.*pattern"  # ✗ Regex not supported
   ```

---

## Networking Issues

### Issue: Network device not detected

**Symptoms:**
- `net` command shows "no AX210-family WiFi device detected"
- Cannot ping or connect to network
- No `wlan0` interface

**Solutions:**

1. **Verify hardware:**
   ```bash
   lspci | grep -i "network\|wireless\|intel"
   # Look for Intel AX210 or similar adapter
   ```

2. **Check network stack status:**
   ```bash
   net
   # Should show "wlan0: <status>"
   ```

3. **For QEMU users:**
   - QEMU doesn't emulate wireless devices by default
   - Use TCP/IP stack simulation for testing
   - Real hardware testing required for WiFi

4. **Verify PCI enumeration:**
   ```bash
   lspci -v
   # Network device should be listed
   ```

### Issue: Ping fails with "Host unreachable"

**Symptoms:**
- `ping 8.8.8.8` shows "no route to host" or timeout
- Cannot reach any external servers
- `ifconfig` shows no IP address

**Solutions:**

1. **Check network connection:**
   ```bash
   ifconfig
   # Should show `inet <ip_address>` assigned
   ```

2. **Verify DHCP assignment:**
   ```bash
   wifi status
   # Should show "connected to <SSID>"
   # IP address should be shown
   ```

3. **Connect to WiFi first:**
   ```bash
   wifi scan
   # List available networks
   
   wifi connect "NetworkName"
   # Wait for connection
   ```

4. **Check gateway:**
   ```bash
   ifconfig
   # Note the gateway/netmask
   # Try pinging gateway first: ping 192.168.1.1
   ```

5. **For QEMU/Virtual machines:**
   - Network emulation may be limited
   - Loopback (localhost) usually works
   - External connectivity requires network bridge

### Issue: WiFi connection fails

**Symptoms:**
- `wifi connect "SSID"` fails or times out
- Error message or no response
- Network not showing in `wifi scan`

**Solutions:**

1. **Verify network is visible:**
   ```bash
   wifi scan
   # Look for your SSID in the list
   # Check signal strength
   ```

2. **Try connecting to open network first:**
   - Open networks are easier for initial testing
   - WPA2/WPA3 may require additional setup
   - Note: WiFi password handling not fully exposed in shell

3. **Check device status:**
   ```bash
   net
   # Should show device is present and active
   ```

4. **Restart network stack:**
   ```bash
   # Reboot to reinitialize network
   reboot
   ```

5. **Signal strength:**
   ```bash
   wifi scan
   # If signal < -80 dBm, move closer to router
   # Obstacles (walls, metal) reduce signal
   ```

### Issue: `ifconfig` shows no IP address

**Symptoms:**
- `ifconfig` shows `inet 0.0.0.0`
- Cannot reach any hosts
- DHCP appears to have failed

**Solutions:**

1. **Check DHCP server:**
   - Router must have DHCP enabled
   - Check router admin interface
   - DHCP pool may be exhausted

2. **Manually assign IP (if needed):**
   - Current RustOS doesn't support manual IP configuration
   - Workaround: Wait for DHCP timeout and retry
   - Or connect to different network

3. **Check WiFi connection:**
   ```bash
   wifi status
   # Must be "connected" before DHCP
   ```

4. **Wait for DHCP:**
   - DHCP negotiation takes several seconds
   - Give the system time after connecting
   - Try again after waiting

---

## Performance and Resource Issues

### Issue: System appears to hang

**Symptoms:**
- Shell becomes unresponsive
- Commands don't execute
- Keyboard input has no effect

**Solutions:**

1. **Large file operations timeout:**
   - Reading/writing 100+ MB may take extended time
   - Be patient (can be minutes on USB 2.0)
   - No progress indicator shown

2. **Memory exhaustion:**
   ```bash
   meminfo
   # Check available heap memory
   # Large operations may allocate all heap
   ```

3. **Invalid filesystem operation:**
   - Trying to read corrupted FAT32 may cause lag
   - Reformat partition if suspected
   - Use `fsck.fat` from host OS

4. **USB device issues:**
   - Slow USB 2.0 devices very slow
   - Try faster USB 3.0 device
   - Check USB cable quality

5. **Interrupt handling:**
   - If hang persists, may be hardware issue
   - Last resort: Reboot by power cycling

### Issue: Memory usage increases over time

**Symptoms:**
- `meminfo` shows decreasing free memory
- System eventually runs out of memory
- Commands fail with allocation errors

**Solutions:**

1. **Check for leaks:**
   ```bash
   meminfo
   # Allocated should remain relatively stable
   ```

2. **Restart shell for clean state:**
   ```bash
   reboot
   # Clears memory after each boot
   ```

3. **Avoid large operations:**
   - Very large files (>10 MB) consume heap
   - Multiple operations sequentially safer than simultaneous

---

## Display Issues

### Issue: Garbled or distorted output

**Symptoms:**
- Text appears corrupted or misaligned
- Colors wrong or background filled
- Framebuffer not clearing properly

**Solutions:**

1. **Clear screen:**
   ```bash
   clear
   # Resets framebuffer and cursor
   ```

2. **Reset colors:**
   ```bash
   color white black
   # Sets default colors
   ```

3. **Reboot:**
   ```bash
   reboot
   # Reinitializes display subsystem
   ```

4. **Check terminal resolution:**
   - UEFI GOP framebuffer resolution set at boot
   - Changing monitor may require BIOS reset
   - Try different HDMI/DP ports

### Issue: No output to screen but serial works

**Symptoms:**
- Framebuffer shows nothing
- Serial output (serial_println!) works
- System otherwise responsive

**Solutions:**

1. **UEFI GOP framebuffer not initialized:**
   - May not be available in BIOS mode
   - Try UEFI boot mode in BIOS

2. **Monitor not connected:**
   - Output goes to framebuffer but no display
   - Check HDMI cable and connections
   - Verify monitor is powered on

3. **Wrong resolution:**
   - Framebuffer size may not match monitor
   - BIOS may set odd resolutions
   - Disable graphics and use serial console

---

## Build and Development Issues

### Issue: Cannot build RustOS

**Symptoms:**
- Compilation fails with errors
- Missing dependencies
- `cargo build` fails

**Solutions:**

1. **Install Rust nightly:**
   ```bash
   rustup toolchain install nightly
   rustup component add rust-src llvm-tools-preview --toolchain nightly
   rustup default nightly
   ```

2. **Install bootimage:**
   ```bash
   cargo install bootimage
   ```

3. **Clone submodules:**
   ```bash
   git clone --recurse-submodules https://github.com/RustOS-Dev/RustOS.git
   cd RustOS
   ```

4. **Check LLVM tools:**
   ```bash
   rustc --version  # Should be nightly
   rustup component list | grep llvm-tools
   ```

### Issue: QEMU tests fail

**Symptoms:**
- `cargo test` fails
- QEMU not found or crashes
- Test timeout

**Solutions:**

1. **Install QEMU:**
   ```bash
   # Ubuntu/Debian:
   sudo apt install qemu-system-x86 ovmf
   
   # Fedora:
   sudo dnf install qemu-system-x86
   
   # macOS:
   brew install qemu
   ```

2. **Install OVMF firmware:**
   ```bash
   sudo apt install ovmf
   # Needed for UEFI boot testing
   ```

3. **Run specific test:**
   ```bash
   cargo test --test basic_boot
   ```

4. **Increase timeout:**
   - Some tests need more time
   - Check test configuration in `Cargo.toml`

---

## Common Error Messages

| Error | Cause | Fix |
|-------|-------|-----|
| "command not found" | Command typo or not built-in | Check spelling, use `help` |
| "file not found" | File doesn't exist | Use `ls` to list files |
| "permission denied" | Trying to write to read-only FS | Check mount point |
| "not a directory" | Using file as directory | Use correct path |
| "directory not empty" | Trying to remove non-empty dir | Use `rm -rf` |
| "no such device" | Device not mounted | Use `mount` first |
| "no space left on device" | Disk/memory full | Delete files or reboot |
| "invalid filesystem" | Wrong partition type | Use FAT32 formatted partition |

---

## Getting Help

1. **Check documentation:**
   - [SHELL_COMMANDS.md](SHELL_COMMANDS.md) - Command reference
   - [SYSCALLS.md](SYSCALLS.md) - System call reference
   - [LIMITATIONS.md](LIMITATIONS.md) - Known limitations

2. **View kernel logs:**
   - Serial output on COM1
   - QEMU window may show additional info
   - Use `dmesg` equivalent (not available in RustOS)

3. **Report issues:**
   - [GitHub Issues](https://github.com/RustOS-Dev/RustOS/issues)
   - Include kernel version, hardware, and reproduction steps
   - Attach screenshots of error messages

4. **Community:**
   - Discussions on GitHub
   - Related projects: Philipp Oppermann's tutorial, linux-kernel
