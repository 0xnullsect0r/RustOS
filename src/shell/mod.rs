//! Interactive shell module.

pub mod commands;

use alloc::{string::String, vec::Vec};
use crate::drivers::vga::Color;
use crate::vfs::RamFs;

/// The interactive shell state: VFS instance, input line buffer, and current colors.
pub struct Shell {
    pub fs: RamFs,
    input_buf: String,
    pub fg_color: Color,
    pub bg_color: Color,
}

impl Shell {
    pub fn new() -> Self {
        Shell {
            fs: RamFs::new(),
            input_buf: String::new(),
            fg_color: Color::Yellow,
            bg_color: Color::Black,
        }
    }

    /// Process a single decoded unicode character from the keyboard.
    pub fn handle_char(&mut self, c: char) {
        match c {
            '\n' | '\r' => {
                crate::println!();
                let line = self.input_buf.clone();
                self.input_buf.clear();
                self.execute(&line);
                self.print_prompt();
            }
            // Backspace / DEL
            '\x08' | '\x7f' => {
                if self.input_buf.pop().is_some() {
                    crate::print!("\x08 \x08");
                }
            }
            c if c.is_ascii() && !c.is_ascii_control() => {
                self.input_buf.push(c);
                crate::print!("{}", c);
            }
            _ => {}
        }
    }

    /// Print the shell prompt, including the current working directory.
    pub fn print_prompt(&self) {
        crate::print!("rustos:{}> ", self.fs.cwd());
    }

    fn execute(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        // Split into command and remaining args string
        let (cmd, rest) = match line.find(' ') {
            Some(pos) => (&line[..pos], line[pos + 1..].trim()),
            None => (line, ""),
        };
        let args: Vec<&str> = rest.split_whitespace().collect();
        commands::dispatch(self, cmd, &args);
    }
}
