use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;
use x86_64::instructions::port::Port;

const COM1_PORT: u16 = 0x3f8;
const SERIAL_SPIN_LIMIT: usize = 10_000;

static SERIAL_LOCK: Mutex<()> = Mutex::new(());
static SERIAL_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Best-effort COM1 initialization.
///
/// Modern laptops often have no legacy UART at COM1. Port I/O is still safe in
/// ring 0, but probing libraries can fail or block on absent hardware. Keep this
/// path non-panicking so early boot never depends on QEMU-style serial hardware.
pub fn init() {
    if SERIAL_INITIALIZED.swap(true, Ordering::AcqRel) {
        return;
    }

    unsafe {
        Port::<u8>::new(COM1_PORT + 1).write(0x00); // Disable interrupts
        Port::<u8>::new(COM1_PORT + 3).write(0x80); // Enable DLAB
        Port::<u8>::new(COM1_PORT).write(0x03); // 38400 baud divisor low
        Port::<u8>::new(COM1_PORT + 1).write(0x00); // divisor high
        Port::<u8>::new(COM1_PORT + 3).write(0x03); // 8N1
        Port::<u8>::new(COM1_PORT + 2).write(0xc7); // Enable FIFO
        Port::<u8>::new(COM1_PORT + 4).write(0x0b); // IRQs disabled, RTS/DSR set
    }
}

#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments) {
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        let _guard = SERIAL_LOCK.lock();
        init();
        let _ = SerialWriter.write_fmt(args);
    });
}

struct SerialWriter;

impl fmt::Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            match byte {
                b'\n' => {
                    write_byte(b'\r');
                    write_byte(b'\n');
                }
                byte => write_byte(byte),
            }
        }
        Ok(())
    }
}

fn write_byte(byte: u8) {
    unsafe {
        let mut line_status = Port::<u8>::new(COM1_PORT + 5);
        for _ in 0..SERIAL_SPIN_LIMIT {
            if line_status.read() & 0x20 != 0 {
                break;
            }
        }
        // If absent hardware never reports ready, still attempt the byte once.
        // This keeps serial diagnostic output best-effort rather than blocking
        // boot on real laptops without COM1.
        Port::<u8>::new(COM1_PORT).write(byte);
    }
}

/// Prints to the host through the serial interface.
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::drivers::serial::_print(format_args!($($arg)*));
    };
}

/// Prints to the host through the serial interface, appending a newline.
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(
        concat!($fmt, "\n"), $($arg)*));
}
