// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`IoRequest`] — the primary abstraction for a dispatched I/O request.
//!
//! `IoRequest<C>` wraps an [`Irp<C>`] together with the current
//! `IO_STACK_LOCATION` pointer. The `C` type parameter is the
//! [`IrpCompleter`] that supplies `IoCompleteRequest`; it is a zero-sized
//! type, so the wrapper is the same size as two raw pointers.
//!
//! [`IrpCompleter`]: crate::irp::IrpCompleter

use core::marker::PhantomData;

use crate::{
    ioctl::IoControlCode,
    irp::{Irp, IrpCompleter},
    NtStatus,
};

/// An I/O request received in a dispatch callback.
///
/// Must be **exactly once** either:
///
/// - Completed via [`complete`] / [`complete_with_information`], or
/// - Forwarded via [`into_raw_irp`].
///
/// `#[must_use]` and the drop bomb on the inner [`Irp`] make both
/// mistakes compile-time or debug-time errors.
///
/// # Type parameter
///
/// `C: IrpCompleter` — the zero-sized type that calls `IoCompleteRequest`.
/// Production drivers use a concrete type from their own crate; tests use
/// [`NoopCompleter`](crate::irp::NoopCompleter).
///
/// [`complete`]: IoRequest::complete
/// [`complete_with_information`]: IoRequest::complete_with_information
/// [`into_raw_irp`]: IoRequest::into_raw_irp
#[must_use = "IoRequest must be completed or forwarded before the dispatch routine returns"]
pub struct IoRequest<'irp, C: IrpCompleter> {
    irp: Irp<'irp, C>,
    stack: *const core::ffi::c_void,
    _not_send: PhantomData<*mut ()>,
}

impl<C: IrpCompleter> IoRequest<'_, C> {
    /// Constructs an [`IoRequest`] from raw kernel pointers.
    ///
    /// # Safety
    ///
    /// - `irp` must be non-null, valid, and **exclusively owned** by this call.
    /// - `stack` must be the result of `IoGetCurrentIrpStackLocation(irp)`.
    /// - Neither pointer may be freed or aliased for the duration of the
    ///   dispatch callback.
    // The type itself is already `#[must_use]`; a second attribute would be
    // redundant (`clippy::double_must_use`).
    pub unsafe fn from_raw(irp: *mut core::ffi::c_void, stack: *const core::ffi::c_void) -> Self {
        debug_assert!(!irp.is_null(), "IoRequest::from_raw: null IRP pointer");
        debug_assert!(!stack.is_null(), "IoRequest::from_raw: null stack pointer");
        Self {
            // SAFETY: caller guarantees `irp` is valid and exclusively owned.
            irp: unsafe { Irp::from_raw(irp) },
            stack,
            _not_send: PhantomData,
        }
    }

    // ── IOCTL helpers ─────────────────────────────────────────────────────────

    /// Reads the IOCTL control code from `IO_STACK_LOCATION`.
    ///
    /// `ioctl_offset` is the byte offset of
    /// `Parameters.DeviceIoControl.IoControlCode` within
    /// `IO_STACK_LOCATION` for the current WDK version. Driver code
    /// computes this from `wdk-sys` and passes it through.
    ///
    /// Returns `None` when the field is zero (not a device-control IRP).
    #[must_use]
    pub const fn ioctl_code_at_offset(&self, ioctl_offset: usize) -> Option<IoControlCode> {
        // SAFETY: `stack` is a valid `IO_STACK_LOCATION` pointer for the
        // lifetime of this dispatch call. The caller is responsible for
        // providing the correct offset for the running WDK version.
        let raw = unsafe {
            let base = self.stack.cast::<u8>();
            base.add(ioctl_offset).cast::<u32>().read_unaligned()
        };
        if raw == 0 {
            None
        } else {
            Some(IoControlCode::from_raw(raw))
        }
    }

    /// Returns the `InputBufferLength` field from `IO_STACK_LOCATION`.
    ///
    /// `offset` is the byte offset of
    /// `Parameters.DeviceIoControl.InputBufferLength`.
    #[must_use]
    pub const fn input_buffer_length_at_offset(&self, offset: usize) -> usize {
        // SAFETY: same invariant as `ioctl_code_at_offset`.
        unsafe {
            let base = self.stack.cast::<u8>();
            base.add(offset).cast::<u32>().read_unaligned() as usize
        }
    }

    /// Returns the `OutputBufferLength` field from `IO_STACK_LOCATION`.
    ///
    /// `offset` is the byte offset of
    /// `Parameters.DeviceIoControl.OutputBufferLength`.
    #[must_use]
    pub const fn output_buffer_length_at_offset(&self, offset: usize) -> usize {
        // SAFETY: same invariant as `ioctl_code_at_offset`.
        unsafe {
            let base = self.stack.cast::<u8>();
            base.add(offset).cast::<u32>().read_unaligned() as usize
        }
    }

    /// Returns the system buffer for `METHOD_BUFFERED` IOCTLs.
    ///
    /// `irp_system_buffer_offset` is the byte offset of
    /// `AssociatedIrp.SystemBuffer` inside the `IRP` (not the stack
    /// location). Driver code passes the correct offset from `wdk-sys`.
    ///
    /// Returns `None` if the pointer stored there is null.
    ///
    /// # Safety
    ///
    /// - Only call this for `METHOD_BUFFERED` requests.
    /// - `irp_system_buffer_offset` must be the correct offset of
    ///   `AssociatedIrp.SystemBuffer` in the current WDK layout.
    #[must_use]
    pub unsafe fn system_buffer_at_offset(
        &self,
        irp_system_buffer_offset: usize,
    ) -> Option<*mut core::ffi::c_void> {
        // SAFETY: caller guarantees the offset is correct.
        let ptr = unsafe {
            let irp_base = self.irp.as_raw_ptr().cast::<u8>();
            irp_base
                .add(irp_system_buffer_offset)
                .cast::<*mut core::ffi::c_void>()
                .read_unaligned()
        };
        if ptr.is_null() {
            None
        } else {
            Some(ptr)
        }
    }

    // ── Completion ────────────────────────────────────────────────────────────

    /// Completes the request with `status` and zero bytes transferred.
    #[must_use]
    #[inline]
    pub fn complete(self, status: NtStatus) -> NtStatus {
        self.irp.complete(status)
    }

    /// Sets `IoStatus.Information` to `bytes` and completes with `status`.
    ///
    /// `info_ptr` must be the address of the `Information` field in
    /// `irp.IoStatus` — driver code computes this from `wdk-sys`.
    #[must_use]
    pub fn complete_with_information(
        self,
        status: NtStatus,
        bytes: usize,
        info_ptr: *mut usize,
    ) -> NtStatus {
        Irp::<C>::set_information_ptr(info_ptr, bytes);
        self.irp.complete(status)
    }

    // ── Forwarding ────────────────────────────────────────────────────────────

    /// Consumes `self` and returns the raw IRP pointer.
    ///
    /// Use **only** when forwarding to a lower driver:
    ///
    /// ```rust,ignore
    /// // SAFETY: lower_device is valid; IRP ownership transfers to IoCallDriver.
    /// unsafe {
    ///     let irp = request.into_raw_irp();
    ///     IoSkipCurrentIrpStackLocation(irp.cast());
    ///     IoCallDriver(lower_device, irp.cast());
    /// }
    /// ```
    ///
    /// After this call you must **not** access `request` again — the IRP
    /// belongs to the lower driver.
    #[must_use = "raw IRP must be passed to IoCallDriver; dropping it hangs the I/O manager"]
    #[inline]
    pub fn into_raw_irp(self) -> *mut core::ffi::c_void {
        self.irp.into_raw()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irp::NoopCompleter;

    fn dummy_request() -> IoRequest<'static, NoopCompleter> {
        // SAFETY: dummy non-null pointers — never dereferenced in tests.
        unsafe { IoRequest::from_raw(1usize as *mut _, 1usize as *const _) }
    }

    // ── Completion ────────────────────────────────────────────────────────────

    #[test]
    fn complete_success() {
        assert!(dummy_request().complete(NtStatus::SUCCESS).is_success());
    }

    #[test]
    fn complete_error() {
        assert!(dummy_request().complete(NtStatus::NOT_SUPPORTED).is_error());
    }

    #[test]
    fn complete_returns_same_status() {
        let status = NtStatus::INVALID_PARAMETER;
        assert_eq!(
            dummy_request().complete(status).into_raw(),
            status.into_raw()
        );
    }

    // ── Forwarding ────────────────────────────────────────────────────────────

    #[test]
    fn into_raw_irp_returns_pointer() {
        let ptr = 42usize as *mut core::ffi::c_void;
        // SAFETY: dummy pointers, no dereference.
        let req = unsafe { IoRequest::<NoopCompleter>::from_raw(ptr, 1usize as *const _) };
        assert_eq!(req.into_raw_irp(), ptr);
    }

    // ── IOCTL code reading ────────────────────────────────────────────────────

    #[test]
    fn ioctl_code_at_zero_offset_returns_none_for_zero() {
        let buf = [0u8; 8];
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        let result = req.ioctl_code_at_offset(0);
        let _ = req.complete(NtStatus::SUCCESS);
        assert!(result.is_none());
    }

    #[test]
    fn ioctl_code_at_offset_returns_code() {
        let mut buf = [0u8; 16];
        let code: u32 = 0x8000_2000;
        buf[4..8].copy_from_slice(&code.to_ne_bytes());
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        let result = req.ioctl_code_at_offset(4);
        let _ = req.complete(NtStatus::SUCCESS);
        assert_eq!(result.unwrap().into_raw(), code);
    }

    #[test]
    fn ioctl_code_at_offset_zero_returns_code_at_start() {
        let mut buf = [0u8; 8];
        let code: u32 = 0x0022_2407;
        buf[0..4].copy_from_slice(&code.to_ne_bytes());
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        let result = req.ioctl_code_at_offset(0);
        let _ = req.complete(NtStatus::SUCCESS);
        assert_eq!(result.unwrap().into_raw(), code);
    }

    // ── Buffer length reading ─────────────────────────────────────────────────

    #[test]
    fn input_buffer_length_at_offset() {
        let mut buf = [0u8; 16];
        let len: u32 = 256;
        buf[0..4].copy_from_slice(&len.to_ne_bytes());
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        let result = req.input_buffer_length_at_offset(0);
        let _ = req.complete(NtStatus::SUCCESS);
        assert_eq!(result, 256);
    }

    #[test]
    fn output_buffer_length_at_offset() {
        let mut buf = [0u8; 16];
        let len: u32 = 1024;
        buf[4..8].copy_from_slice(&len.to_ne_bytes());
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        let result = req.output_buffer_length_at_offset(4);
        let _ = req.complete(NtStatus::SUCCESS);
        assert_eq!(result, 1024);
    }

    #[test]
    fn buffer_length_zero_when_not_set() {
        let buf = [0u8; 16];
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        let input_len = req.input_buffer_length_at_offset(0);
        let output_len = req.output_buffer_length_at_offset(4);
        let _ = req.complete(NtStatus::SUCCESS);
        assert_eq!(input_len, 0);
        assert_eq!(output_len, 0);
    }

    // ── complete_with_information ─────────────────────────────────────────────

    #[test]
    fn complete_with_information_writes_bytes() {
        let ptr = 42usize as *mut core::ffi::c_void;
        let req = unsafe { IoRequest::<NoopCompleter>::from_raw(ptr, 1usize as *const _) };
        let mut info: usize = 0;
        let status = req.complete_with_information(NtStatus::SUCCESS, 128, &raw mut info);
        assert!(status.is_success());
        assert_eq!(info, 128);
    }

    #[test]
    fn complete_with_information_zero_bytes() {
        let ptr = 42usize as *mut core::ffi::c_void;
        let req = unsafe { IoRequest::<NoopCompleter>::from_raw(ptr, 1usize as *const _) };
        let mut info: usize = 99;
        let _ = req.complete_with_information(NtStatus::SUCCESS, 0, &raw mut info);
        assert_eq!(info, 0);
    }
}