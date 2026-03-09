// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`IoRequest`] — the primary abstraction for a dispatched I/O request.

use core::marker::PhantomData;

use crate::{ioctl::IoControlCode, irp::Irp, NtStatus};

/// An I/O request received by a dispatch callback.
///
/// Must be completed (via [`complete`] or forwarded via [`into_raw_irp`])
/// before the dispatch routine returns.
///
/// [`complete`]: IoRequest::complete
/// [`into_raw_irp`]: IoRequest::into_raw_irp
#[must_use = "IoRequest must be completed before the dispatch routine returns"]
pub struct IoRequest<'irp> {
    irp:       Irp<'irp>,
    /// Raw pointer to the current `IO_STACK_LOCATION` (opaque on host).
    stack:     *const core::ffi::c_void,
    _not_send: PhantomData<*mut ()>,
}

impl<'irp> IoRequest<'irp> {
    /// Constructs an [`IoRequest`] from raw kernel pointers.
    ///
    /// # Safety
    ///
    /// - `irp` must be non-null, valid, and exclusively owned.
    /// - `stack` must be the current `IO_STACK_LOCATION` for `irp`,
    ///   obtained via `IoGetCurrentIrpStackLocation`.
    #[must_use]
    pub unsafe fn from_raw(
        irp:   *mut core::ffi::c_void,
        stack: *const core::ffi::c_void,
    ) -> Self {
        debug_assert!(!irp.is_null());
        debug_assert!(!stack.is_null());
        Self {
            // SAFETY: caller guarantees irp is valid and exclusively owned.
            irp:       unsafe { Irp::from_raw(irp) },
            stack,
            _not_send: PhantomData,
        }
    }

    /// Returns the IOCTL control code, if this is a `IRP_MJ_DEVICE_CONTROL`.
    ///
    /// The `ioctl_offset` parameter is the byte offset of
    /// `Parameters.DeviceIoControl.IoControlCode` within `IO_STACK_LOCATION`.
    /// Driver code obtains this via `wdk-sys` and passes it through.
    ///
    /// Returns `None` when `code == 0` (not a device-control request).
    #[must_use]
    pub fn ioctl_code_at_offset(&self, ioctl_offset: usize) -> Option<IoControlCode> {
        // SAFETY: stack is valid for 'irp per construction. The caller is
        // responsible for passing the correct offset for the current WDK version.
        let code = unsafe {
            let base = self.stack as *const u8;
            let ptr  = base.add(ioctl_offset) as *const u32;
            *ptr
        };
        if code == 0 { None } else { Some(IoControlCode::from_raw(code)) }
    }

    /// Consumes the request and returns the raw IRP pointer.
    ///
    /// Use **only** when forwarding the IRP to a lower driver via
    /// `IoSkipCurrentIrpStackLocation` + `IoCallDriver`.
    #[must_use = "raw IRP must be forwarded via IoCallDriver or completed"]
    pub fn into_raw_irp(self) -> *mut core::ffi::c_void {
        let raw = self.irp.as_raw_ptr();
        core::mem::forget(self);
        raw
    }

    /// Completes the request with zero bytes transferred.
    pub fn complete(self, status: NtStatus) -> NtStatus {
        self.irp.complete(status)
    }

    /// Sets bytes transferred and completes the request.
    pub fn complete_with_information(
        mut self,
        status: NtStatus,
        bytes:  usize,
        info_ptr: *mut usize,
    ) -> NtStatus {
        self.irp.set_information_ptr(info_ptr, bytes);
        self.irp.complete(status)
    }
}