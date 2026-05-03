use alloc::collections::VecDeque;
use alloc::vec::Vec;
use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;

/// 8x16 PC BIOS font data - Basic ASCII characters (32-126)
/// Each character is 16 bytes (8 pixels wide, 16 pixels tall)
/// Bit 1 = foreground, Bit 0 = background
const FONT_8X16: &[u8] = include_bytes!("../../assets/font8x16.bin");

const FONT_WIDTH: usize = 8;
const FONT_HEIGHT: usize = 16;
const CHARS_IN_FONT: usize = 95; // ASCII 32-126

/// Maximum number of committed lines kept in the scrollback history.
const SCROLLBACK_MAX: usize = 200;

lazy_static! {
    pub static ref FRAMEBUFFER_WRITER: Mutex<Option<FrameBufferWriter>> = Mutex::new(None);
}

/// A writer type that allows writing text to the framebuffer.
pub struct FrameBufferWriter {
    framebuffer: &'static mut [u8],
    info: FrameBufferInfo,
    x_pos: usize,
    y_pos: usize,
    foreground: Color,
    background: Color,

    // ── scrollback (heap-backed; None until enable_scrollback() is called) ──
    /// Completed lines, oldest at the front.
    scrollback: Option<VecDeque<Vec<u8>>>,
    /// Characters on the line currently being written (not yet committed).
    current_line: Vec<u8>,
    /// 0 = live view.  N > 0 = screen shows content N lines above live.
    scroll_offset: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
    };
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
    pub const YELLOW: Color = Color {
        r: 255,
        g: 255,
        b: 0,
    };
    pub const RED: Color = Color { r: 255, g: 0, b: 0 };
    pub const GREEN: Color = Color { r: 0, g: 255, b: 0 };
    pub const BLUE: Color = Color { r: 0, g: 0, b: 255 };
    pub const CYAN: Color = Color {
        r: 0,
        g: 255,
        b: 255,
    };
    pub const MAGENTA: Color = Color {
        r: 255,
        g: 0,
        b: 255,
    };
}

impl FrameBufferWriter {
    /// Creates a new framebuffer writer from the bootloader-provided framebuffer.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the framebuffer is valid and that this is the only
    /// reference to it.
    pub unsafe fn new(framebuffer: FrameBuffer) -> Self {
        let info = framebuffer.info();
        let buffer = framebuffer.into_buffer();

        Self {
            framebuffer: buffer,
            info,
            x_pos: 0,
            y_pos: 0,
            foreground: Color::YELLOW,
            background: Color::BLACK,
            scrollback: None,
            current_line: Vec::new(),
            scroll_offset: 0,
        }
    }

    /// Enables the heap-backed scrollback buffer.  Must be called after the
    /// global allocator is initialised (i.e. after `allocator::init_heap`).
    pub fn enable_scrollback(&mut self) {
        if self.scrollback.is_none() {
            self.scrollback = Some(VecDeque::new());
        }
    }

    /// Scroll the *view* up by one text row (shows older content).
    pub fn scroll_view_up(&mut self) {
        let sb_len = match &self.scrollback {
            Some(sb) => sb.len(),
            None => return,
        };
        let lines_per_screen = self.lines_per_screen();
        // total virtual lines = committed + current_line
        let total = sb_len + 1;
        let max_offset = total.saturating_sub(lines_per_screen);
        if max_offset == 0 {
            return;
        }
        if self.scroll_offset < max_offset {
            self.scroll_offset += 1;
            self.redraw_current_view();
        }
    }

    /// Scroll the *view* down by one text row (shows newer content).
    pub fn scroll_view_down(&mut self) {
        if self.scroll_offset == 0 {
            return;
        }
        self.scroll_offset -= 1;
        self.redraw_current_view();
    }

    /// Repaint the framebuffer from the scrollback buffer for the current
    /// `scroll_offset`.  When `scroll_offset == 0` the live view is restored.
    fn redraw_current_view(&mut self) {
        let lines_per_screen = self.lines_per_screen();
        let chars_per_line = self.chars_per_line();

        let sb_len = match &self.scrollback {
            Some(sb) => sb.len(),
            None => return,
        };
        let total = sb_len + 1; // +1 for current_line

        // Which virtual lines to show:
        //   last_excl = first index NOT shown
        //   first     = first index shown
        let last_excl = total.saturating_sub(self.scroll_offset);
        let first = last_excl.saturating_sub(lines_per_screen);

        // Clone the visible slice to release the borrow on `self.scrollback`
        // before we call draw_char (which needs `&mut self`).
        let mut display: Vec<Vec<u8>> = Vec::with_capacity(lines_per_screen);
        {
            let sb = match &self.scrollback {
                Some(sb) => sb,
                None => return,
            };
            for i in first..last_excl {
                if i < sb_len {
                    display.push(sb[i].clone());
                } else {
                    // i == sb_len → the in-progress current_line
                    display.push(self.current_line.clone());
                }
            }
        }

        // Fast pixel clear then re-render
        self.fill_background();
        for (row, line) in display.iter().enumerate() {
            for (col, &byte) in line.iter().enumerate() {
                if col >= chars_per_line {
                    break;
                }
                self.draw_char(byte, col, row);
            }
        }
    }

    /// Sets the foreground and background colors.
    pub fn set_colors(&mut self, foreground: Color, background: Color) {
        self.foreground = foreground;
        self.background = background;
    }

    // ── pixel helpers ──────────────────────────────────────────────────────

    /// Encode `color` into the framebuffer's byte order.
    fn color_to_bytes(&self, color: Color) -> [u8; 4] {
        match self.info.pixel_format {
            PixelFormat::Rgb => [color.r, color.g, color.b, 0],
            PixelFormat::Bgr => [color.b, color.g, color.r, 0],
            _ => [color.r, color.g, color.b, 0],
        }
    }

    /// Fill the entire framebuffer with the background colour in one pass.
    fn fill_background(&mut self) {
        let bpp = self.info.bytes_per_pixel;
        let bg_bytes = self.color_to_bytes(self.background);
        for chunk in self.framebuffer.chunks_mut(bpp) {
            chunk.copy_from_slice(&bg_bytes[..chunk.len()]);
        }
    }

    /// Clears the entire screen with the background color.
    pub fn clear_screen(&mut self) {
        self.fill_background();
        self.x_pos = 0;
        self.y_pos = 0;
    }

    /// Calculates the maximum number of characters that fit on one line.
    fn chars_per_line(&self) -> usize {
        self.info.width / FONT_WIDTH
    }

    /// Calculates the maximum number of lines that fit on screen.
    fn lines_per_screen(&self) -> usize {
        self.info.height / FONT_HEIGHT
    }

    /// Scrolls the screen up by one text row using a single bulk copy.
    fn scroll_up_pixels(&mut self) {
        let bpp = self.info.bytes_per_pixel;
        let row_stride = self.info.stride * bpp;
        let scroll_bytes = FONT_HEIGHT * row_stride;
        let fb_len = self.framebuffer.len();

        if scroll_bytes >= fb_len {
            return;
        }

        // Shift everything up by FONT_HEIGHT pixel rows in one memmove.
        self.framebuffer.copy_within(scroll_bytes..fb_len, 0);

        // Blank the last FONT_HEIGHT rows.
        let clear_start = fb_len.saturating_sub(scroll_bytes);
        let bg_bytes = self.color_to_bytes(self.background);
        for chunk in self.framebuffer[clear_start..].chunks_mut(bpp) {
            chunk.copy_from_slice(&bg_bytes[..chunk.len()]);
        }
    }

    /// Moves to a new line, scrolling if necessary.
    fn newline(&mut self) {
        // Commit the in-progress line to scrollback.
        if let Some(ref mut sb) = self.scrollback {
            let line = core::mem::take(&mut self.current_line);
            if sb.len() >= SCROLLBACK_MAX {
                sb.pop_front();
            }
            sb.push_back(line);
        }

        self.x_pos = 0;
        self.y_pos += 1;

        if self.y_pos >= self.lines_per_screen() {
            self.scroll_up_pixels();
            self.y_pos = self.lines_per_screen() - 1;
        }
    }

    /// Handles backspace by moving cursor back and clearing the character.
    fn backspace(&mut self) {
        if self.x_pos > 0 {
            self.x_pos -= 1;
            // Pop from current_line too, if tracking.
            if self.scrollback.is_some() {
                self.current_line.pop();
            }
            // Clear the character at the current position
            self.draw_char(b' ', self.x_pos, self.y_pos);
        }
    }

    /// Draws a single character at the specified character position.
    fn draw_char(&mut self, byte: u8, char_x: usize, char_y: usize) {
        // Only support printable ASCII for now
        if !(32..=126).contains(&byte) {
            return;
        }

        let glyph_index = (byte - 32) as usize;
        if glyph_index >= CHARS_IN_FONT {
            return;
        }

        // Each character is 16 bytes in the font
        let glyph_offset = glyph_index * FONT_HEIGHT;

        let pixel_x = char_x * FONT_WIDTH;
        let pixel_y = char_y * FONT_HEIGHT;

        for row in 0..FONT_HEIGHT {
            if glyph_offset + row >= FONT_8X16.len() {
                break;
            }

            let glyph_row = FONT_8X16[glyph_offset + row];

            for col in 0..FONT_WIDTH {
                let bit = (glyph_row >> (7 - col)) & 1;
                let color = if bit == 1 {
                    self.foreground
                } else {
                    self.background
                };
                self.write_pixel(pixel_x + col, pixel_y + row, color);
            }
        }
    }

    /// Writes a byte to the framebuffer.
    pub fn write_byte(&mut self, byte: u8) {
        // Any output jumps back to the live view so the user sees new content.
        if self.scroll_offset > 0 {
            self.scroll_offset = 0;
            self.redraw_current_view();
        }

        match byte {
            b'\n' => self.newline(),
            0x08 => self.backspace(), // Backspace
            byte => {
                if self.x_pos >= self.chars_per_line() {
                    self.newline();
                }

                self.draw_char(byte, self.x_pos, self.y_pos);
                if self.scrollback.is_some() {
                    self.current_line.push(byte);
                }
                self.x_pos += 1;
            }
        }
    }

    /// Writes a string to the framebuffer.
    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x08 | 0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0xfe), // Unprintable character
            }
        }
    }
}

impl fmt::Write for FrameBufferWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

/// Prints the given formatted string to the framebuffer.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        if let Some(writer) = FRAMEBUFFER_WRITER.lock().as_mut() {
            writer.write_fmt(args).unwrap();
        }
    });
}

/// Initialize the framebuffer writer from boot info.
///
/// # Safety
///
/// This must be called exactly once during kernel initialization.
pub unsafe fn init(framebuffer: FrameBuffer) {
    unsafe {
        let mut writer = FrameBufferWriter::new(framebuffer);
        writer.clear_screen();
        *FRAMEBUFFER_WRITER.lock() = Some(writer);
    }
}

/// Enable the heap-backed scrollback buffer.  Must be called after the
/// global allocator is ready.
pub fn enable_scrollback() {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        if let Some(writer) = FRAMEBUFFER_WRITER.lock().as_mut() {
            writer.enable_scrollback();
        }
    });
}

/// Scroll the terminal view up by one text row (Arrow Up).
pub fn scroll_view_up() {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        if let Some(writer) = FRAMEBUFFER_WRITER.lock().as_mut() {
            writer.scroll_view_up();
        }
    });
}

/// Scroll the terminal view down by one text row (Arrow Down).
pub fn scroll_view_down() {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        if let Some(writer) = FRAMEBUFFER_WRITER.lock().as_mut() {
            writer.scroll_view_down();
        }
    });
}
