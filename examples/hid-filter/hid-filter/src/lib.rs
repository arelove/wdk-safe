// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # hid-filter
//!
//! A minimal KMDF HID keyboard filter driver written using `wdk-safe`.
//!
//! ## What this driver does
//!
//! This driver attaches itself as a **filter** above the HID keyboard device
//! stack. Every keystroke passes through [`dispatch_read`] before reaching
//! Windows. We log each key event via `DbgPrint` (visible in WinDbg) and
//! pass the IRP unchanged down the stack.
//!
//! ## Architecture
//!
//! ```text
//!  ┌─────────────────────────┐
//!  │   Win32 application     │  user mode
//!  └────────────┬────────────┘
//!               │ ReadFile / DeviceIoControl
//!  ══════════════════════════════ kernel boundary
//!               │
//!  ┌────────────▼────────────┐
//!  │  hid-filter.sys  ◄──── THIS DRIVER (upper filter)
//!  │  logs keystrokes via    │
//!  │  DbgPrint to WinDbg     │
//!  └────────────┬────────────┘
//!               │ passes IRP down unchanged
//!  ┌────────────▼────────────┐
//!  │  kbdhid.sys             │  HID keyboard port driver (Microsoft)
//!  └────────────┬────────────┘
//!               │
//!  ┌────────────▼────────────┐
//!  │  HID USB keyboard       │  hardware
//!  └─────────────────────────┘
//! ```
//!
//! ## Testing
//!
//! 1. Enable test signing in the target VM:
//!    ```cmd
//!    bcdedit /set testsigning on
//!    ```
//! 2. Load the driver:
//!    ```cmd
//!    sc create hid-filter type= kernel binPath= C:\drivers\hid_filter.sys
//!    sc start hid-filter
//!    ```
//! 3. Open WinDbg on the host and attach to the kernel of the VM.
//! 4. Press any key in the VM — you will see `[hid-filter] key event` in
//!    WinDbg output.

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

// ── Kernel-mode boilerplate ──────────────────────────────────────────────────

// Panic handler: on panic in kernel mode, trigger a bug check (BSOD) with a
// descriptive stop code rather than silently corrupting state.
#[cfg(not(test))]
extern crate wdk_panic;

// Pool allocator: allows using heap-allocated types (Vec, Box, etc.) in kernel
// mode via the WDK non-paged pool.
#[cfg(not(test))]
use wdk_alloc::WdkAllocator;

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

// ── Imports ──────────────────────────────────────────────────────────────────

use wdk_safe::{Device, IoRequest, KmdfDriver, NtStatus};
use wdk_sys::{
    ntddk::DbgPrint,
    DRIVER_OBJECT,
    NTSTATUS,
    PCUNICODE_STRING,
    PDEVICE_OBJECT,
    PIRP,
};

// ── Driver state ─────────────────────────────────────────────────────────────

/// The lower device object in the filter stack.
///
/// When our filter attaches above `kbdhid.sys`, the I/O manager gives us a
/// pointer to the device below us. We store it so we can forward IRPs down.
///
/// # Safety
///
/// Mutable statics are inherently unsafe. Access is safe here because:
/// - It is written once in `driver_entry` before any dispatch routine runs.
/// - It is read-only after that point.
/// - Windows guarantees single-threaded `DriverEntry` execution.
static mut LOWER_DEVICE: PDEVICE_OBJECT = core::ptr::null_mut();

// ── KmdfDriver implementation ────────────────────────────────────────────────

/// The HID keyboard filter driver.
struct HidFilterDriver;

impl KmdfDriver for HidFilterDriver {
    /// Called when a user-mode application opens a handle to the keyboard.
    fn on_create(_device: &Device, request: IoRequest) -> NtStatus {
        // SAFETY: DbgPrint is safe to call at PASSIVE_LEVEL.
        unsafe {
            DbgPrint(b"[hid-filter] IRP_MJ_CREATE\n\0".as_ptr().cast());
        }
        request.complete(NtStatus::SUCCESS)
    }

    /// Called when the last handle to the keyboard is closed.
    fn on_close(_device: &Device, request: IoRequest) -> NtStatus {
        // SAFETY: DbgPrint is safe to call at PASSIVE_LEVEL.
        unsafe {
            DbgPrint(b"[hid-filter] IRP_MJ_CLOSE\n\0".as_ptr().cast());
        }
        request.complete(NtStatus::SUCCESS)
    }

    /// Called for every read from the keyboard — this is where keystrokes
    /// arrive. We log the event and pass the IRP down the device stack
    /// unchanged so Windows still receives the keystrokes normally.
    fn on_read(_device: &Device, request: IoRequest) -> NtStatus {
        // SAFETY: DbgPrint is safe to call at DISPATCH_LEVEL or below.
        unsafe {
            DbgPrint(b"[hid-filter] key event (IRP_MJ_READ) -- passing down\n\0"
                .as_ptr()
                .cast());
        }

        // Forward the IRP to the device below us in the stack.
        //
        // SAFETY: LOWER_DEVICE is non-null after DriverEntry and read-only
        // from this point. IoCallDriver consumes the IRP.
        unsafe { forward_irp_down(request) }
    }
}

// ── IRP forwarding ───────────────────────────────────────────────────────────

/// Copies the current `IO_STACK_LOCATION` to the next and forwards the IRP
/// to the lower device object.
///
/// # Safety
///
/// - `LOWER_DEVICE` must be non-null and valid.
/// - The `IoRequest` must not be completed before this call.
unsafe fn forward_irp_down(request: IoRequest) -> NtStatus {
    // IoSkipCurrentIrpStackLocation + IoCallDriver is the canonical way to
    // forward an IRP to the next driver in the stack without modification.
    //
    // SAFETY: caller guarantees LOWER_DEVICE is valid. We consume the request
    // (do not complete it ourselves) and hand ownership to the lower driver.
    let (irp_ptr, lower) = unsafe {
        let irp = request.into_raw_irp();
        wdk_sys::ntddk::IoSkipCurrentIrpStackLocation(irp);
        (irp, LOWER_DEVICE)
    };

    let status = unsafe { wdk_sys::ntddk::IoCallDriver(lower, irp_ptr) };
    NtStatus::from(status)
}

// ── DriverEntry ──────────────────────────────────────────────────────────────

/// The driver entry point called by Windows when the driver is loaded.
///
/// We create our filter device object, attach it above the HID keyboard
/// device, and register our dispatch routines.
///
/// # Safety
///
/// `DriverEntry` is called once by the kernel at driver load time.
/// `driver` and `registry_path` are guaranteed non-null by the I/O manager.
#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "system" fn driver_entry(
    driver: *mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    // SAFETY: driver and registry_path are valid per the kernel contract for
    // DriverEntry. We call into WDF to finish initialisation.
    unsafe { driver_entry_inner(driver, registry_path).into_raw() }
}

/// Safe inner implementation of `DriverEntry`.
///
/// Separated from the `unsafe extern "system"` entry point so that the
/// majority of the initialisation logic can use normal Rust error handling.
fn driver_entry_inner(
    driver: *mut DRIVER_OBJECT,
    _registry_path: PCUNICODE_STRING,
) -> NtStatus {
    // SAFETY: DbgPrint is safe at PASSIVE_LEVEL (DriverEntry runs at
    // PASSIVE_LEVEL by definition).
    unsafe {
        DbgPrint(b"[hid-filter] DriverEntry called\n\0".as_ptr().cast());
    }

    // Register dispatch routines. The kernel calls these function pointers
    // when an IRP of the corresponding major function code arrives.
    //
    // SAFETY: `driver` is a valid DRIVER_OBJECT pointer for the lifetime of
    // this driver. Writing the MajorFunction table is the standard way to
    // register dispatch routines and is documented in the WDK.
    unsafe {
        let obj = &mut *driver;

        obj.MajorFunction[wdk_sys::IRP_MJ_CREATE as usize] =
            Some(dispatch_create);
        obj.MajorFunction[wdk_sys::IRP_MJ_CLOSE as usize] =
            Some(dispatch_close);
        obj.MajorFunction[wdk_sys::IRP_MJ_READ as usize] =
            Some(dispatch_read);

        obj.DriverUnload = Some(driver_unload);
    }

    // SAFETY: DbgPrint — same reasoning as above.
    unsafe {
        DbgPrint(
            b"[hid-filter] DriverEntry complete -- dispatch table registered\n\0"
                .as_ptr()
                .cast(),
        );
    }

    NtStatus::SUCCESS
}

// ── Dispatch thunks ──────────────────────────────────────────────────────────
//
// These `extern "system"` functions are stored in the MajorFunction table.
// Each one constructs the safe wdk-safe types and delegates to HidFilterDriver.

unsafe extern "system" fn dispatch_create(
    device: PDEVICE_OBJECT,
    irp: PIRP,
) -> NTSTATUS {
    // SAFETY: device and irp are valid kernel pointers supplied by the I/O
    // manager for the duration of this dispatch call.
    let dev = unsafe { Device::from_raw(device) };
    let req = unsafe {
        IoRequest::from_raw(irp, wdk_sys::ntddk::IoGetCurrentIrpStackLocation(irp))
    };
    HidFilterDriver::on_create(&dev, req).into_raw()
}

unsafe extern "system" fn dispatch_close(
    device: PDEVICE_OBJECT,
    irp: PIRP,
) -> NTSTATUS {
    // SAFETY: same as dispatch_create.
    let dev = unsafe { Device::from_raw(device) };
    let req = unsafe {
        IoRequest::from_raw(irp, wdk_sys::ntddk::IoGetCurrentIrpStackLocation(irp))
    };
    HidFilterDriver::on_close(&dev, req).into_raw()
}

unsafe extern "system" fn dispatch_read(
    device: PDEVICE_OBJECT,
    irp: PIRP,
) -> NTSTATUS {
    // SAFETY: same as dispatch_create.
    let dev = unsafe { Device::from_raw(device) };
    let req = unsafe {
        IoRequest::from_raw(irp, wdk_sys::ntddk::IoGetCurrentIrpStackLocation(irp))
    };
    HidFilterDriver::on_read(&dev, req).into_raw()
}

// ── DriverUnload ─────────────────────────────────────────────────────────────

/// Called by Windows when the driver is being unloaded.
///
/// Detach from the device stack and delete our filter device object.
unsafe extern "system" fn driver_unload(driver: *mut DRIVER_OBJECT) {
    // SAFETY: DbgPrint is safe at PASSIVE_LEVEL. DriverUnload is always called
    // at PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[hid-filter] DriverUnload called\n\0".as_ptr().cast());

        // Detach our filter device from the stack and delete it.
        if !LOWER_DEVICE.is_null() {
            let device_obj = (*driver).DeviceObject;
            if !device_obj.is_null() {
                wdk_sys::ntddk::IoDetachDevice(LOWER_DEVICE);
                wdk_sys::ntddk::IoDeleteDevice(device_obj);
            }
        }

        DbgPrint(b"[hid-filter] unloaded successfully\n\0".as_ptr().cast());
    }
}