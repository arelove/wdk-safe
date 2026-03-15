// Copyright (c) 2026 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # hid-filter
//!
//! A minimal WDM HID keyboard upper-filter driver built with `wdk-safe`.
//!
//! ## What this driver does
//!
//! Attaches above the HID keyboard device stack. Every `IRP_MJ_READ` passes
//! through [`dispatch_read`] — it logs via `DbgPrint` (visible in `WinDbg` /
//! DebugView) and forwards the IRP unchanged so Windows still receives input.
//!
//! ## Device stack position
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
//!  │  logs each IRP_MJ_READ   │
//!  │  forwards IRP unchanged  │
//!  └────────────┬─────────────┘
//!               │  IofCallDriver
//!  ┌────────────▼─────────────┐
//!  │  kbdhid.sys / hidclass   │  HID class / port driver (Microsoft)
//!  └──────────────────────────┘
//! ```
//!
//! ## How to build and test
//!
//! See `CONTRIBUTING.md` for the full setup.
//!
//! Short version (inside an eWDK developer prompt):
//!
//! ```powershell
//! cd examples/hid-filter/hid-filter
//! cargo make           # build + package debug
//! cargo make --release # build + package release
//! ```

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

// ── Kernel-mode boilerplate
// ───────────────────────────────────────────────────

/// Bug-check on panic — the only safe behaviour in kernel mode (no unwinding).
#[cfg(not(test))]
extern crate wdk_panic;

/// Kernel non-paged pool allocator for `alloc` types (`Box`, `Vec`, …).
#[cfg(not(test))]
use wdk_alloc::WdkAllocator;

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

// ── compiler_builtins float stubs
// ─────────────────────────────────────────────
//
// `compiler_builtins 0.1.160+` references `fma` / `fmaf` (C runtime symbols)
// on x86-64 MSVC. These symbols do not exist in kernel-mode import libraries
// (ntoskrnl.lib / hal.lib), causing LNK2019.
//
// Fix: build.rs emits `/ALTERNATENAME:fma=__wdk_fma_stub` which redirects the
// linker to the no-op stubs below. These are never called at runtime — the
// references are dead code from a generic trait impl.
#[no_mangle]
#[doc(hidden)]
pub unsafe extern "C" fn __wdk_fma_stub(x: f64, y: f64, z: f64) -> f64 {
    // SAFETY: never called. Satisfies linker only.
    x * y + z
}

#[no_mangle]
#[doc(hidden)]
pub unsafe extern "C" fn __wdk_fmaf_stub(x: f32, y: f32, z: f32) -> f32 {
    // SAFETY: never called. Satisfies linker only.
    x * y + z
}

use wdk_safe::{Device, IoRequest, NtStatus, WdmDriver};
use wdk_sys::{
    ntddk::{
        DbgPrint, IoAttachDeviceToDeviceStack, IoCreateDevice, IoDeleteDevice, IoDetachDevice,
        IofCallDriver, IofCompleteRequest, PoCallDriver, PoStartNextPowerIrp,
    },
    DRIVER_OBJECT, FILE_DEVICE_KEYBOARD, NTSTATUS, PCUNICODE_STRING, PDEVICE_OBJECT, PIRP,
};

// ── FORCEINLINE macro reimplementations
// ───────────────────────────────────────
//
// `IoGetCurrentIrpStackLocation` and `IoSkipCurrentIrpStackLocation` are
// FORCEINLINE macros in WDK C headers. bindgen cannot emit inline functions,
// so wdk-sys 0.5.x does not export them. We reimplement them from the
// documented NT IRP layout (identical on x64 and ARM64).

/// Returns a pointer to the current `IO_STACK_LOCATION` for `irp`.
///
/// Equivalent to the WDK macro `IoGetCurrentIrpStackLocation(Irp)`.
///
/// # Safety
///
/// `irp` must be non-null and valid for the duration of the dispatch call.
///
/// # IRQL
///
/// No constraint — pure pointer arithmetic.
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

/// Reuses our `IO_STACK_LOCATION` slot for the next driver down the stack.
///
/// Equivalent to the WDK macro `IoSkipCurrentIrpStackLocation(Irp)`.
///
/// This is the first step of the pass-through pattern:
///
/// ```text
/// irp_skip_current_stack(irp);   // reuse our slot
/// IofCallDriver(lower, irp);     // hand IRP to lower driver
/// ```
///
/// # Safety
///
/// - `irp` must be non-null and valid.
/// - The IRP must have at least one remaining stack slot (`CurrentLocation <
///   StackCount`).
///
/// # IRQL
///
/// No constraint — pure pointer arithmetic.
#[inline]
unsafe fn irp_skip_current_stack(irp: *mut wdk_sys::IRP) {
    // SAFETY: caller guarantees validity and remaining stack depth.
    unsafe {
        (*irp).CurrentLocation += 1;
        (*irp)
            .Tail
            .Overlay
            .__bindgen_anon_2
            .__bindgen_anon_1
            .CurrentStackLocation = (*irp)
            .Tail
            .Overlay
            .__bindgen_anon_2
            .__bindgen_anon_1
            .CurrentStackLocation
            .add(1);
    }
}

// ── KernelCompleter
// ───────────────────────────────────────────────────────────

/// Concrete [`IrpCompleter`](wdk_safe::IrpCompleter) that calls
/// `IofCompleteRequest` via `wdk-sys`.
///
/// Zero-sized type — adds no overhead to `IoRequest<KernelCompleter>`.
struct KernelCompleter;

impl wdk_safe::IrpCompleter for KernelCompleter {
    /// # Safety
    ///
    /// - `irp` must be non-null, valid, and not yet completed.
    /// - Must be called at `IRQL <= DISPATCH_LEVEL`.
    unsafe fn complete(irp: *mut core::ffi::c_void, status: i32) {
        // SAFETY: Caller upholds the IrpCompleter contract.
        // `IO_NO_INCREMENT` (0) means no thread priority boost — appropriate
        // for a filter that doesn't own the data source.
        unsafe {
            let pirp = irp.cast::<wdk_sys::IRP>();
            (*pirp).IoStatus.__bindgen_anon_1.Status = status;
            IofCompleteRequest(pirp, wdk_sys::IO_NO_INCREMENT as i8);
        }
    }
}

// ── Device extension
// ──────────────────────────────────────────────────────────

/// Per-device state stored in `DeviceObject->DeviceExtension`.
///
/// Allocated once in [`add_device`] by `IoCreateDevice` and freed implicitly
/// when `IoDeleteDevice` is called in [`driver_unload`].
#[repr(C)]
struct FilterDeviceExtension {
    /// The device object one level below us in the filter stack.
    ///
    /// Written once in `add_device` and read-only thereafter — no
    /// synchronisation needed.
    lower_device: PDEVICE_OBJECT,
}

// ── WdmDriver implementation
// ──────────────────────────────────────────────────

/// The HID keyboard upper-filter driver.
struct HidFilterDriver;

impl WdmDriver<KernelCompleter> for HidFilterDriver {
    /// `IRP_MJ_CREATE` — a process opened the keyboard device.
    ///
    /// # IRQL
    ///
    /// `PASSIVE_LEVEL`.
    fn on_create(device: &Device<'_>, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        unsafe { DbgPrint(b"[hid-filter] IRP_MJ_CREATE\n\0".as_ptr().cast()) };
        let lower = get_lower_device(device);
        unsafe { forward_irp(request, lower) }
    }

    /// `IRP_MJ_CLOSE` — the last handle to the keyboard device was closed.
    ///
    /// # IRQL
    ///
    /// `PASSIVE_LEVEL`.
    fn on_close(device: &Device<'_>, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        unsafe { DbgPrint(b"[hid-filter] IRP_MJ_CLOSE\n\0".as_ptr().cast()) };
        let lower = get_lower_device(device);
        unsafe { forward_irp(request, lower) }
    }

    fn on_internal_device_control(
        device: &Device<'_>,
        request: IoRequest<'_, KernelCompleter>,
    ) -> NtStatus {
        let lower = get_lower_device(device);
        unsafe { forward_irp(request, lower) }
    }

    fn on_cleanup(device: &Device<'_>, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        let lower = get_lower_device(device);
        unsafe { forward_irp(request, lower) }
    }

    /// `IRP_MJ_READ` — a keystroke is arriving from hardware.
    ///
    /// Logs the event and forwards the IRP down the stack unchanged. The lower
    /// driver (`kbdhid.sys`) fills the keystroke buffer and completes the IRP.
    ///
    /// # IRQL
    ///
    /// `<= DISPATCH_LEVEL`.
    fn on_read(device: &Device<'_>, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        // SAFETY: DbgPrint is safe at IRQL <= DISPATCH_LEVEL.
        unsafe {
            DbgPrint(
                b"[hid-filter] IRP_MJ_READ -- forwarding\n\0"
                    .as_ptr()
                    .cast(),
            );
        }
        let lower = get_lower_device(device);
        // SAFETY: `lower` is valid per the add_device invariant.
        unsafe { forward_irp(request, lower) }
    }

    /// `IRP_MJ_POWER` — system/device power state transition.
    ///
    /// A filter driver **must** pass power IRPs through; blocking them
    /// prevents the system from entering or leaving sleep states.
    ///
    /// # IRQL
    ///
    /// `PASSIVE_LEVEL` or `DISPATCH_LEVEL` depending on the power IRP type.
    fn on_power(device: &Device<'_>, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        let lower = get_lower_device(device);
        // SAFETY: `lower` is valid per the add_device invariant.
        // IRP ownership transfers to PoCallDriver.
        unsafe {
            let irp = request.into_raw_irp().cast::<wdk_sys::IRP>();
            PoStartNextPowerIrp(irp);
            // SAFETY: irp is valid with stack depth > 0.
            irp_skip_current_stack(irp);
            let status = PoCallDriver(lower, irp);
            NtStatus::from_raw(status)
        }
    }

    /// `IRP_MJ_PNP` — Plug-and-Play request.
    ///
    /// A filter driver must forward `PnP` IRPs it does not handle.
    ///
    /// # IRQL
    ///
    /// `PASSIVE_LEVEL`.
    fn on_pnp(device: &Device<'_>, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        let lower = get_lower_device(device);
        // SAFETY: `lower` is valid per the add_device invariant.
        unsafe { forward_irp(request, lower) }
    }
}

// ── Helpers
// ───────────────────────────────────────────────────────────────────

/// Reads `lower_device` from `device`'s extension.
///
/// # IRQL
///
/// No constraint — pure memory read.
fn get_lower_device(device: &Device<'_>) -> PDEVICE_OBJECT {
    // SAFETY: `device.as_raw_ptr()` is a valid `DEVICE_OBJECT` for the
    // duration of this dispatch call. `DeviceExtension` was set in
    // `add_device` and is never mutated afterwards.
    unsafe {
        let dev_obj = device.as_raw_ptr().cast::<wdk_sys::DEVICE_OBJECT>();
        let ext = (*dev_obj).DeviceExtension.cast::<FilterDeviceExtension>();
        (*ext).lower_device
    }
}

/// Reuses our stack slot and calls `IofCallDriver` on `lower`.
///
/// # Safety
///
/// - `lower` must be non-null and valid.
/// - `request` must not have been completed.
///
/// # IRQL
///
/// `<= DISPATCH_LEVEL`.
unsafe fn forward_irp(request: IoRequest<'_, KernelCompleter>, lower: PDEVICE_OBJECT) -> NtStatus {
    // SAFETY: `into_raw_irp` consumes `request`; ownership transfers to the
    // lower driver. `irp_skip_current_stack` must be called before
    // `IofCallDriver` so the lower driver sees its own stack slot.
    let status = unsafe {
        let irp = request.into_raw_irp().cast::<wdk_sys::IRP>();
        irp_skip_current_stack(irp);
        IofCallDriver(lower, irp)
    };
    NtStatus::from_raw(status)
}

// ── Dispatch thunks
// ───────────────────────────────────────────────────────────
//
// `dispatch_fn!` generates `unsafe extern "C" fn(PDEVICE_OBJECT, PIRP) ->
// NTSTATUS`. The `extern "C"` ABI is required because
// DRIVER_OBJECT.MajorFunction entries are typed `unsafe extern "C"` on MSVC
// kernel targets.

wdk_safe::dispatch_fn!(
    dispatch_create = HidFilterDriver,
    on_create,
    KernelCompleter,
    irp_stack = irp_current_stack
);
wdk_safe::dispatch_fn!(
    dispatch_close = HidFilterDriver,
    on_close,
    KernelCompleter,
    irp_stack = irp_current_stack
);
wdk_safe::dispatch_fn!(
    dispatch_read = HidFilterDriver,
    on_read,
    KernelCompleter,
    irp_stack = irp_current_stack
);
wdk_safe::dispatch_fn!(
    dispatch_power = HidFilterDriver,
    on_power,
    KernelCompleter,
    irp_stack = irp_current_stack
);
wdk_safe::dispatch_fn!(
    dispatch_pnp = HidFilterDriver,
    on_pnp,
    KernelCompleter,
    irp_stack = irp_current_stack
);

wdk_safe::dispatch_fn!(
    dispatch_passthrough = HidFilterDriver,
    on_pnp,
    KernelCompleter,
    irp_stack = irp_current_stack
);

// ── AddDevice
// ─────────────────────────────────────────────────────────────────

/// Called by the PnP manager when a new keyboard device is enumerated.
///
/// Creates our filter device object, attaches it above the PDO, and stores
/// the lower device pointer in the device extension.
///
/// # Safety
///
/// `driver` and `pdo` are valid non-null pointers supplied by the I/O manager
/// at `IRQL == PASSIVE_LEVEL`.
#[unsafe(export_name = "AddDevice")]
pub unsafe extern "C" fn add_device(driver: *mut DRIVER_OBJECT, pdo: PDEVICE_OBJECT) -> NTSTATUS {
    // SAFETY: pointers are valid per the AddDevice kernel contract.
    unsafe { add_device_inner(driver, pdo).into_raw() }
}

/// Safe inner body of [`add_device`].
unsafe fn add_device_inner(driver: *mut DRIVER_OBJECT, pdo: PDEVICE_OBJECT) -> NtStatus {
    let mut filter_device: PDEVICE_OBJECT = core::ptr::null_mut();

    // Create the filter device object with enough extension storage for
    // `FilterDeviceExtension`.
    //
    // SAFETY: `driver` is valid; `filter_device` is written by this call.
    let create_status = unsafe {
        IoCreateDevice(
            driver,
            core::mem::size_of::<FilterDeviceExtension>() as u32,
            core::ptr::null_mut(), // no name — unnamed filter device
            FILE_DEVICE_KEYBOARD,
            0,
            false as u8, // non-exclusive
            &mut filter_device,
        )
    };

    if create_status != wdk_sys::STATUS_SUCCESS {
        return NtStatus::from_raw(create_status);
    }

    // Attach our filter device above `pdo` to get the lower device pointer.
    //
    // SAFETY: `filter_device` and `pdo` are both valid non-null pointers.
    let lower = unsafe { IoAttachDeviceToDeviceStack(filter_device, pdo) };
    if lower.is_null() {
        // Attachment failed — clean up the device we just created.
        // SAFETY: `filter_device` is valid and was just created.
        unsafe { IoDeleteDevice(filter_device) };
        return NtStatus::UNSUCCESSFUL;
    }

    // Store the lower device pointer in our extension.
    //
    // SAFETY: `DeviceExtension` points to a `FilterDeviceExtension` we
    // allocated via `IoCreateDevice` above.
    unsafe {
        let ext = (*filter_device)
            .DeviceExtension
            .cast::<FilterDeviceExtension>();
        (*ext).lower_device = lower;
    }

    // Copy the I/O stack size and buffer flags from the lower device so the
    // I/O manager allocates the right number of stack locations and uses the
    // same buffering strategy as the device below us.
    //
    // SAFETY: both pointers are valid for the duration of AddDevice.
    unsafe {
        (*filter_device).StackSize = (*lower).StackSize + 1;
        (*filter_device).Flags |=
            (*lower).Flags & (wdk_sys::DO_BUFFERED_IO | wdk_sys::DO_DIRECT_IO);
        // Clear DO_DEVICE_INITIALIZING — the device is ready to receive IRPs.
        (*filter_device).Flags &= !wdk_sys::DO_DEVICE_INITIALIZING;
    }

    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(
            b"[hid-filter] AddDevice -- filter attached\n\0"
                .as_ptr()
                .cast(),
        );
    }

    NtStatus::SUCCESS
}

// ── DriverUnload
// ──────────────────────────────────────────────────────────────

/// Called when the driver is unloaded.
///
/// Detaches from each device stack and deletes our filter device objects.
///
/// # Safety
///
/// `driver` is a valid non-null pointer supplied by the I/O manager at
/// `IRQL == PASSIVE_LEVEL`.
unsafe extern "C" fn driver_unload(driver: *mut DRIVER_OBJECT) {
    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(
            b"[hid-filter] DriverUnload -- detaching\n\0"
                .as_ptr()
                .cast(),
        );
    }

    // Walk the device object list and detach/delete each filter device we own.
    //
    // SAFETY: `driver` is valid. We only touch `DeviceObject` entries we
    // created in `add_device`.
    unsafe {
        let mut device = (*driver).DeviceObject;
        while !device.is_null() {
            let next = (*device).NextDevice;
            let ext = (*device).DeviceExtension.cast::<FilterDeviceExtension>();
            let lower = (*ext).lower_device;
            IoDetachDevice(lower);
            IoDeleteDevice(device);
            device = next;
        }
    }
}

// ── DriverEntry
// ───────────────────────────────────────────────────────────────

/// Entry point called by the I/O manager when the driver is loaded.
///
/// Registers dispatch routines, the AddDevice callback, and the unload
/// routine.
///
/// # Safety
///
/// `driver` and `registry_path` are valid non-null pointers supplied by the
/// I/O manager at `IRQL == PASSIVE_LEVEL`.
#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "system" fn driver_entry(
    driver: *mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    // SAFETY: pointers are valid per the DriverEntry kernel contract.
    unsafe { driver_entry_inner(driver, registry_path).into_raw() }
}

/// Safe inner body of [`driver_entry`].
///
/// Separated from the `unsafe extern "system"` shell so the bulk of
/// initialisation logic lives in a safe function — matching the pattern
/// used in `microsoft/windows-drivers-rs`.
unsafe fn driver_entry_inner(
    driver: *mut DRIVER_OBJECT,
    _registry_path: PCUNICODE_STRING,
) -> NtStatus {
    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[hid-filter] DriverEntry -- loading\n\0".as_ptr().cast());
    }

    // Register the PnP AddDevice callback.
    //
    // SAFETY: `driver` is valid for the lifetime of the driver. The I/O
    // manager guarantees `DriverExtension` is non-null for a successfully
    // loaded driver; returning STATUS_UNSUCCESSFUL here causes a clean
    // failure rather than a bug check.
    let driver_ext = unsafe {
        match (*driver).DriverExtension.as_mut() {
            Some(ext) => ext,
            None => return NtStatus::UNSUCCESSFUL,
        }
    };
    driver_ext.AddDevice = Some(add_device);

    // Register dispatch routines.
    //
    // SAFETY: `MajorFunction` is a fixed-size array indexed by IRP major
    // function code. Writing to these entries is the documented way to
    // register dispatch routines.
    unsafe {
        let obj = &mut *driver;
        // Заполнить все слоты passthrough-форвардом
        for i in 0..(wdk_sys::IRP_MJ_MAXIMUM_FUNCTION as usize + 1) {
            if obj.MajorFunction[i].is_none() {
                obj.MajorFunction[i] = Some(dispatch_passthrough);
            }
        }
        obj.MajorFunction[wdk_sys::IRP_MJ_CREATE as usize] = Some(dispatch_create);
        obj.MajorFunction[wdk_sys::IRP_MJ_CLOSE as usize] = Some(dispatch_close);
        obj.MajorFunction[wdk_sys::IRP_MJ_READ as usize] = Some(dispatch_read);
        obj.MajorFunction[wdk_sys::IRP_MJ_INTERNAL_DEVICE_CONTROL as usize] =
            Some(dispatch_passthrough);
        obj.MajorFunction[wdk_sys::IRP_MJ_POWER as usize] = Some(dispatch_power);
        obj.MajorFunction[wdk_sys::IRP_MJ_PNP as usize] = Some(dispatch_pnp);
        obj.DriverUnload = Some(driver_unload);
    }
    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[hid-filter] DriverEntry -- ready\n\0".as_ptr().cast());
    }

    NtStatus::SUCCESS
}
