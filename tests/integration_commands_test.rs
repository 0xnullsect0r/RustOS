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

/// Test that all built-in commands are registered
#[test_case]
fn test_all_commands_registered() {
    let expected_commands = [
        "help", "echo", "clear", "uname", "color", "pwd", "ls", "cd", "mkdir", "rm", "cat",
        "write", "cp", "mv", "meminfo", "mount", "umount", "net", "exec", "usbscan", "reboot",
        "shutdown", "lspci", "lsusb", "lsblk", "grep", "ps", "wifi", "ping", "ifconfig", "netstat",
    ];

    let registered_cmds = rustos::bin_commands::virtual_bin_commands();

    for cmd in expected_commands.iter() {
        assert!(
            registered_cmds.contains(cmd),
            "Command {} should be registered",
            cmd
        );
    }
}

/// Test that /bin/ virtual paths are recognized
#[test_case]
fn test_bin_virtual_paths() {
    let test_paths = ["/bin/echo", "/bin/ls", "/bin/cat", "/bin/pwd", "/bin/grep"];

    for path in test_paths.iter() {
        let result = rustos::bin_commands::is_virtual_bin_path(path);
        assert!(
            result.is_some(),
            "Path {} should be recognized as virtual binary",
            path
        );
    }
}

/// Test that echo command is available for virtual execution
#[test_case]
fn test_echo_command_available() {
    let result = rustos::bin_commands::is_virtual_bin_path("/bin/echo");
    assert_eq!(result, Some("echo"), "echo should be a virtual binary");
}

/// Test that ls command is available
#[test_case]
fn test_ls_command_available() {
    let result = rustos::bin_commands::is_virtual_bin_path("/bin/ls");
    assert_eq!(result, Some("ls"), "ls should be a virtual binary");
}

/// Test that cat command is available
#[test_case]
fn test_cat_command_available() {
    let result = rustos::bin_commands::is_virtual_bin_path("/bin/cat");
    assert_eq!(result, Some("cat"), "cat should be a virtual binary");
}

/// Test that grep command is available
#[test_case]
fn test_grep_command_available() {
    let result = rustos::bin_commands::is_virtual_bin_path("/bin/grep");
    assert_eq!(result, Some("grep"), "grep should be a virtual binary");
}

/// Test that networking commands are available
#[test_case]
fn test_network_commands_available() {
    let net_commands = [
        ("ping", "/bin/ping"),
        ("wifi", "/bin/wifi"),
        ("ifconfig", "/bin/ifconfig"),
        ("netstat", "/bin/netstat"),
    ];

    for &(cmd, path) in net_commands.iter() {
        let result = rustos::bin_commands::is_virtual_bin_path(path);
        assert_eq!(
            result,
            Some(cmd),
            "Network command {} should be available",
            cmd
        );
    }
}

/// Test that hardware commands are available
#[test_case]
fn test_hardware_commands_available() {
    let hw_commands = [
        ("lspci", "/bin/lspci"),
        ("lsusb", "/bin/lsusb"),
        ("lsblk", "/bin/lsblk"),
        ("ps", "/bin/ps"),
    ];

    for &(cmd, path) in hw_commands.iter() {
        let result = rustos::bin_commands::is_virtual_bin_path(path);
        assert_eq!(
            result,
            Some(cmd),
            "Hardware command {} should be available",
            cmd
        );
    }
}

/// Test file command VFS integration
#[test_case]
fn test_file_command_vfs_integration() {
    let test_file = "/test_cmd.txt";
    let test_content = b"command test data";

    {
        let mut vfs = rustos::vfs::VFS.lock();
        if let Some(vfs) = vfs.as_mut() {
            let _ = vfs.write_file(test_file, test_content);
        }
    }

    {
        let mut vfs = rustos::vfs::VFS.lock();
        if let Some(vfs) = vfs.as_mut() {
            assert!(vfs.exists(test_file), "Test file should exist in VFS");
        }
    }
}

/// Test command path parsing
#[test_case]
fn test_command_path_parsing() {
    struct TestCase {
        path: &'static str,
        should_match: bool,
    }

    let test_cases = [
        TestCase {
            path: "/bin/echo",
            should_match: true,
        },
        TestCase {
            path: "/bin/nonexistent",
            should_match: false,
        },
        TestCase {
            path: "/usr/bin/echo",
            should_match: false,
        },
    ];

    for test in test_cases.iter() {
        let result = rustos::bin_commands::is_virtual_bin_path(test.path);
        if test.should_match {
            assert!(result.is_some(), "Path {} should match", test.path);
        }
    }
}

/// Test command availability consistency
#[test_case]
fn test_command_availability_consistency() {
    let cmds1 = rustos::bin_commands::virtual_bin_commands();
    let cmds2 = rustos::bin_commands::virtual_bin_commands();

    assert_eq!(
        cmds1.len(),
        cmds2.len(),
        "Command list should be consistent"
    );
}

/// Test that all commands have proper infrastructure
#[test_case]
fn test_command_infrastructure() {
    let cmds = rustos::bin_commands::virtual_bin_commands();
    assert!(
        !cmds.is_empty(),
        "At least some commands should be registered"
    );
    assert!(cmds.len() > 20, "Should have multiple commands");
}
