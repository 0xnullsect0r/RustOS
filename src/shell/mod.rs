//! Interactive shell module.

pub mod commands;

use crate::drivers::vga::Color;
use alloc::string::String;

/// The interactive shell state: current directory, input line buffer, and current colors.
pub struct Shell {
    pub cwd: String,
    input_buf: String,
    pub fg_color: Color,
    pub bg_color: Color,
}

impl Shell {
    pub fn new() -> Self {
        Shell {
            cwd: String::from("/"),
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
            '\x08' | '\x7f' if self.input_buf.pop().is_some() => {
                crate::print!("\x08 \x08");
            }
            '\x08' | '\x7f' => {}
            c if c.is_ascii() && !c.is_ascii_control() => {
                self.input_buf.push(c);
                crate::print!("{}", c);
            }
            _ => {}
        }
    }

    /// Print the shell prompt, including the current working directory.
    pub fn print_prompt(&self) {
        crate::print!("rustos:{}> ", self.cwd);
    }

    /// Resolve a path relative to the shell's current working directory.
    pub fn resolve_path(&self, path: &str) -> String {
        if path.starts_with('/') {
            crate::vfs::RamFs::pub_normalize(path)
        } else if path.is_empty() || path == "." {
            self.cwd.clone()
        } else {
            let base = if self.cwd == "/" {
                String::from("/")
            } else {
                alloc::format!("{}/", self.cwd)
            };
            crate::vfs::RamFs::pub_normalize(&alloc::format!("{}{}", base, path))
        }
    }

    fn execute(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let (cmd, rest) = match line.find(' ') {
            Some(pos) => (&line[..pos], line[pos + 1..].trim()),
            None => (line, ""),
        };
        let args: alloc::vec::Vec<&str> = rest.split_whitespace().collect();
        commands::dispatch(self, cmd, &args);
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}
