// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # hid-filter
//!
//! A minimal KMDF HID keyboard upper-filter driver built with `wdk-safe`.
//!
//! ## What this driver does
//!
//! Attaches above the HID keyboard device stack. Every `IRP_MJ_READ` passes
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
//! 2. Build the driver inside an eWDK developer prompt:
//!    ```powershell
//!    cargo make
//!    ```
//! 3. Copy `target\debug\package\` to the VM.
//! 4. Install the driver via Device Manager → Update Driver → Browse for
//!    driver files → point to the `.inf`.
//! 5. On the host, attach WinDbg to the VM kernel.
//! 6. Press any key in the VM — you will see:
//!    ```text
//!    [hid-filter] IRP_MJ_READ -- forwarding down
//!    ```

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

// ── Kernel-mode boilerplate ───────────────────────────────────────────────────

/// Bug-check on panic — the only safe behaviour in kernel mode.
#[cfg(not(test))]
extern crate wdk_panic;

/// Kernel-mode pool allocator for `alloc` types (Vec, Box, …).
#[cfg(not(test))]
use wdk_alloc::WdkAllocator;

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

// ── External crate imports ────────────────────────────────────────────────────

use wdk_safe::{Device, IoRequest, KmdfDriver, NtStatus};
use wdk_sys::{
    ntddk::{
        DbgPrint,
        IoAttachDeviceToDeviceStack,
        IoCallDriver,
        IoCreateDevice,
        IoDeleteDevice,
        IoDetachDevice,
        IoGetCurrentIrpStackLocation,
        IoSkipCurrentIrpStackLocation,
    },
    DEVICE_TYPE,
    DRIVER_OBJECT,
    FILE_DEVICE_KEYBOARD,
    NTSTATUS,
    PCUNICODE_STRING,
    PDEVICE_OBJECT,
    PIRP,
    STATUS_SUCCESS,
    STATUS_UNSUCCESSFUL,
};

// ── KernelCompleter ───────────────────────────────────────────────────────────

/// Concrete [`IrpCompleter`](wdk_safe::IrpCompleter) that calls
/// `IoCompleteRequest` via `wdk-sys`.
///
/// This is a zero-sized type — it adds no overhead to `IoRequest`.
struct KernelCompleter;

impl wdk_safe::IrpCompleter for KernelCompleter {
    /// # Safety
    ///
    /// `irp` must be non-null, valid, and not yet completed.
    unsafe fn complete(irp: *mut core::ffi::c_void, status: i32) {
        // SAFETY: Caller guarantees irp is valid and not yet completed.
        // IO_NO_INCREMENT (0) is appropriate for interactive devices on
        // which no special boost is needed.
        unsafe {
            let pirp = irp.cast::<wdk_sys::IRP>();
            (*pirp).IoStatus.__bindgen_anon_1.Status = status;
            wdk_sys::ntddk::IoCompleteRequest(pirp, wdk_sys::IO_NO_INCREMENT as i8);
        }
    }
}

// ── Driver device extension ───────────────────────────────────────────────────

/// Per-device state stored in `DeviceObject->DeviceExtension`.
///
/// Allocated once in [`add_device`] and freed in [`driver_unload`].
#[repr(C)]
struct FilterDeviceExtension {
    /// The device object one level below us in the filter stack.
    ///
    /// Written once in `add_device`; read-only thereafter.
    lower_device: PDEVICE_OBJECT,
}

// ── KmdfDriver implementation ─────────────────────────────────────────────────

/// The HID keyboard upper-filter driver.
struct HidFilterDriver;

impl KmdfDriver<KernelCompleter> for HidFilterDriver {
    /// `IRP_MJ_CREATE` — a user-mode process opened the keyboard device.
    fn on_create(_device: &Device, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        // SAFETY: DbgPrint is safe at IRQL PASSIVE_LEVEL.
        unsafe {
            DbgPrint(b"[hid-filter] IRP_MJ_CREATE\n\0".as_ptr().cast());
        }
        request.complete(NtStatus::SUCCESS)
    }

    /// `IRP_MJ_CLOSE` — the last handle to the keyboard device was closed.
    fn on_close(_device: &Device, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        // SAFETY: DbgPrint is safe at IRQL PASSIVE_LEVEL.
        unsafe {
            DbgPrint(b"[hid-filter] IRP_MJ_CLOSE\n\0".as_ptr().cast());
        }
        request.complete(NtStatus::SUCCESS)
    }

    /// `IRP_MJ_READ` — a keystroke is arriving from hardware.
    ///
    /// Logs the event and forwards the IRP down the stack unchanged.
    /// The lower driver (`kbdhid.sys`) fills the buffer and completes it.
    fn on_read(device: &Device, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        // SAFETY: DbgPrint is safe at IRQL <= DISPATCH_LEVEL.
        unsafe {
            DbgPrint(
                b"[hid-filter] IRP_MJ_READ -- forwarding down\n\0"
                    .as_ptr()
                    .cast(),
            );
        }

        // Retrieve `lower_device` from the device extension.
        //
        // SAFETY: `device.as_raw_ptr()` is valid for the duration of this
        // dispatch call; `DeviceExtension` was set in `add_device` and is
        // never mutated after that.
        let lower = unsafe {
            let dev_obj = device.as_raw_ptr().cast::<wdk_sys::DEVICE_OBJECT>();
            let ext = (*dev_obj).DeviceExtension.cast::<FilterDeviceExtension>();
            (*ext).lower_device
        };

        // SAFETY: lower is a valid PDEVICE_OBJECT (set in add_device).
        // IRP ownership transfers to IoCallDriver.
        unsafe { forward_irp(request, lower) }
    }

    /// `IRP_MJ_POWER` — pass power IRPs down via `PoCallDriver`.
    ///
    /// A filter driver *must* pass power IRPs through; failing to do so
    /// prevents the system from entering or leaving sleep states.
    fn on_power(device: &Device, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        // SAFETY: same safety reasoning as `on_read`.
        let lower = unsafe {
            let dev_obj = device.as_raw_ptr().cast::<wdk_sys::DEVICE_OBJECT>();
            let ext = (*dev_obj).DeviceExtension.cast::<FilterDeviceExtension>();
            (*ext).lower_device
        };

        // SAFETY: lower is valid; IRP ownership transfers.
        unsafe {
            let irp = request.into_raw_irp().cast::<wdk_sys::IRP>();
            wdk_sys::ntddk::PoStartNextPowerIrp(irp);
            IoSkipCurrentIrpStackLocation(irp);
            let status = wdk_sys::ntddk::PoCallDriver(lower, irp);
            NtStatus::from_raw(status)
        }
    }

    /// `IRP_MJ_PNP` — pass PnP IRPs down.
    ///
    /// A filter driver must forward PnP IRPs it does not handle.
    fn on_pnp(device: &Device, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        let lower = unsafe {
            let dev_obj = device.as_raw_ptr().cast::<wdk_sys::DEVICE_OBJECT>();
            let ext = (*dev_obj).DeviceExtension.cast::<FilterDeviceExtension>();
            (*ext).lower_device
        };

        // SAFETY: lower is valid; IRP ownership transfers.
        unsafe { forward_irp(request, lower) }
    }
}

// ── IRP forwarding ────────────────────────────────────────────────────────────

/// Skips our stack location and calls `IoCallDriver` on `lower`.
///
/// This is the canonical pass-through pattern for a filter that does not
/// need a completion routine.
///
/// # Safety
///
/// - `lower` must be non-null and valid.
/// - `request` must not have been completed.
unsafe fn forward_irp(
    request: IoRequest<'_, KernelCompleter>,
    lower:   PDEVICE_OBJECT,
) -> NtStatus {
    // SAFETY: `into_raw_irp` consumes `request`; we must not touch it again.
    // `IoSkipCurrentIrpStackLocation` reuses our current stack slot for the
    // lower driver, so we must call it before `IoCallDriver`.
    let status = unsafe {
        let irp = request.into_raw_irp().cast::<wdk_sys::IRP>();
        IoSkipCurrentIrpStackLocation(irp);
        IoCallDriver(lower, irp)
    };
    NtStatus::from_raw(status)
}

// ── AddDevice ─────────────────────────────────────────────────────────────────

/// Called by the PnP manager when a new keyboard device is enumerated.
///
/// Creates our filter device object, attaches it above the PDO, and stores
/// the lower device pointer in the device extension.
///
/// # Safety
///
/// `driver` and `pdo` are valid non-null pointers supplied by the I/O manager.
#[unsafe(export_name = "AddDevice")]
pub unsafe extern "system" fn add_device(
    driver: *mut DRIVER_OBJECT,
    pdo:    PDEVICE_OBJECT,
) -> NTSTATUS {
    // SAFETY: `driver` is valid for the duration of the driver load.
    let status = unsafe { add_device_inner(driver, pdo) };
    status.into_raw()
}

/// Safe inner body of [`add_device`].
unsafe fn add_device_inner(
    driver: *mut DRIVER_OBJECT,
    pdo:    PDEVICE_OBJECT,
) -> NtStatus {
    let mut filter_device: PDEVICE_OBJECT = core::ptr::null_mut();

    // Create the filter device object.
    //
    // SAFETY: `driver` is valid; `filter_device` is written by this call.
    let create_status = unsafe {
        IoCreateDevice(
            driver,
            core::mem::size_of::<FilterDeviceExtension>() as u32,
            core::ptr::null_mut(), // no name — filter devices are unnamed
            FILE_DEVICE_KEYBOARD as DEVICE_TYPE,
            0,           // no device characteristics
            false as u8, // non-exclusive
            &mut filter_device,
        )
    };

    if create_status != STATUS_SUCCESS {
        return NtStatus::from_raw(create_status);
    }

    // Attach the filter device above `pdo` and record the lower device.
    //
    // SAFETY: `filter_device` and `pdo` are both valid non-null pointers.
    let lower = unsafe { IoAttachDeviceToDeviceStack(filter_device, pdo) };
    if lower.is_null() {
        // Failed to attach — clean up the device we just created.
        // SAFETY: `filter_device` is valid and was just created.
        unsafe { IoDeleteDevice(filter_device) };
        return NtStatus::UNSUCCESSFUL;
    }

    // Write the lower device pointer into our extension.
    //
    // SAFETY: `filter_device` is valid; `DeviceExtension` points to a
    // `FilterDeviceExtension` we allocated via `IoCreateDevice`.
    unsafe {
        let ext = (*filter_device)
            .DeviceExtension
            .cast::<FilterDeviceExtension>();
        (*ext).lower_device = lower;
    }

    // Copy I/O stack size and flags from the attached device.
    //
    // SAFETY: both pointers are valid.
    unsafe {
        (*filter_device).StackSize = (*lower).StackSize + 1;
        (*filter_device).Flags |= (*lower).Flags
            & (wdk_sys::DO_BUFFERED_IO | wdk_sys::DO_DIRECT_IO);
        // Clear the initialisation flag — the device is now ready.
        (*filter_device).Flags &= !wdk_sys::DO_DEVICE_INITIALIZING;
    }

    // SAFETY: PASSIVE_LEVEL, always safe to call.
    unsafe {
        DbgPrint(b"[hid-filter] AddDevice -- filter attached\n\0".as_ptr().cast());
    }

    NtStatus::SUCCESS
}

// ── DriverUnload ──────────────────────────────────────────────────────────────

/// Called when the driver is unloaded.
///
/// Detaches from the device stack and deletes our filter device object.
///
/// # Safety
///
/// `driver` is valid for the duration of this call.
unsafe extern "system" fn driver_unload(driver: *mut DRIVER_OBJECT) {
    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[hid-filter] DriverUnload -- detaching\n\0".as_ptr().cast());
    }

    // Walk the device object list and detach/delete each filter device we own.
    //
    // SAFETY: `driver` is valid; we only touch `DeviceObject` entries we
    // created in `add_device`.
    unsafe {
        let mut device = (*driver).DeviceObject;
        while !device.is_null() {
            let next = (*device).NextDevice;
            let ext  = (*device).DeviceExtension.cast::<FilterDeviceExtension>();
            let lower = (*ext).lower_device;
            IoDetachDevice(lower);
            IoDeleteDevice(device);
            device = next;
        }
    }
}

// ── Dispatch thunks ───────────────────────────────────────────────────────────

/// Builds an [`IoRequest`] from the raw dispatch arguments and routes it
/// to the correct [`KmdfDriver`] method.
macro_rules! dispatch_thunk {
    ($name:ident, $method:ident) => {
        /// # Safety
        ///
        /// `device` and `irp` are valid non-null pointers supplied by the
        /// I/O manager at the correct IRQL.
        unsafe extern "system" fn $name(
            device: PDEVICE_OBJECT,
            irp:    PIRP,
        ) -> NTSTATUS {
            // SAFETY: pointers are valid per the dispatch callback contract.
            let status = unsafe {
                let stack = IoGetCurrentIrpStackLocation(irp);
                let req   = IoRequest::<KernelCompleter>::from_raw(
                    irp.cast(),
                    stack.cast(),
                );
                let dev = Device::from_raw(device.cast());
                HidFilterDriver::$method(&dev, req)
            };
            status.into_raw()
        }
    };
}

dispatch_thunk!(dispatch_create,  on_create);
dispatch_thunk!(dispatch_close,   on_close);
dispatch_thunk!(dispatch_read,    on_read);
dispatch_thunk!(dispatch_power,   on_power);
dispatch_thunk!(dispatch_pnp,     on_pnp);

// ── DriverEntry ───────────────────────────────────────────────────────────────

/// Windows calls this once when the driver is loaded.
///
/// # Safety
///
/// `driver` and `registry_path` are valid non-null pointers supplied by the
/// I/O manager. This function must be `unsafe extern "system"`.
#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "system" fn driver_entry(
    driver:        *mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    // SAFETY: pointers are valid per the DriverEntry kernel contract.
    unsafe { driver_entry_inner(driver, registry_path).into_raw() }
}

/// Safe inner body of [`driver_entry`].
///
/// Separated from the `unsafe extern "system"` shell so the bulk of
/// initialisation logic lives in a safe function, matching the pattern
/// used across `microsoft/windows-drivers-rs`.
unsafe fn driver_entry_inner(
    driver:         *mut DRIVER_OBJECT,
    _registry_path: PCUNICODE_STRING,
) -> NtStatus {
    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[hid-filter] DriverEntry -- loading\n\0".as_ptr().cast());
    }

    // Register the PnP AddDevice callback.
    //
    // SAFETY: `driver` is valid for the lifetime of the driver.
    unsafe {
        (*driver).DriverExtension.as_mut()
            .expect("DriverExtension must not be null")
            .AddDevice = Some(add_device);
    }

    // Register dispatch routines.
    //
    // SAFETY: `MajorFunction` is a fixed-size array indexed by IRP major
    // function code. Writing to these entries is the documented way to
    // register dispatch routines (WDK: Writing a DriverEntry Routine).
    unsafe {
        let obj = &mut *driver;
        obj.MajorFunction[wdk_sys::IRP_MJ_CREATE  as usize] = Some(dispatch_create);
        obj.MajorFunction[wdk_sys::IRP_MJ_CLOSE   as usize] = Some(dispatch_close);
        obj.MajorFunction[wdk_sys::IRP_MJ_READ    as usize] = Some(dispatch_read);
        obj.MajorFunction[wdk_sys::IRP_MJ_POWER   as usize] = Some(dispatch_power);
        obj.MajorFunction[wdk_sys::IRP_MJ_PNP     as usize] = Some(dispatch_pnp);
        obj.DriverUnload = Some(driver_unload);
    }

    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(
            b"[hid-filter] DriverEntry -- ready\n\0"
                .as_ptr()
                .cast(),
        );
    }

    NtStatus::SUCCESS
}