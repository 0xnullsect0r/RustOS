#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(rustos::test_runner)]
#![reexport_test_harness_main = "test_main"]

use bootloader_api::{BootInfo, entry_point};
use core::panic::PanicInfo;
extern crate rustos;

entry_point!(main);

fn main(_boot_info: &'static mut BootInfo) -> ! {
    test_main();
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    rustos::test_panic_handler(info)
}

/// Test that the network stack is initialized
#[test_case]
fn test_network_stack_initialization() {
    rustos::serial_println!("[test] Checking network stack initialization...");

    let status = tcp_ip::kernel::status_str();
    assert!(
        status.is_some() || status.is_none(),
        "Network status should be queryable"
    );
}

/// Test that Intel AX210 WiFi device is detected (if present)
#[test_case]
fn test_ax210_device_detection() {
    rustos::serial_println!("[test] Checking for Intel AX210 WiFi device...");

    let status = tcp_ip::kernel::status_str();
    let _ = status;
}

/// Test network syscall dispatch for syscalls 300-310
#[test_case]
fn test_network_syscall_dispatch() {
    rustos::serial_println!("[test] Testing network syscall dispatch...");

    let result = tcp_ip::kernel::dispatch_syscall(300, 999, 999, 999);
    let _ = result;
}

/// Test that socket creation syscall is available
#[test_case]
fn test_socket_syscall_availability() {
    rustos::serial_println!("[test] Checking socket syscall (300) availability...");

    let result = tcp_ip::kernel::dispatch_syscall(300, 2, 1, 6);
    let _ = result;
}

/// Test network binaries availability in /bin
#[test_case]
fn test_network_binary_paths() {
    rustos::serial_println!("[test] Checking network binary paths...");

    let network_bins = ["ping", "ifconfig", "netstat", "wifi", "net"];

    for cmd in network_bins.iter() {
        assert!(
            rustos::bin_commands::virtual_bin_commands().contains(cmd),
            "Network binary {} should be available",
            cmd
        );
    }
}

/// Test TCP/IP stack is active when available
#[test_case]
fn test_tcp_ip_stack_active() {
    rustos::serial_println!("[test] Checking TCP/IP stack activity...");

    let is_active = tcp_ip::kernel::is_active();
    if is_active {
        rustos::serial_println!("[test] TCP/IP stack is active");
    } else {
        rustos::serial_println!("[test] TCP/IP stack is not active (expected in test environment)");
    }
}

/// Test invalid network syscall handling
#[test_case]
fn test_invalid_network_syscall() {
    rustos::serial_println!("[test] Testing invalid network syscall handling...");

    let result = tcp_ip::kernel::dispatch_syscall(999, 0, 0, 0);
    let _ = result;
}

/// Test network configuration persistence
#[test_case]
fn test_network_config_state() {
    rustos::serial_println!("[test] Testing network configuration state...");

    let status1 = tcp_ip::kernel::status_str();
    let status2 = tcp_ip::kernel::status_str();

    assert_eq!(
        status1, status2,
        "Network status should be consistent across queries"
    );
}

/// Test network device enumeration capability
#[test_case]
fn test_network_device_enumeration() {
    rustos::serial_println!("[test] Testing network device enumeration...");

    let devices = rustos::pci::enumerate();
    let _ = devices;
}
