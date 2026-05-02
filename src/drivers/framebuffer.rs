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
        }
    }

    /// Sets the foreground and background colors.
    pub fn set_colors(&mut self, foreground: Color, background: Color) {
        self.foreground = foreground;
        self.background = background;
    }

    /// Writes a single pixel at the given position.
    fn write_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }

        let pixel_offset = y * self.info.stride + x;
        let byte_offset = pixel_offset * self.info.bytes_per_pixel;

        if byte_offset + self.info.bytes_per_pixel > self.framebuffer.len() {
            return;
        }

        let color_bytes = match self.info.pixel_format {
            PixelFormat::Rgb => [color.r, color.g, color.b, 0],
            PixelFormat::Bgr => [color.b, color.g, color.r, 0],
            _ => [color.r, color.g, color.b, 0],
        };

        let len = self.info.bytes_per_pixel.min(4);
        self.framebuffer[byte_offset..(len + byte_offset)].copy_from_slice(&color_bytes[..len]);
    }

    /// Clears the entire screen with the background color.
    pub fn clear_screen(&mut self) {
        for y in 0..self.info.height {
            for x in 0..self.info.width {
                self.write_pixel(x, y, self.background);
            }
        }
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

    /// Scrolls the screen up by one line.
    fn scroll_up(&mut self) {
        // Copy all lines up by FONT_HEIGHT pixels
        for y in FONT_HEIGHT..self.info.height {
            for x in 0..self.info.width {
                // Read pixel from current line
                let src_offset = y * self.info.stride + x;
                let src_byte_offset = src_offset * self.info.bytes_per_pixel;

                let dst_y = y - FONT_HEIGHT;
                let dst_offset = dst_y * self.info.stride + x;
                let dst_byte_offset = dst_offset * self.info.bytes_per_pixel;

                // Copy the pixel
                for i in 0..self.info.bytes_per_pixel {
                    if src_byte_offset + i < self.framebuffer.len()
                        && dst_byte_offset + i < self.framebuffer.len()
                    {
                        self.framebuffer[dst_byte_offset + i] =
                            self.framebuffer[src_byte_offset + i];
                    }
                }
            }
        }

        // Clear the last line
        let last_line_y = self.info.height - FONT_HEIGHT;
        for y in last_line_y..self.info.height {
            for x in 0..self.info.width {
                self.write_pixel(x, y, self.background);
            }
        }
    }

    /// Moves to a new line, scrolling if necessary.
    fn newline(&mut self) {
        self.x_pos = 0;
        self.y_pos += 1;

        if self.y_pos >= self.lines_per_screen() {
            self.scroll_up();
            self.y_pos = self.lines_per_screen() - 1;
        }
    }

    /// Handles backspace by moving cursor back and clearing the character.
    fn backspace(&mut self) {
        if self.x_pos > 0 {
            self.x_pos -= 1;
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
        match byte {
            b'\n' => self.newline(),
            0x08 => self.backspace(), // Backspace
            byte => {
                if self.x_pos >= self.chars_per_line() {
                    self.newline();
                }

                self.draw_char(byte, self.x_pos, self.y_pos);
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
        let writer = FrameBufferWriter::new(framebuffer);
        *FRAMEBUFFER_WRITER.lock() = Some(writer);
    }
}
