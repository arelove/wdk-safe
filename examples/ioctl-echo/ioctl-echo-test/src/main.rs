// Copyright (c) 2026 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test utility for the `ioctl-echo` kernel driver.
//!
//! Opens `\\.\WdkSafeEcho`, sends a `u32` via `IOCTL_ECHO`,
//! and verifies the echoed value matches.
//!
//! Usage:
//!   ioctl_echo_test.exe [value]
//!
//! Examples:
//!   ioctl_echo_test.exe          # uses default value 0xDEADBEEF
//!   ioctl_echo_test.exe 12345

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_NONE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

// CTL_CODE(DeviceType, Function, Method, Access)
// = (DeviceType << 16) | (Access << 14) | (Function << 2) | Method
//
// DeviceType = 0x8000, Function = 0x800, Method = 0 (Buffered), Access = 0 (Any)
// = (0x8000 << 16) | (0 << 14) | (0x800 << 2) | 0
// = 0x8000_0000 | 0x0000_2000
// = 0x8000_2000
const IOCTL_ECHO: u32 = 0x8000_2000;

#[repr(C)]
struct EchoRequest {
    value: u32,
}

#[repr(C)]
struct EchoResponse {
    value: u32,
}

fn to_wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn main() {
    // Parse optional CLI argument
    let send_value: u32 = std::env::args()
        .nth(1)
        .and_then(|s| {
            if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                u32::from_str_radix(hex, 16).ok()
            } else {
                s.parse().ok()
            }
        })
        .unwrap_or(0xDEAD_BEEF);

    println!("=== ioctl-echo test ===");
    println!("Device : \\\\.\\WdkSafeEcho");
    println!("IOCTL  : 0x{IOCTL_ECHO:08X}");
    println!("Send   : 0x{send_value:08X} ({})", send_value);

    // Open the device
    let path = to_wide_null("\\\\.\\WdkSafeEcho");
    let handle: HANDLE = unsafe {
        CreateFileW(
            path.as_ptr(),
            // GENERIC_READ | GENERIC_WRITE
            0x8000_0000u32 | 0x4000_0000u32,
            FILE_SHARE_NONE,
            std::ptr::null(),
            OPEN_EXISTING,
            0, // no FILE_FLAG_OVERLAPPED — synchronous
            std::ptr::null_mut(),
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        let err = unsafe { GetLastError() };
        eprintln!("ERROR: CreateFile failed — Win32 error 0x{err:08X}");
        eprintln!("  Is the driver running? (sc start ioctl-echo)");
        std::process::exit(1);
    }
    println!("Opened device handle: OK");

    // Prepare buffers
    let request = EchoRequest { value: send_value };
    let mut response = EchoResponse { value: 0 };
    let mut bytes_returned: u32 = 0;

    // Send IOCTL
    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_ECHO,
            &request as *const EchoRequest as *const _,
            std::mem::size_of::<EchoRequest>() as u32,
            &mut response as *mut EchoResponse as *mut _,
            std::mem::size_of::<EchoResponse>() as u32,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };

    unsafe { CloseHandle(handle) };

    if ok == 0 {
        let err = unsafe { GetLastError() };
        eprintln!("ERROR: DeviceIoControl failed — Win32 error 0x{err:08X}");
        std::process::exit(1);
    }

    println!("Received: 0x{:08X} ({})", response.value, response.value);
    println!("Bytes returned: {bytes_returned}");

    if response.value == send_value {
        println!("\n✓ PASS — echo correct");
    } else {
        eprintln!("\n✗ FAIL — expected 0x{send_value:08X}, got 0x{:08X}", response.value);
        std::process::exit(1);
    }
}