// Copyright (c) 2026 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # ioctl-echo
//!
//! A WDM driver that exposes one IOCTL: `IOCTL_ECHO`.
//!
//! Send it a `u32`, get the same `u32` back. Simple, but it exercises:
//!
//! - [`define_ioctl!`](wdk_safe::define_ioctl) — type-safe IOCTL declaration
//! - [`IoRequest::system_buffer`] — reading the kernel-mapped input buffer
//! - [`IoRequest::complete_with_info`] — returning data + byte count
//! - Full device create/delete lifecycle
//!
//! ## Device path
//!
//! `\\.\WdkSafeEcho` (user-mode) / `\Device\WdkSafeEcho` (kernel-mode)
//!
//! ## Test with PowerShell in the VM
//!
//! ```powershell
//! # After installing + starting the driver:
//! $dev = [System.IO.File]::Open("\\.\WdkSafeEcho",
//!     [System.IO.FileMode]::Open,
//!     [System.IO.FileAccess]::ReadWrite,
//!     [System.IO.FileShare]::None)
//! # Use DeviceIoControl via P/Invoke or a small test tool.
//! ```
//!
//! ## How to build
//!
//! Inside an eWDK developer prompt:
//!
//! ```powershell
//! cd examples/ioctl-echo/ioctl-echo
//! cargo make
//! ```

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

// ── Kernel boilerplate ────────────────────────────────────────────────────────

#[cfg(not(test))]
extern crate wdk_panic;

#[cfg(not(test))]
use wdk_alloc::WdkAllocator;

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

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

use wdk_safe::{
    define_ioctl,
    ioctl::IoStackOffsets,
    Device, IoRequest, NtStatus, WdmDriver,
};
use wdk_sys::{
    ntddk::{
        DbgPrint, IoCreateDevice, IoCreateSymbolicLink, IoDeleteDevice, IoDeleteSymbolicLink,
        IofCompleteRequest,
    },
    DRIVER_OBJECT, FILE_DEVICE_UNKNOWN, NTSTATUS, PCUNICODE_STRING, PDEVICE_OBJECT,
    UNICODE_STRING,
};

// ── FORCEINLINE reimplementation ──────────────────────────────────────────────

/// Reimplements `IoGetCurrentIrpStackLocation`.
///
/// # Safety
///
/// `irp` must be non-null and valid for the dispatch call duration.
#[inline]
unsafe fn irp_current_stack(irp: *mut wdk_sys::IRP) -> *mut wdk_sys::IO_STACK_LOCATION {
    // SAFETY: caller guarantees validity.
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

/// Zero-sized type that calls `IofCompleteRequest`.
struct KernelCompleter;

impl wdk_safe::IrpCompleter for KernelCompleter {
    /// # Safety
    ///
    /// `irp` must be non-null, valid, not yet completed, at `IRQL <=
    /// DISPATCH_LEVEL`.
    unsafe fn complete(irp: *mut core::ffi::c_void, status: i32) {
        // SAFETY: caller upholds the contract.
        unsafe {
            let pirp = irp.cast::<wdk_sys::IRP>();
            (*pirp).IoStatus.__bindgen_anon_1.Status = status;
            IofCompleteRequest(pirp, wdk_sys::IO_NO_INCREMENT as i8);
        }
    }
}

// ── IOCTL definition ──────────────────────────────────────────────────────────

/// Echo request: a single `u32` value.
#[repr(C)]
pub struct EchoRequest {
    /// The value to echo back.
    pub value: u32,
}

/// Echo response: the same `u32` value.
#[repr(C)]
pub struct EchoResponse {
    /// The echoed value.
    pub value: u32,
}

// Declares:
//   pub const IOCTL_ECHO: IoControlCode
//   pub type  IoctlEchoInput  = EchoRequest
//   pub type  IoctlEchoOutput = EchoResponse
//
// Device type 0x8000 = vendor-defined; function 0x800 = first vendor function.
// method = Buffered → I/O manager provides AssociatedIrp.SystemBuffer.
define_ioctl!(IOCTL_ECHO, 0x8000u16, 0x800u16, EchoRequest => EchoResponse);

// ── WdmDriver implementation ──────────────────────────────────────────────────

/// The IOCTL echo driver.
struct IoctlEchoDriver;

impl WdmDriver<KernelCompleter> for IoctlEchoDriver {
    fn on_create(_device: &Device<'_>, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        // SAFETY: PASSIVE_LEVEL.
        unsafe { DbgPrint(b"[ioctl-echo] IRP_MJ_CREATE\n\0".as_ptr().cast()) };
        request.complete(NtStatus::SUCCESS)
    }

    fn on_close(_device: &Device<'_>, request: IoRequest<'_, KernelCompleter>) -> NtStatus {
        // SAFETY: PASSIVE_LEVEL.
        unsafe { DbgPrint(b"[ioctl-echo] IRP_MJ_CLOSE\n\0".as_ptr().cast()) };
        request.complete(NtStatus::SUCCESS)
    }

    /// Handles `IOCTL_ECHO`: reads a `u32` from the input buffer and writes
    /// the same value back to the output buffer.
    ///
    /// Both input and output share `AssociatedIrp.SystemBuffer` for
    /// `METHOD_BUFFERED` — the I/O manager copies from user-mode on entry
    /// and back to user-mode on completion.
    fn on_device_control(
        _device: &Device<'_>,
        request: IoRequest<'_, KernelCompleter>,
    ) -> NtStatus {
        let offsets = &IoStackOffsets::WDK_SYS_0_5_X64;

        // Verify this is our IOCTL.
        let Some(code) = request.ioctl_code(offsets) else {
            return request.complete(NtStatus::INVALID_DEVICE_REQUEST);
        };

        if code != IOCTL_ECHO {
            // SAFETY: PASSIVE_LEVEL.
            unsafe {
                DbgPrint(
                    b"[ioctl-echo] unknown IOCTL code\n\0".as_ptr().cast(),
                );
            }
            return request.complete(NtStatus::INVALID_DEVICE_REQUEST);
        }

        // Validate buffer sizes.
        let in_len  = request.input_buffer_length(offsets);
        let out_len = request.output_buffer_length(offsets);

        if in_len < core::mem::size_of::<EchoRequest>() {
            return request.complete(NtStatus::BUFFER_TOO_SMALL);
        }
        if out_len < core::mem::size_of::<EchoResponse>() {
            return request.complete(NtStatus::BUFFER_TOO_SMALL);
        }

        // Read the echoed value from AssociatedIrp.SystemBuffer.
        //
        // SAFETY: we verified this is METHOD_BUFFERED via define_ioctl!
        // (Buffered is the default). The buffer is valid for the IRP lifetime.
        let value = unsafe {
            let buf_ptr = request.system_buffer(offsets);
            match buf_ptr {
                None => return request.complete(NtStatus::INVALID_PARAMETER),
                Some(ptr) => ptr.cast::<EchoRequest>().read_unaligned().value,
            }
        };

        // SAFETY: PASSIVE_LEVEL.
        unsafe {
            DbgPrint(b"[ioctl-echo] echoing value\n\0".as_ptr().cast());
        }

        // Write the response back into the same SystemBuffer.
        //
        // SAFETY: same buffer, same size guarantees as above.
        // `info_ptr` is the address of IoStatus.Information inside the IRP.
        // The I/O manager will copy `bytes_returned` bytes back to user-mode
        // after IofCompleteRequest.
        let bytes_returned = core::mem::size_of::<EchoResponse>();

        unsafe {
            let buf_ptr = request
                .system_buffer(offsets)
                .expect("checked above")
                .cast::<EchoResponse>();
            buf_ptr.write_unaligned(EchoResponse { value });

            // complete_with_info writes IoStatus.Information = bytes_returned
            // and then calls IofCompleteRequest.
            let info_ptr = request.io_status_information_ptr(offsets);
            request.complete_with_info(NtStatus::SUCCESS, bytes_returned, info_ptr)
        }
    }
}

// ── Dispatch thunks ───────────────────────────────────────────────────────────

wdk_safe::dispatch_fn!(
    dispatch_create = IoctlEchoDriver, on_create, KernelCompleter,
    irp_stack = irp_current_stack
);
wdk_safe::dispatch_fn!(
    dispatch_close = IoctlEchoDriver, on_close, KernelCompleter,
    irp_stack = irp_current_stack
);
wdk_safe::dispatch_fn!(
    dispatch_device_control = IoctlEchoDriver, on_device_control, KernelCompleter,
    irp_stack = irp_current_stack
);

// ── Unicode string helper ─────────────────────────────────────────────────────

/// Constructs a `UNICODE_STRING` from a static UTF-16 null-terminated slice.
///
/// # Safety
///
/// `buf` must remain valid for the lifetime of the returned `UNICODE_STRING`.
#[inline]
unsafe fn unicode_from_slice(buf: &[u16]) -> UNICODE_STRING {
    let len = ((buf.len() - 1) * 2) as u16;
    UNICODE_STRING {
        Length: len,
        MaximumLength: len + 2,
        Buffer: buf.as_ptr().cast_mut(),
    }
}

// ── DriverUnload ──────────────────────────────────────────────────────────────

/// Removes the symbolic link and device object on unload.
///
/// # Safety
///
/// `driver` is valid at `PASSIVE_LEVEL`.
unsafe extern "C" fn driver_unload(driver: *mut DRIVER_OBJECT) {
    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[ioctl-echo] DriverUnload\n\0".as_ptr().cast());
    }

    // \DosDevices\WdkSafeEcho in UTF-16
    let dos_buf: &[u16] = &[
        0x005C,0x0044,0x006F,0x0073,0x0044,0x0065,0x0076,0x0069,
        0x0063,0x0065,0x0073,0x005C,0x0057,0x0064,0x006B,0x0053,
        0x0061,0x0066,0x0065,0x0045,0x0063,0x0068,0x006F,0x0000,
    ];
    // SAFETY: static slice.
    let mut dos_name = unsafe { unicode_from_slice(dos_buf) };
    // SAFETY: PASSIVE_LEVEL.
    unsafe { IoDeleteSymbolicLink(&mut dos_name) };

    // SAFETY: driver is valid.
    unsafe {
        let device = (*driver).DeviceObject;
        if !device.is_null() {
            IoDeleteDevice(device);
        }
    }
}

// ── DriverEntry ───────────────────────────────────────────────────────────────

/// Kernel entry point.
///
/// # Safety
///
/// `driver` and `registry_path` are valid at `PASSIVE_LEVEL`.
#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "system" fn driver_entry(
    driver: *mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    // SAFETY: valid per DriverEntry contract.
    unsafe { driver_entry_inner(driver, registry_path).into_raw() }
}

/// Inner body — creates device, symbolic link, registers routines.
unsafe fn driver_entry_inner(
    driver: *mut DRIVER_OBJECT,
    _registry_path: PCUNICODE_STRING,
) -> NtStatus {
    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[ioctl-echo] DriverEntry -- loading\n\0".as_ptr().cast());
    }

    // \Device\WdkSafeEcho (UTF-16)
    let dev_buf: &[u16] = &[
        0x005C,0x0044,0x0065,0x0076,0x0069,0x0063,0x0065,0x005C,
        0x0057,0x0064,0x006B,0x0053,0x0061,0x0066,0x0065,0x0045,
        0x0063,0x0068,0x006F,0x0000,
    ];
    // \DosDevices\WdkSafeEcho (UTF-16)
    let dos_buf: &[u16] = &[
        0x005C,0x0044,0x006F,0x0073,0x0044,0x0065,0x0076,0x0069,
        0x0063,0x0065,0x0073,0x005C,0x0057,0x0064,0x006B,0x0053,
        0x0061,0x0066,0x0065,0x0045,0x0063,0x0068,0x006F,0x0000,
    ];

    // SAFETY: static slices, valid for driver lifetime.
    let mut dev_name = unsafe { unicode_from_slice(dev_buf) };
    let mut dos_name = unsafe { unicode_from_slice(dos_buf) };

    let mut device_obj: PDEVICE_OBJECT = core::ptr::null_mut();
    let status = unsafe {
        IoCreateDevice(
            driver,
            0,
            &mut dev_name,
            FILE_DEVICE_UNKNOWN,
            0,
            false as u8,
            &mut device_obj,
        )
    };
    if status != wdk_sys::STATUS_SUCCESS {
        return NtStatus::from_raw(status);
    }

    let sym_status = unsafe { IoCreateSymbolicLink(&mut dos_name, &mut dev_name) };
    if sym_status != wdk_sys::STATUS_SUCCESS {
        // SAFETY: device_obj valid.
        unsafe { IoDeleteDevice(device_obj) };
        return NtStatus::from_raw(sym_status);
    }

    // SAFETY: device_obj valid.
    unsafe {
        (*device_obj).Flags &= !wdk_sys::DO_DEVICE_INITIALIZING;
        // DO_BUFFERED_IO: tell the I/O manager to use SystemBuffer for IOCTL.
        (*device_obj).Flags |= wdk_sys::DO_BUFFERED_IO;
    }

    // SAFETY: driver valid, MajorFunction is a fixed-size array.
    unsafe {
        let obj = &mut *driver;
        obj.MajorFunction[wdk_sys::IRP_MJ_CREATE         as usize] = Some(dispatch_create);
        obj.MajorFunction[wdk_sys::IRP_MJ_CLOSE          as usize] = Some(dispatch_close);
        obj.MajorFunction[wdk_sys::IRP_MJ_DEVICE_CONTROL as usize] = Some(dispatch_device_control);
        obj.DriverUnload = Some(driver_unload);
    }

    // SAFETY: PASSIVE_LEVEL.
    unsafe {
        DbgPrint(b"[ioctl-echo] DriverEntry -- ready\n\0".as_ptr().cast());
    }

    NtStatus::SUCCESS
}