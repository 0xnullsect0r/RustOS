use crate::{print, println};
use conquer_once::spin::OnceCell;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use crossbeam_queue::ArrayQueue;
use futures_util::{
    stream::{Stream, StreamExt},
    task::AtomicWaker,
};
use lazy_static::lazy_static;
use pc_keyboard::{DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet1, layouts};
use spin::Mutex;

static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
static WAKER: AtomicWaker = AtomicWaker::new();

/// Decoded-byte queue for blocking SYS_READ from userspace processes.
/// Holds UTF-8 bytes and VT100 escape sequences generated from keypresses.
static STDIN_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();

lazy_static! {
    /// Separate keyboard decoder used exclusively for the stdin byte stream.
    /// Uses MapLettersToUnicode so Ctrl+letter combos produce control bytes
    /// (e.g. Ctrl-C → 0x03, Ctrl-U → 0x15, Ctrl-D → 0x04).
    static ref STDIN_KB: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> =
        Mutex::new(Keyboard::new(
            ScancodeSet1::new(),
            layouts::Us104Key,
            HandleControl::MapLettersToUnicode,
        ));
}

/// Initialise the stdin byte queue.  Must be called before the first userspace
/// process is launched.
pub fn init_stdin() {
    STDIN_QUEUE
        .try_init_once(|| ArrayQueue::new(256))
        .expect("init_stdin called more than once");
}

/// Drain all pending bytes from the stdin queue (call after a process exits to
/// prevent stale input from leaking into the next process or the kernel shell).
pub fn drain_stdin() {
    if let Ok(q) = STDIN_QUEUE.try_get() {
        while q.pop().is_some() {}
    }
}

/// Drain all pending raw scancodes from the async scancode queue (call before
/// launching a userspace process so accumulated scancodes are not replayed to
/// the kernel shell on return).
pub fn drain_scancode_queue() {
    if let Ok(q) = SCANCODE_QUEUE.try_get() {
        while q.pop().is_some() {}
    }
}

/// Read up to `buf.len()` bytes from the stdin queue, blocking (spin + hlt)
/// until at least one byte is available.  Returns the number of bytes read.
pub fn read_stdin(buf: &mut [u8]) -> usize {
    if buf.is_empty() {
        return 0;
    }
    let queue = match STDIN_QUEUE.try_get() {
        Ok(q) => q,
        Err(_) => return 0,
    };
    // Spin until at least one byte is available.
    loop {
        if let Some(b) = queue.pop() {
            buf[0] = b;
            let mut n = 1;
            while n < buf.len() {
                match queue.pop() {
                    Some(b) => {
                        buf[n] = b;
                        n += 1;
                    }
                    None => break,
                }
            }
            return n;
        }
        // Yield to allow keyboard interrupts to fire while we wait.
        x86_64::instructions::hlt();
    }
}

/// Called by the keyboard interrupt handler.
///
/// Pushes the raw scancode onto the async queue (for the kernel shell task)
/// and also decodes it into the stdin byte queue (for userspace SYS_READ).
/// Must not block or allocate.
pub(crate) fn add_scancode(scancode: u8) {
    // ── Async queue (kernel shell task) ──────────────────────────────────
    if let Ok(queue) = SCANCODE_QUEUE.try_get() {
        if queue.push(scancode).is_err() {
            println!("WARNING: scancode queue full; dropping keyboard input");
        } else {
            WAKER.wake();
        }
    } else {
        println!("WARNING: scancode queue uninitialized");
    }

    // ── Stdin byte queue (userspace SYS_READ) ─────────────────────────────
    let stdin_queue = match STDIN_QUEUE.try_get() {
        Ok(q) => q,
        Err(_) => return,
    };
    if let Some(mut kb) = STDIN_KB.try_lock() {
        if let Ok(Some(key_event)) = kb.add_byte(scancode) {
            if let Some(key) = kb.process_keyevent(key_event) {
                match key {
                    DecodedKey::Unicode(c) => {
                        // Encode to UTF-8 (almost always a single ASCII byte).
                        let mut tmp = [0u8; 4];
                        for &b in c.encode_utf8(&mut tmp).as_bytes() {
                            let _ = stdin_queue.push(b);
                        }
                    }
                    DecodedKey::RawKey(k) => {
                        // Emit standard VT100 escape sequences for navigation keys.
                        let seq: &[u8] = match k {
                            KeyCode::ArrowUp => b"\x1b[A",
                            KeyCode::ArrowDown => b"\x1b[B",
                            KeyCode::ArrowRight => b"\x1b[C",
                            KeyCode::ArrowLeft => b"\x1b[D",
                            KeyCode::Delete => b"\x1b[3~",
                            KeyCode::Home => b"\x1b[H",
                            KeyCode::End => b"\x1b[F",
                            KeyCode::PageUp => b"\x1b[5~",
                            KeyCode::PageDown => b"\x1b[6~",
                            _ => b"",
                        };
                        for &b in seq {
                            let _ = stdin_queue.push(b);
                        }
                    }
                }
            }
        }
    }
}

pub struct ScancodeStream {
    _private: (),
}

impl ScancodeStream {
    pub fn new() -> Self {
        SCANCODE_QUEUE
            .try_init_once(|| ArrayQueue::new(100))
            .expect("ScancodeStream::new should only be called once");
        ScancodeStream { _private: () }
    }
}

impl Default for ScancodeStream {
    fn default() -> Self {
        Self::new()
    }
}

impl Stream for ScancodeStream {
    type Item = u8;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<u8>> {
        let queue = SCANCODE_QUEUE
            .try_get()
            .expect("scancode queue not initialized");

        // fast path
        if let Some(scancode) = queue.pop() {
            return Poll::Ready(Some(scancode));
        }

        WAKER.register(cx.waker());
        match queue.pop() {
            Some(scancode) => {
                WAKER.take();
                Poll::Ready(Some(scancode))
            }
            None => Poll::Pending,
        }
    }
}

pub async fn print_keypresses() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );

    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode)
            && let Some(key) = keyboard.process_keyevent(key_event)
        {
            match key {
                DecodedKey::Unicode(character) => print!("{}", character),
                DecodedKey::RawKey(key) => print!("{:?}", key),
            }
        }
    }
}
