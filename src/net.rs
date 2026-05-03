//! Kernel-side integration points for the external `tcp-ip` stack submodule.
//!
//! The stack source is pinned as a git submodule under `third_party/tcp-ip`.
//! This module owns the RustOS-facing ABI: boot-time network device discovery,
//! status reporting, and the syscall numbers consumed by the tcp-ip userspace
//! management tools.

use spin::Mutex;

const TCP_IP_SUBMODULE_COMMIT: &str = "52dc615e209218d3974bbb67ec01e6e00e5cdeec";

const INTEL_VENDOR_ID: u16 = 0x8086;
const AX210_DEVICE_IDS: &[u16] = &[0x2725, 0x51F0, 0x54F0, 0x7F70];

pub const SYS_WIFI_SCAN: u64 = 300;
pub const SYS_WIFI_CONNECT: u64 = 301;
pub const SYS_WIFI_DISCONNECT: u64 = 302;
pub const SYS_WIFI_STATUS: u64 = 303;
pub const SYS_NET_IFCONFIG: u64 = 304;
pub const SYS_NET_IFCONFIG_SET: u64 = 305;
pub const SYS_NET_PING: u64 = 306;
pub const SYS_NET_STAT: u64 = 307;
pub const SYS_NET_DHCP: u64 = 308;
pub const SYS_NET_ROUTES: u64 = 310;

const ENODEV: i64 = -19;
const EINVAL: i64 = -22;

#[derive(Clone, Copy)]
struct NetworkDevice {
    bus: u8,
    dev: u8,
    func: u8,
    vendor_id: u16,
    device_id: u16,
    bar0: u64,
}

#[derive(Clone, Copy)]
struct NetworkState {
    initialized: bool,
    ax210: Option<NetworkDevice>,
}

static NETWORK_STATE: Mutex<NetworkState> = Mutex::new(NetworkState {
    initialized: false,
    ax210: None,
});

pub fn init() {
    let devices = crate::pci::enumerate();
    let ax210 = devices
        .iter()
        .find(|dev| dev.vendor_id == INTEL_VENDOR_ID && AX210_DEVICE_IDS.contains(&dev.device_id))
        .map(|dev| NetworkDevice {
            bus: dev.bus,
            dev: dev.dev,
            func: dev.func,
            vendor_id: dev.vendor_id,
            device_id: dev.device_id,
            bar0: dev.mmio_base(0),
        });

    *NETWORK_STATE.lock() = NetworkState {
        initialized: true,
        ax210,
    };

    match ax210 {
        Some(dev) => {
            crate::serial_println!(
                "[net] AX210-family WiFi device {:04x}:{:04x} at {:02x}:{:02x}.{} BAR0=0x{:x}",
                dev.vendor_id,
                dev.device_id,
                dev.bus,
                dev.dev,
                dev.func,
                dev.bar0
            );
        }
        None => {
            crate::serial_println!("[net] no Intel AX210-family WiFi device found");
        }
    }
}

pub fn print_status() {
    let state = *NETWORK_STATE.lock();
    crate::println!(
        "tcp-ip submodule: third_party/tcp-ip @ {}",
        TCP_IP_SUBMODULE_COMMIT
    );
    if !state.initialized {
        crate::println!("network: not initialized");
        return;
    }
    match state.ax210 {
        Some(dev) => {
            crate::println!(
                "wlan0: AX210-family {:04x}:{:04x} at {:02x}:{:02x}.{}",
                dev.vendor_id,
                dev.device_id,
                dev.bus,
                dev.dev,
                dev.func
            );
            crate::println!("wlan0: driver hooks present, link down until tcp-ip driver is active");
        }
        None => crate::println!("wlan0: no AX210-family WiFi device detected"),
    }
}

pub fn dispatch_syscall(nr: u64, a1: u64, a2: u64, _a3: u64) -> Option<i64> {
    let ret = match nr {
        SYS_WIFI_SCAN | SYS_WIFI_STATUS | SYS_NET_IFCONFIG | SYS_NET_STAT | SYS_NET_ROUTES => {
            empty_query(a1 as *mut u8, a2 as usize)
        }
        SYS_WIFI_CONNECT | SYS_NET_IFCONFIG_SET => validate_input(a1 as *const u8, a2 as usize),
        SYS_WIFI_DISCONNECT => {
            if has_device() {
                0
            } else {
                ENODEV
            }
        }
        SYS_NET_PING | SYS_NET_DHCP => ENODEV,
        _ => return None,
    };
    Some(ret)
}

fn has_device() -> bool {
    NETWORK_STATE.lock().ax210.is_some()
}

fn empty_query(buf: *mut u8, len: usize) -> i64 {
    if len > 0 && buf.is_null() {
        return EINVAL;
    }
    0
}

fn validate_input(buf: *const u8, len: usize) -> i64 {
    if len > 0 && buf.is_null() {
        return EINVAL;
    }
    ENODEV
}
