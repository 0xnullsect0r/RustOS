# Framebuffer Driver Implementation

## Problem Analysis

### Original Issue
When booting RustOS from USB on real hardware, the system would hang after the bootloader displayed ~20 lines of yellow INFO messages. The screen would show:

```
INFO: UEFI bootloader started
INFO: Using framebuffer at [address]
INFO: Entry point at [address]
INFO: Map framebuffer
INFO: Map physical memory
INFO: Allocate bootinfo
INFO: Create Memory Map
INFO: Jumping to kernel entry point at VirtAddr(0x20b8e0)
```

Then nothing more would appear - the kernel appeared to hang.

### Root Cause

The bootloader successfully wrote to the screen using the **UEFI GOP (Graphics Output Protocol) framebuffer**, proving the framebuffer was working. However, after control transferred to the kernel:

1. **The kernel used VGA text mode** - The existing `src/drivers/vga.rs` wrote directly to the VGA text buffer at hardcoded address `0xb8000`
2. **VGA buffer not available on UEFI** - Modern UEFI systems boot in graphics mode, not VGA text mode
3. **Address 0xb8000 may not be mapped** - The bootloader doesn't identity-map the VGA buffer, causing page faults when accessed
4. **No output appeared** - Without working VGA, the kernel couldn't display anything, making it look like it hung (even if it was running)

### Key Insight

The bootloader's INFO messages proved:
- ✅ The UEFI GOP framebuffer exists and works
- ✅ The memory is accessible and properly mapped
- ✅ Text can be rendered to the screen

The solution: Use the same framebuffer the bootloader used!

## Implementation

### Architecture Overview

```
┌─────────────────────────────────────────┐
│         Kernel Code (println!)          │
└───────────────┬─────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────┐
│    src/drivers/vga.rs::_print()         │
│  (Chooses output method)                │
└──────┬──────────────────────┬───────────┘
       │                      │
       │ Try framebuffer      │ Fall back to VGA
       ▼                      ▼
┌──────────────────┐   ┌─────────────────┐
│  FRAMEBUFFER     │   │  VGA Text Mode  │
│  (UEFI/Modern)   │   │  (BIOS/Legacy)  │
└──────────────────┘   └─────────────────┘
       │                      │
       └──────────┬───────────┘
                  │ Always mirror
                  ▼
         ┌────────────────┐
         │  Serial Port   │
         │  (Debugging)   │
         └────────────────┘
```

### Components

#### 1. Framebuffer Driver (`src/drivers/framebuffer.rs`)

**Key Features:**
- Takes ownership of the bootloader-provided `FrameBuffer` from `BootInfo`
- Supports both RGB and BGR pixel formats
- Implements pixel-level drawing operations
- Software text rendering using an embedded bitmap font
- Handles scrolling, newlines, backspace
- Thread-safe using `spin::Mutex`

**Structure:**
```rust
pub struct FrameBufferWriter {
    framebuffer: &'static mut [u8],  // Raw pixel buffer
    info: FrameBufferInfo,            // Width, height, stride, pixel format
    x_pos: usize,                     // Current character column
    y_pos: usize,                     // Current character row
    foreground: Color,                // Text color (default: yellow)
    background: Color,                // Background color (default: black)
}
```

**Key Methods:**
- `write_pixel(x, y, color)` - Writes a single pixel
- `clear_screen()` - Fills screen with background color
- `draw_char(byte, x, y)` - Renders a character using the font
- `scroll_up()` - Scrolls content up by one line
- `write_byte(byte)` - Handles text output (newline, backspace, printable chars)

#### 2. Font Asset (`assets/font8x16.bin`)

- **Format**: 8x16 PC BIOS-style bitmap font
- **Coverage**: ASCII characters 32-126 (95 characters)
- **Size**: 1520 bytes (95 chars × 16 bytes per char)
- **Rendering**: Each byte represents 8 horizontal pixels in one row

Generated using a Python script that creates simple but readable glyphs for all printable ASCII characters.

#### 3. Output System Integration

**Updated `src/drivers/vga.rs::_print()`:**
```rust
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        // Try framebuffer first (for UEFI systems)
        use crate::drivers::framebuffer::FRAMEBUFFER_WRITER;
        if let Some(fb_writer) = FRAMEBUFFER_WRITER.lock().as_mut() {
            fb_writer.write_fmt(args).unwrap();
        } else {
            // Fall back to VGA text buffer (for BIOS systems)
            WRITER.lock().write_fmt(args).unwrap();
        }
    });
    
    // Always mirror to serial for debugging
    crate::serial_print!("{}", args);
}
```

#### 4. Initialization (`src/main.rs`)

Early in `kernel_main()`, before any `println!` calls:
```rust
// Initialize framebuffer early if available (before println!)
if let Some(framebuffer) = boot_info.framebuffer.take() {
    rustos::serial_println!("[kernel] Framebuffer available, initializing...");
    unsafe {
        rustos::drivers::framebuffer::init(framebuffer);
    }
    rustos::serial_println!("[kernel] Framebuffer initialized successfully");
    // Clear screen and show boot message
    println!("\n=== RustOS Kernel Initializing ===\n");
} else {
    rustos::serial_println!("[kernel] No framebuffer available, using VGA fallback");
}
```

## How It Works

### Boot Sequence

1. **UEFI Firmware** loads the bootloader
2. **Bootloader** (bootloader 0.11):
   - Initializes GOP framebuffer
   - Sets up page tables
   - Maps framebuffer memory
   - Displays INFO messages using the framebuffer
   - Passes `BootInfo` (including framebuffer) to kernel
   - Jumps to kernel entry point
3. **Kernel** (`kernel_main`):
   - Calls `rustos::init()` (GDT, IDT, interrupts)
   - Takes framebuffer from `BootInfo`
   - Initializes `FrameBufferWriter`
   - Now `println!` works on the screen!
   - Continues with memory, heap, VFS, USB initialization
   - Launches the shell

### Text Rendering Process

When `println!("Hello")` is called:

1. **Macro expansion** → `vga::_print(format_args!("Hello\n"))`
2. **Output routing** → Checks if framebuffer is available
3. **Character processing** → For each byte in "Hello\n":
   - For 'H': `draw_char(b'H', x_pos, y_pos)`
     - Lookup glyph in font: `FONT_8X16[(b'H' - 32) * 16..(b'H' - 32 + 1) * 16]`
     - For each of 16 rows in the glyph:
       - Read the byte (8 bits = 8 pixels)
       - For each bit:
         - If bit is 1: draw foreground color pixel
         - If bit is 0: draw background color pixel
   - Increment `x_pos`
   - For '\n': `newline()` - sets `x_pos = 0`, increments `y_pos`, scrolls if needed
4. **Scrolling** (if at bottom of screen):
   - Copy all pixel rows up by `FONT_HEIGHT` (16 pixels)
   - Clear the bottom line
   - Reset `y_pos` to last line

### Pixel Format Handling

The framebuffer supports different pixel formats:
- **RGB**: Red, Green, Blue bytes (in that order)
- **BGR**: Blue, Green, Red bytes (in that order)

The driver detects the format from `FrameBufferInfo.pixel_format` and writes pixels accordingly:
```rust
let color_bytes = match self.info.pixel_format {
    PixelFormat::Rgb => [color.r, color.g, color.b, 0],
    PixelFormat::Bgr => [color.b, color.g, color.r, 0],
    _ => [color.r, color.g, color.b, 0],
};
```

## Benefits

### ✅ Solves the Boot Hang

- The kernel can now output to the screen on UEFI systems
- Boot messages are visible
- Users can see what's happening
- Debugging is much easier

### ✅ Backward Compatible

- Still works on BIOS systems (falls back to VGA)
- No breaking changes to existing code
- Same `println!` macro API

### ✅ Modern and Future-Proof

- Uses standard UEFI GOP framebuffer
- Works on all modern hardware
- Supports high-resolution displays
- Handles different pixel formats

### ✅ Flexible and Extensible

- Easy to add colors (already has Color struct)
- Can implement graphics later (have pixel access)
- Software rendering gives full control
- Font can be swapped or upgraded

## Testing

### In QEMU (UEFI Mode)

```bash
cargo run
```

Expected output on screen:
```
=== RustOS Kernel Initializing ===

[USB initialization messages...]
RustOS v0.1.0 — launching /bin/rsh
rsh>
```

Expected serial output:
```
[kernel] Framebuffer available, initializing...
[kernel] Framebuffer initialized successfully
[init] Launching /bin/rsh...
```

### On Real Hardware (USB Boot)

1. Build release image: `cargo build --release`
2. Create bootable USB: `./write_to_drive.sh --drive /dev/sdX`
3. Boot from USB
4. Should see yellow text on black background
5. Shell prompt should appear

## Performance Considerations

### Scrolling Performance

Scrolling is done by copying all pixels up by 16 rows. For a 1920x1080 framebuffer:
- ~2 million pixels to copy
- 4 bytes per pixel = ~8 MB of data
- This is done in software (CPU)

**Optimization opportunities:**
- Use SIMD/vectorized copy operations
- Implement circular buffer for lines
- Only clear visible regions
- Add dirty region tracking

### Font Rendering

Each character renders 16×8 = 128 pixels individually. This is acceptable for terminal output but not for high-performance graphics.

**Future improvements:**
- Add glyph caching
- Pre-render common character combinations
- Use GPU for rendering (via graphics API)

## Future Enhancements

### Short Term

- [ ] Add more colors and text attributes (bold, italic)
- [ ] Support larger fonts (12x24, 16x32)
- [ ] Implement cursor blinking
- [ ] Add border/padding control

### Medium Term

- [ ] Graphics primitives (lines, rectangles, circles)
- [ ] Image/logo display (PNG/BMP decoder)
- [ ] Multiple virtual consoles (Alt+F1, Alt+F2, etc.)
- [ ] Mouse cursor support

### Long Term

- [ ] Full GUI framework
- [ ] Window manager
- [ ] Hardware-accelerated rendering
- [ ] TrueType/OpenType font support

## Troubleshooting

### Issue: Blank screen after bootloader messages

**Cause**: Framebuffer not initialized or init failed
**Solution**: Check serial output for error messages, verify bootloader provides framebuffer

### Issue: Garbled text or wrong colors

**Cause**: Incorrect pixel format detection
**Solution**: Check `FrameBufferInfo.pixel_format`, test with both RGB and BGR

### Issue: Text cut off or misaligned

**Cause**: Stride calculation error
**Solution**: Use `stride` instead of `width` for line offset calculations

### Issue: Slow scrolling

**Cause**: Software pixel copying is slow
**Solution**: Normal for software rendering, can optimize with SIMD or reduce screen updates

## Technical Details

### Memory Layout

```
Framebuffer Memory:
┌────────────────────────────────────┐ ← buffer[0]
│ Pixel (0,0): [R][G][B][X]          │
│ Pixel (1,0): [R][G][B][X]          │
│ ...                                │
│ Pixel (width-1, 0): [R][G][B][X]   │
│ [padding if stride > width]        │ ← buffer[stride * bytes_per_pixel]
│ Pixel (0,1): [R][G][B][X]          │
│ ...                                │
└────────────────────────────────────┘ ← buffer[stride * height * bytes_per_pixel]
```

### Character Grid Mapping

```
Screen Resolution: 1920×1080 pixels
Character Size: 8×16 pixels
Character Grid: 240 columns × 67 rows

Character (0,0) → Pixels (0,0) to (7,15)
Character (1,0) → Pixels (8,0) to (15,15)
Character (0,1) → Pixels (0,16) to (7,31)
...
```

## References

- [bootloader_api 0.11 Documentation](https://docs.rs/bootloader_api/0.11/)
- [UEFI GOP Specification](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#graphics-output-protocol)
- [Writing an OS in Rust](https://os.phil-opp.com/)
- [VGA Text Mode](https://en.wikipedia.org/wiki/VGA_text_mode)
- [PC Screen Font Format](https://en.wikipedia.org/wiki/PC_Screen_Font)

## Credits

Implementation by GitHub Copilot for RustOS-Dev/RustOS
Based on bootloader_api 0.11 by Philipp Oppermann
