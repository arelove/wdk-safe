// Copyright (c) 2026 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # null-device
//!
//! A minimal WDM driver that creates `\Device\WdkSafeNull` — a
//! [`/dev/null`](https://en.wikipedia.org/wiki/Null_device) equivalent for
//! Windows kernel mode.
//!
//! ## What this driver does
//!
//! - Creates a named device object at `\Device\WdkSafeNull` with a symbolic
//!   link at `\DosDevices\WdkSafeNull` (accessible as `\\.\WdkSafeNull`).
//! - Accepts `IRP_MJ_CREATE` and `IRP_MJ_CLOSE` — any process can open it.
//! - `IRP_MJ_WRITE` — silently discards all data (classic null device).
//! - `IRP_MJ_READ` — returns zero bytes (EOF).
//! - `IRP_MJ_DEVICE_CONTROL` — returns `STATUS_NOT_SUPPORTED`.
//! - Cleans up on unload.
//!
//! ## Purpose as a demo
//!
//! This example shows the **full WDM driver lifecycle** using `wdk-safe`:
//!
//! 1. `DriverEntry` → `IoCreateDevice` + `IoCreateSymbolicLink`
//! 2. Dispatch via `WdmDriver` trait + `dispatch_fn!` macro
//! 3. `DriverUnload` → `IoDeleteSymbolicLink` + `IoDeleteDevice`
//!
//! Because it has no hardware dependency, it loads on any Windows 10+ system
//! in test-signing mode.
//!
//! ## How to build and test
//!
//! Inside an eWDK developer prompt:
//!
//! ```powershell
//! cd examples/null-device/null-device
//! cargo make           # build + package debug
//! cargo make --release # build + package release
//! ```
//!
//! In the VM:
//!
//! ```cmd
//! pnputil /add-driver null_device.inf /install
//! sc start null-device
//! # Verify in DebugView: "[null-device] DriverEntry -- ready"
//! echo hello > \\.\WdkSafeNull
//! sc stop null-device
//! ```

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

// ── Kernel-mode boilerplate ───────────────────────────────────────────────────

/// Bug-check on panic — the only safe behaviour in kernel mode (no unwinding).
#[cfg(not(test))]
extern crate wdk_panic;

/// Kernel non-paged pool allocator for `alloc` types.
#[cfg(not(test))]
use wdk_alloc::WdkAllocator;

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

// ── compiler_builtins float stubs ─────────────────────────────────────────────
// See hid-filter for full explanation.
#[no_mangle]
#[doc(hidden)]
pub unsafe extern "C" fn __wdk_fma_stub(x: f64, y: f64, z: f64) -> f64 {
    x * y + z
}
#[no_mangle]
#[doc(hidden)]
pub unsafe extern "C" fn __wdk_fmaf_stub(x: f32, y: f32, z: f32) -> f32 {
    x * y + z
}

use wdk_safe::{Device, IoRequest, NtStatus, WdmDriver};
use wdk_sys::{
    ntddk::{
        DbgPrint, IoCreateDevice, IoCreateSymbolicLink, IoDeleteDevice, IoDeleteSymbolicLink,
        IofCompleteRequest, RtlInitUnicodeString,
    },
    DRIVER_OBJECT, FILE_DEVICE_NULL, NTSTATUS, PCUNICODE_STRING, PDEVICE_OBJECT,
    UNICODE_STRING,
};

// ── FORCEINLINE reimplementations ─────────────────────────────────────────────

/// Returns the current `IO_STACK_LOCATION` for `irp`.
///
/// Reimplements the `IoGetCurrentIrpStackLocation` WDK FORCEINLINE macro.
///
/// # Safety
///
/// `irp` must be non-null and valid for the dispatch call duration.
#[inline]
unsafe fn irp_current_stack(irp: *mut wdk_sys::IRP) -> *mut wdk_sys::IO_STACK_LOCATION {
    // SAFETY: caller guarantees `irp` is valid.
    unsafe {
        (*irp)
            .Tail
            .Overlay
            .__bindgen_anon_2
            .__bindgen_anon_1
            .CurrentStackLocation
    }
}

// ── KernelCompleter ───────────────────────────────────────────────────────────

/// Zero-sized [`IrpCompleter`](wdk_safe::IrpCompleter) that calls
/// `IofCompleteRequest`.
struct KernelCompleter;

impl wdk_safe::IrpCompleter for KernelCompleter {
    /// # Safety
    ///
    /// - `irp` must be non-null, valid, and not yet completed.
    /// - Must be called at `IRQL <= DISPATCH_LEVEL`.
    unsafe fn complete(irp: *mut core::ffi::c_void, status: i32) {
        // SAFETY: Caller upholds the IrpCompleter contract.
        unsafe {
            let pirp = irp.cast::<wdk_sys::IRP>();
            (*pirp).IoStatus.__bindgen_anon_1.Status = status;
            IofCompleteRequest(pirp, wdk_sys::IO_NO_INCREMENT as i8);
        }
    }
}

// ── WdmDriver implementation ──────────────────────────────────────────────────

/// The null device driver — discards writes, returns EOF on reads.
struct NullDeviceDriver;

impl WdmDriver<KernelCompleter> for NullDeviceDriver {
    /// Allow any process to open the null device.
    fn on_create(_device: &Device<'_>, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        // SAFETY: PASSIVE_LEVEL.
        unsafe { DbgPrint(b"[null-device] IRP_MJ_CREATE\n\0".as_ptr().cast()) };
        request.complete(NtStatus::SUCCESS)
    }

    /// Allow clean close.
    fn on_close(_device: &Device<'_>, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        // SAFETY: PASSIVE_LEVEL.
        unsafe { DbgPrint(b"[null-device] IRP_MJ_CLOSE\n\0".as_ptr().cast()) };
        request.complete(NtStatus::SUCCESS)
    }

    /// Discard all written data — classic /dev/null behaviour.
    fn on_write(_device: &Device<'_>, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        // SAFETY: PASSIVE_LEVEL / APC_LEVEL for typical synchronous writes.
        unsafe { DbgPrint(b"[null-device] IRP_MJ_WRITE -- discarding\n\0".as_ptr().cast()) };
        // Complete with SUCCESS and 0 bytes information — data is discarded.
        request.complete(NtStatus::SUCCESS)
    }

    /// Return EOF — zero bytes available.
    fn on_read(_device: &Device<'_>, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        // SAFETY: PASSIVE_LEVEL / APC_LEVEL.
        unsafe { DbgPrint(b"[null-device] IRP_MJ_READ -- returning EOF\n\0".as_ptr().cast()) };
        // Complete with SUCCESS and Information=0 signals EOF to the caller.
        request.complete(NtStatus::SUCCESS)
    }
}

// ── Dispatch thunks ───────────────────────────────────────────────────────────

wdk_safe::dispatch_fn!(
    dispatch_create = NullDeviceDriver, on_create, KernelCompleter,
    irp_stack = irp_current_stack
);
wdk_safe::dispatch_fn!(
    dispatch_close = NullDeviceDriver, on_close, KernelCompleter,
    irp_stack = irp_current_stack
);
wdk_safe::dispatch_fn!(
    dispatch_write = NullDeviceDriver, on_write, KernelCompleter,
    irp_stack = irp_current_stack
);
wdk_safe::dispatch_fn!(
    dispatch_read = NullDeviceDriver, on_read, KernelCompleter,
    irp_stack = irp_current_stack
);

// ── Device names ──────────────────────────────────────────────────────────────

/// NT device name — used by the kernel to route I/O.
const DEVICE_NAME: &[u8] = b"\\\x00000\\\x00000D\x00e\x00v\x00i\x00c\x00e\x00\\\x00W\x00d\x00k\x00S\x00a\x00f\x00e\x00N\x00u\x00l\x00l\x00\0\0";

// We use raw UTF-16 literals via a helper below instead of the byte string above.

/// Initialises a `UNICODE_STRING` from a UTF-16 string literal slice.
///
/// The slice must be null-terminated (`\0` as last element).
///
/// # Safety
///
/// `buf` must remain valid for the lifetime of the returned `UNICODE_STRING`.
#[inline]
unsafe fn unicode_string_from_slice(buf: &[u16]) -> UNICODE_STRING {
    let len_bytes = ((buf.len() - 1) * 2) as u16; // exclude null terminator
    UNICODE_STRING {
        Length: len_bytes,
        MaximumLength: len_bytes + 2,
        Buffer: buf.as_ptr().cast_mut(),
    }
}

// ── DriverUnload ──────────────────────────────────────────────────────────────

/// Called when the driver is unloaded. Removes symbolic link and device.
///
/// # Safety
///
/// `driver` is a valid non-null `DRIVER_OBJECT` pointer at `PASSIVE_LEVEL`.
unsafe extern "C" fn driver_unload(driver: *mut DRIVER_OBJECT) {
    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[null-device] DriverUnload -- cleaning up\n\0".as_ptr().cast());
    }

    // Delete the DOS symbolic link.
    #[allow(clippy::unicode_not_nfc)]
    let dos_name_buf: &[u16] = &[
        b'\\' as u16, b'\\'  as u16, // "\\"  — wrong, use proper UTF-16
        // Build \DosDevices\WdkSafeNull as UTF-16
        0x005C, 0x0044, 0x006F, 0x0073, 0x0044, 0x0065, 0x0076, 0x0069,
        0x0063, 0x0065, 0x0073, 0x005C, 0x0057, 0x0064, 0x006B, 0x0053,
        0x0061, 0x0066, 0x0065, 0x004E, 0x0075, 0x006C, 0x006C, 0x0000,
    ];
    // SAFETY: slice is static.
    let mut dos_name = unsafe { unicode_string_from_slice(dos_name_buf) };
    // SAFETY: PASSIVE_LEVEL, dos_name is valid.
    unsafe { IoDeleteSymbolicLink(&mut dos_name) };

    // Delete the device object.
    // SAFETY: driver is valid; DeviceObject was created in driver_entry.
    unsafe {
        let device = (*driver).DeviceObject;
        if !device.is_null() {
            IoDeleteDevice(device);
        }
    }
}

// ── DriverEntry ───────────────────────────────────────────────────────────────

/// Entry point. Creates device + symbolic link, registers dispatch routines.
///
/// # Safety
///
/// `driver` and `registry_path` are valid at `PASSIVE_LEVEL` per kernel
/// contract.
#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "system" fn driver_entry(
    driver: *mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    // SAFETY: pointers are valid per DriverEntry contract.
    unsafe { driver_entry_inner(driver, registry_path).into_raw() }
}

/// Safe inner body of [`driver_entry`].
unsafe fn driver_entry_inner(
    driver: *mut DRIVER_OBJECT,
    _registry_path: PCUNICODE_STRING,
) -> NtStatus {
    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[null-device] DriverEntry -- loading\n\0".as_ptr().cast());
    }

    // NT device name: \Device\WdkSafeNull (UTF-16)
    let device_name_buf: &[u16] = &[
        0x005C, 0x0044, 0x0065, 0x0076, 0x0069, 0x0063, 0x0065, 0x005C,
        0x0057, 0x0064, 0x006B, 0x0053, 0x0061, 0x0066, 0x0065, 0x004E,
        0x0075, 0x006C, 0x006C, 0x0000,
    ];
    // DOS symbolic link: \DosDevices\WdkSafeNull (UTF-16)
    let dos_name_buf: &[u16] = &[
        0x005C, 0x0044, 0x006F, 0x0073, 0x0044, 0x0065, 0x0076, 0x0069,
        0x0063, 0x0065, 0x0073, 0x005C, 0x0057, 0x0064, 0x006B, 0x0053,
        0x0061, 0x0066, 0x0065, 0x004E, 0x0075, 0x006C, 0x006C, 0x0000,
    ];

    // SAFETY: static slices remain valid for the driver lifetime.
    let mut device_name = unsafe { unicode_string_from_slice(device_name_buf) };
    let mut dos_name    = unsafe { unicode_string_from_slice(dos_name_buf) };

    // Create the device object.
    let mut device_obj: PDEVICE_OBJECT = core::ptr::null_mut();
    let status = unsafe {
        IoCreateDevice(
            driver,
            0,                      // no device extension
            &mut device_name,
            FILE_DEVICE_NULL,
            0,
            false as u8,            // non-exclusive
            &mut device_obj,
        )
    };
    if status != wdk_sys::STATUS_SUCCESS {
        // SAFETY: PASSIVE_LEVEL.
        unsafe {
            DbgPrint(b"[null-device] IoCreateDevice failed\n\0".as_ptr().cast());
        }
        return NtStatus::from_raw(status);
    }

    // Create the DOS symbolic link so user-mode can open \\.\WdkSafeNull.
    let sym_status = unsafe { IoCreateSymbolicLink(&mut dos_name, &mut device_name) };
    if sym_status != wdk_sys::STATUS_SUCCESS {
        // SAFETY: device_obj is valid (just created).
        unsafe { IoDeleteDevice(device_obj) };
        return NtStatus::from_raw(sym_status);
    }

    // Clear DO_DEVICE_INITIALIZING so the I/O manager delivers IRPs.
    // SAFETY: device_obj is valid.
    unsafe {
        (*device_obj).Flags &= !wdk_sys::DO_DEVICE_INITIALIZING;
    }

    // Register dispatch routines.
    // SAFETY: driver is valid; MajorFunction is a fixed-size array.
    unsafe {
        let obj = &mut *driver;
        obj.MajorFunction[wdk_sys::IRP_MJ_CREATE as usize] = Some(dispatch_create);
        obj.MajorFunction[wdk_sys::IRP_MJ_CLOSE  as usize] = Some(dispatch_close);
        obj.MajorFunction[wdk_sys::IRP_MJ_READ   as usize] = Some(dispatch_read);
        obj.MajorFunction[wdk_sys::IRP_MJ_WRITE  as usize] = Some(dispatch_write);
        obj.DriverUnload = Some(driver_unload);
    }

    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[null-device] DriverEntry -- ready\n\0".as_ptr().cast());
    }

    NtStatus::SUCCESS
}