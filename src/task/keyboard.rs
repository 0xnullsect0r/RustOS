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
lazy_static! {
    static ref KEYBOARD_DECODER: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> =
        Mutex::new(Keyboard::new(
            ScancodeSet1::new(),
            layouts::Us104Key,
            HandleControl::Ignore,
        ));
}

pub fn init() {
    if SCANCODE_QUEUE.try_get().is_err() {
        SCANCODE_QUEUE
            .try_init_once(|| ArrayQueue::new(100))
            .expect("failed to initialize keyboard scancode queue");
    }
}

/// Called by the keyboard interrupt handler
///
/// Must not block or allocate.
pub(crate) fn add_scancode(scancode: u8) {
    if let Ok(queue) = SCANCODE_QUEUE.try_get() {
        if queue.push(scancode).is_err() {
            println!("WARNING: scancode queue full; dropping keyboard input");
        } else {
            WAKER.wake();
        }
    } else {
        println!("WARNING: scancode queue uninitialized");
    }
}

pub struct ScancodeStream {
    _private: (),
}

impl ScancodeStream {
    pub fn new() -> Self {
        init();
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

pub fn read_input_byte() -> Option<u8> {
    let queue = SCANCODE_QUEUE.try_get().ok()?;
    let scancode = queue.pop().or_else(poll_ps2_scancode)?;
    decode_scancode(scancode)
}

fn decode_scancode(scancode: u8) -> Option<u8> {
    let mut keyboard = KEYBOARD_DECODER.lock();
    let key_event = keyboard.add_byte(scancode).ok()??;
    let key = keyboard.process_keyevent(key_event)?;
    match key {
        DecodedKey::Unicode(c) if c.is_ascii() => Some(c as u8),
        DecodedKey::RawKey(KeyCode::Backspace) => Some(0x08),
        _ => None,
    }
}

fn poll_ps2_scancode() -> Option<u8> {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut status_port = Port::<u8>::new(0x64);
        let status = status_port.read();
        if status & 0x01 == 0 {
            return None;
        }

        let mut data_port = Port::<u8>::new(0x60);
        let scancode = data_port.read();

        // Bit 5 indicates auxiliary PS/2 mouse data. The shell only decodes
        // keyboard set-1 scancodes, so ignore mouse bytes if firmware enables
        // the auxiliary device.
        if status & 0x20 != 0 {
            return None;
        }

        Some(scancode)
    }
}
