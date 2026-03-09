// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2025 arelove

//! # hid-filter
//!
//! A minimal KMDF HID keyboard upper-filter driver built with `wdk-safe`.
//!
//! ## What this driver does
//!
//! Attaches above the HID keyboard device stack. Every keystroke passes
//! through [`dispatch_read`] — we log it via `DbgPrint` (visible in WinDbg)
//! and forward the IRP unchanged so Windows still receives the input.
//!
//! ## Device stack
//!
//! ```text
//!  ┌──────────────────────────┐
//!  │   Win32 application      │  user mode
//!  └────────────┬─────────────┘
//!               │  ReadFile
//!  ═════════════════════════════ kernel boundary
//!               │
//!  ┌────────────▼─────────────┐
//!  │  hid-filter.sys          │  ← THIS DRIVER  (upper filter)
//!  │  logs every keystroke    │
//!  │  via DbgPrint to WinDbg  │
//!  └────────────┬─────────────┘
//!               │  IoCallDriver  (IRP forwarded unchanged)
//!  ┌────────────▼─────────────┐
//!  │  kbdhid.sys              │  HID keyboard port driver (Microsoft)
//!  └────────────┬─────────────┘
//!               │
//!  ┌────────────▼─────────────┐
//!  │  USB keyboard hardware   │
//!  └──────────────────────────┘
//! ```
//!
//! ## How to test
//!
//! 1. In Hyper-V test VM, enable test signing once:
//!    ```cmd
//!    bcdedit /set testsigning on
//!    shutdown /r /t 0
//!    ```
//! 2. Build the driver (inside eWDK developer prompt):
//!    ```powershell
//!    cargo make
//!    ```
//! 3. Copy `target\debug\package\` to the VM.
//! 4. Load the driver in the VM:
//!    ```cmd
//!    sc create hid-filter type= kernel binPath= C:\drivers\hid_filter.sys
//!    sc start hid-filter
//!    ```
//! 5. On the host, open WinDbg → attach to kernel of the VM.
//! 6. Press any key in the VM — you will see lines like:
//!    ```
//!    [hid-filter] IRP_MJ_READ -- forwarding down
//!    ```

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

// ── Kernel-mode boilerplate ───────────────────────────────────────────────────

/// On panic in kernel mode, trigger a bug check rather than corrupt state.
#[cfg(not(test))]
extern crate wdk_panic;

/// WDK pool allocator — enables `alloc` types (Vec, Box, …) in kernel mode.
#[cfg(not(test))]
use wdk_alloc::WdkAllocator;

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

// ── Imports ───────────────────────────────────────────────────────────────────

use wdk_safe::{Device, IoRequest, KmdfDriver, NtStatus};
use wdk_sys::{
    ntddk::{
        DbgPrint,
        IoCallDriver,
        IoDeleteDevice,
        IoDetachDevice,
        IoGetCurrentIrpStackLocation,
        IoSkipCurrentIrpStackLocation,
    },
    DRIVER_OBJECT,
    NTSTATUS,
    PCUNICODE_STRING,
    PDEVICE_OBJECT,
    PIRP,
};

// ── Global driver state ───────────────────────────────────────────────────────

/// Pointer to the device object one level below us in the filter stack.
///
/// Set once in `DriverEntry`, read-only thereafter. Safe because:
/// - Written before any dispatch routine can run.
/// - `DriverEntry` is called single-threaded by the kernel.
static mut LOWER_DEVICE: PDEVICE_OBJECT = core::ptr::null_mut();

// ── KmdfDriver implementation ─────────────────────────────────────────────────

/// The HID keyboard upper-filter driver.
struct HidFilterDriver;

impl KmdfDriver for HidFilterDriver {
    /// `IRP_MJ_CREATE` — a user-mode process opened the keyboard device.
    fn on_create(_device: &Device, request: IoRequest<'_>) -> NtStatus {
        // SAFETY: DbgPrint is safe at IRQL PASSIVE_LEVEL.
        unsafe { DbgPrint(b"[hid-filter] IRP_MJ_CREATE\n\0".as_ptr().cast()); }
        request.complete(NtStatus::SUCCESS)
    }

    /// `IRP_MJ_CLOSE` — the last handle to the keyboard device was closed.
    fn on_close(_device: &Device, request: IoRequest<'_>) -> NtStatus {
        // SAFETY: DbgPrint is safe at IRQL PASSIVE_LEVEL.
        unsafe { DbgPrint(b"[hid-filter] IRP_MJ_CLOSE\n\0".as_ptr().cast()); }
        request.complete(NtStatus::SUCCESS)
    }

    /// `IRP_MJ_READ` — a keystroke is arriving from the hardware.
    ///
    /// We log the event and forward the IRP down the stack unchanged.
    /// The lower driver (`kbdhid.sys`) completes it when data is ready.
    fn on_read(_device: &Device, request: IoRequest<'_>) -> NtStatus {
        // SAFETY: DbgPrint is safe at IRQL DISPATCH_LEVEL or below.
        unsafe {
            DbgPrint(
                b"[hid-filter] IRP_MJ_READ -- forwarding down\n\0"
                    .as_ptr()
                    .cast(),
            );
        }

        // SAFETY: LOWER_DEVICE is non-null after DriverEntry and never
        // mutated again. We hand IRP ownership to the lower driver.
        unsafe { forward_irp_down(request) }
    }
}

// ── IRP forwarding ────────────────────────────────────────────────────────────

/// Skips our stack location and forwards the IRP to the lower device.
///
/// `IoSkipCurrentIrpStackLocation` + `IoCallDriver` is the canonical
/// pattern for a pass-through filter that does not need a completion
/// routine.
///
/// # Safety
///
/// - `LOWER_DEVICE` must be non-null and valid.
/// - `request` must not have been completed already.
unsafe fn forward_irp_down(request: IoRequest<'_>) -> NtStatus {
    // SAFETY: into_raw_irp() consumes the request — we must not touch it
    // after this point. Ownership transfers to IoCallDriver.
    let (irp_raw, lower) = unsafe {
        let irp = request.into_raw_irp() as PIRP;
        IoSkipCurrentIrpStackLocation(irp);
        (irp, LOWER_DEVICE)
    };

    // SAFETY: lower is valid (set in DriverEntry, never mutated again).
    // irp_raw is valid (just obtained from into_raw_irp).
    let status = unsafe { IoCallDriver(lower, irp_raw) };
    NtStatus::from_raw(status)
}

// ── DriverEntry ───────────────────────────────────────────────────────────────

/// Windows calls this once when the driver is loaded.
///
/// # Safety
///
/// `driver` and `registry_path` are valid non-null pointers supplied by
/// the I/O manager. This function must be `unsafe extern "system"`.
#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "system" fn driver_entry(
    driver: *mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    // SAFETY: pointers are valid per the DriverEntry kernel contract.
    unsafe { driver_entry_inner(driver, registry_path).into_raw() }
}

/// Safe inner body of `DriverEntry`.
///
/// Separated so the bulk of initialisation logic lives outside the
/// `unsafe extern "system"` boundary.
fn driver_entry_inner(
    driver: *mut DRIVER_OBJECT,
    _registry_path: PCUNICODE_STRING,
) -> NtStatus {
    // SAFETY: DriverEntry always runs at IRQL PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[hid-filter] DriverEntry -- loading\n\0".as_ptr().cast());
    }

    // Register our dispatch routines in the MajorFunction table.
    //
    // SAFETY: driver is a valid DRIVER_OBJECT for the lifetime of the driver.
    // Writing MajorFunction entries is the documented way to register
    // dispatch routines (WDK: Writing a DriverEntry Routine).
    unsafe {
        let obj = &mut *driver;

        obj.MajorFunction[wdk_sys::IRP_MJ_CREATE as usize] = Some(dispatch_create);
        obj.MajorFunction[wdk_sys::IRP_MJ_CLOSE  as usize] = Some(dispatch_close);
        obj.MajorFunction[wdk_sys::IRP_MJ_READ   as usize] = Some(dispatch_read);

        obj.DriverUnload = Some(driver_unload);
    }

    // SAFETY: PASSIVE_LEVEL — same as above.
    unsafe {
        DbgPrint(
            b"[hid-filter] DriverEntry -- dispatch table registered\n\0"
                .as_ptr()
                .cast(),
        );
    }

    NtStatus::SUCCESS
}

// ── Dispatch thunks ───────────────────────────────────────────────────────────
//
// These `extern "system"` functions are stored in DRIVER_OBJECT.MajorFunction.
// Each one converts the raw kernel pointers into safe wdk-safe types and
// delegates to HidFilterDriver.

unsafe extern "system" fn dispatch_create(
    device: PDEVICE_OBJECT,
    irp: PIRP,
) -> NTSTATUS {
    // SAFETY: device and irp are valid for the duration of this dispatch call,
    // guaranteed by the I/O manager.
    let dev = unsafe { Device::from_raw(device.cast()) };
    let req = unsafe {
        IoRequest::from_raw(
            irp.cast(),
            IoGetCurrentIrpStackLocation(irp).cast(),
        )
    };
    HidFilterDriver::on_create(&dev, req).into_raw()
}

unsafe extern "system" fn dispatch_close(
    device: PDEVICE_OBJECT,
    irp: PIRP,
) -> NTSTATUS {
    // SAFETY: same as dispatch_create.
    let dev = unsafe { Device::from_raw(device.cast()) };
    let req = unsafe {
        IoRequest::from_raw(
            irp.cast(),
            IoGetCurrentIrpStackLocation(irp).cast(),
        )
    };
    HidFilterDriver::on_close(&dev, req).into_raw()
}

unsafe extern "system" fn dispatch_read(
    device: PDEVICE_OBJECT,
    irp: PIRP,
) -> NTSTATUS {
    // SAFETY: same as dispatch_create.
    let dev = unsafe { Device::from_raw(device.cast()) };
    let req = unsafe {
        IoRequest::from_raw(
            irp.cast(),
            IoGetCurrentIrpStackLocation(irp).cast(),
        )
    };
    HidFilterDriver::on_read(&dev, req).into_raw()
}

// ── DriverUnload ──────────────────────────────────────────────────────────────

/// Called by Windows when the driver is being unloaded.
///
/// Detaches our filter device from the stack and deletes it.
unsafe extern "system" fn driver_unload(driver: *mut DRIVER_OBJECT) {
    // SAFETY: DriverUnload always runs at IRQL PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[hid-filter] DriverUnload -- unloading\n\0".as_ptr().cast());

        if !LOWER_DEVICE.is_null() {
            let device_obj = (*driver).DeviceObject;
            if !device_obj.is_null() {
                IoDetachDevice(LOWER_DEVICE);
                IoDeleteDevice(device_obj);
            }
        }

        DbgPrint(b"[hid-filter] DriverUnload -- done\n\0".as_ptr().cast());
    }
}