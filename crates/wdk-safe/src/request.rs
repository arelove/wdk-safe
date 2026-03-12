// Copyright (c) 2026 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`IoRequest`] — the primary abstraction for a dispatched I/O request.
//!
//! `IoRequest<C>` wraps an [`Irp<C>`] together with the current
//! `IO_STACK_LOCATION` pointer. The `C` type parameter is the
//! [`IrpCompleter`] that supplies `IoCompleteRequest`; it is a zero-sized
//! type, so the wrapper is the same size as two raw pointers.
//!
//! # Buffer access
//!
//! The methods for reading IOCTL parameters and buffers accept an
//! [`IoStackOffsets`] argument that encodes the field offsets for the
//! running WDK version. Use the provided constant
//! [`IoStackOffsets::WDK_SYS_0_5_X64`] or define your own.
//!
//! # IRQL constraints
//!
//! `IoRequest` may be constructed and used at `IRQL <= DISPATCH_LEVEL`.
//! The inner `Irp::complete` must also be called at `IRQL <= DISPATCH_LEVEL`.
//!
//! [`IrpCompleter`]: crate::irp::IrpCompleter
//! [`IoStackOffsets`]: crate::ioctl::IoStackOffsets

use core::marker::PhantomData;

use crate::{
    ioctl::{IoControlCode, IoStackOffsets},
    irp::{Irp, IrpCompleter},
    NtStatus,
};

/// An I/O request received in a dispatch callback.
///
/// Must be **exactly once** either:
///
/// - Completed via [`complete`] / [`complete_with_info`], or
/// - Forwarded via [`into_raw_irp`].
///
/// `#[must_use]` on the type and the drop bomb on the inner [`Irp`] make both
/// mistakes into compile-time or debug-time errors.
///
/// # Type parameter
///
/// `C: IrpCompleter` — the zero-sized type that calls `IoCompleteRequest`.
/// Production code uses a type from the driver crate. Tests use
/// [`NoopCompleter`](crate::irp::NoopCompleter).
///
/// # IRQL
///
/// Construction, parameter access, and completion all require
/// `IRQL <= DISPATCH_LEVEL`. Do not hold an `IoRequest` across a wait or
/// page fault.
///
/// [`complete`]: IoRequest::complete
/// [`complete_with_info`]: IoRequest::complete_with_info
/// [`into_raw_irp`]: IoRequest::into_raw_irp
#[must_use = "IoRequest must be completed or forwarded before the dispatch routine returns"]
pub struct IoRequest<'irp, C: IrpCompleter> {
    irp: Irp<'irp, C>,
    stack: *const core::ffi::c_void,
    _not_send_sync: PhantomData<*mut ()>,
}

impl<C: IrpCompleter> IoRequest<'_, C> {
    /// Constructs an [`IoRequest`] from raw kernel pointers.
    ///
    /// # Safety
    ///
    /// - `irp` must be non-null, valid, and **exclusively owned** by this call.
    /// - `stack` must equal `IoGetCurrentIrpStackLocation(irp)`.
    /// - Neither pointer may be freed or aliased for the duration of the
    ///   dispatch callback.
    pub unsafe fn from_raw(irp: *mut core::ffi::c_void, stack: *const core::ffi::c_void) -> Self {
        debug_assert!(!irp.is_null(), "IoRequest::from_raw: null IRP pointer");
        debug_assert!(!stack.is_null(), "IoRequest::from_raw: null stack pointer");
        Self {
            // SAFETY: caller guarantees `irp` is valid and exclusively owned.
            irp: unsafe { Irp::from_raw(irp) },
            stack,
            _not_send_sync: PhantomData,
        }
    }

    // ── IOCTL parameter access ────────────────────────────────────────────────

    /// Reads the IOCTL control code from the current `IO_STACK_LOCATION`.
    ///
    /// Returns `None` if the `IoControlCode` field is zero (not a
    /// device-control IRP, or the IRP major function is something other than
    /// `IRP_MJ_DEVICE_CONTROL` / `IRP_MJ_INTERNAL_DEVICE_CONTROL`).
    ///
    /// # IRQL
    ///
    /// Safe at any IRQL — reads kernel memory, no allocation or wait.
    #[must_use]
    pub const fn ioctl_code(&self, offsets: &IoStackOffsets) -> Option<IoControlCode> {
        // SAFETY: `stack` is a valid `IO_STACK_LOCATION` pointer for the
        // lifetime of this dispatch call. `offsets.ioctl_code` is the verified
        // byte offset of `Parameters.DeviceIoControl.IoControlCode`.
        let raw = unsafe {
            self.stack
                .cast::<u8>()
                .add(offsets.ioctl_code)
                .cast::<u32>()
                .read_unaligned()
        };
        if raw == 0 {
            None
        } else {
            Some(IoControlCode::from_raw(raw))
        }
    }

    /// Returns the `InputBufferLength` field from the current
    /// `IO_STACK_LOCATION`.
    ///
    /// # IRQL
    ///
    /// Safe at any IRQL.
    #[must_use]
    pub const fn input_buffer_length(&self, offsets: &IoStackOffsets) -> usize {
        // SAFETY: same invariant as `ioctl_code`.
        unsafe {
            self.stack
                .cast::<u8>()
                .add(offsets.input_buffer_length)
                .cast::<u32>()
                .read_unaligned() as usize
        }
    }

    /// Returns the `OutputBufferLength` field from the current
    /// `IO_STACK_LOCATION`.
    ///
    /// # IRQL
    ///
    /// Safe at any IRQL.
    #[must_use]
    pub const fn output_buffer_length(&self, offsets: &IoStackOffsets) -> usize {
        // SAFETY: same invariant as `ioctl_code`.
        unsafe {
            self.stack
                .cast::<u8>()
                .add(offsets.output_buffer_length)
                .cast::<u32>()
                .read_unaligned() as usize
        }
    }

    /// Returns the `AssociatedIrp.SystemBuffer` pointer for
    /// `METHOD_BUFFERED` IOCTLs.
    ///
    /// Returns `None` if the pointer stored in the IRP is null.
    ///
    /// # Safety
    ///
    /// - Only call this for `METHOD_BUFFERED` requests
    ///   (`request.ioctl_code(...).map(|c| c.method()) == Some(TransferMethod::Buffered)`).
    /// - `offsets.irp_system_buffer` must be the correct offset of
    ///   `AssociatedIrp.SystemBuffer` in the running WDK layout.
    /// - The returned pointer is valid for the duration of the IRP.
    ///   Do **not** store it past IRP completion.
    ///
    /// # IRQL
    ///
    /// Safe at any IRQL — reads kernel memory.
    #[must_use]
    pub unsafe fn system_buffer(&self, offsets: &IoStackOffsets) -> Option<*mut core::ffi::c_void> {
        // SAFETY: caller guarantees offset is correct and this is METHOD_BUFFERED.
        let ptr = unsafe {
            self.irp
                .as_raw_ptr()
                .cast::<u8>()
                .add(offsets.irp_system_buffer)
                .cast::<*mut core::ffi::c_void>()
                .read_unaligned()
        };
        if ptr.is_null() {
            None
        } else {
            Some(ptr)
        }
    }

    /// Returns a raw pointer to the `IoStatus.Information` field inside
    /// the IRP.
    ///
    /// Pass this to [`complete_with_info`](Self::complete_with_info) to set
    /// the number of bytes transferred.
    ///
    /// # Safety
    ///
    /// - `offsets.irp_information` must be the correct byte offset of
    ///   `IoStatus.Information` in the running WDK layout.
    /// - The returned pointer is valid only while this `IoRequest` is alive.
    ///   Do not store it past completion.
    ///
    /// # IRQL
    ///
    /// Safe at any IRQL — pure pointer arithmetic.
    #[must_use]
    pub const unsafe fn io_status_information_ptr(&self, offsets: &IoStackOffsets) -> *mut usize {
        // SAFETY: caller guarantees offset correctness.
        // cast_ptr_alignment: the IRP field is correctly aligned in the kernel
        // struct; we use `cast::<u8>().add().cast()` intentionally for
        // byte-offset arithmetic — the resulting pointer is valid for
        // unaligned reads/writes via read_unaligned/write_unaligned.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            self.irp
                .as_raw_ptr()
                .cast::<u8>()
                .add(offsets.irp_information)
                .cast::<usize>()
        }
    }

    // ── Completion ────────────────────────────────────────────────────────────

    /// Completes the request with `status` and zero bytes transferred.
    ///
    /// Returns `status` so the caller can `return request.complete(...)`.
    ///
    /// # IRQL
    ///
    /// Must be called at `IRQL <= DISPATCH_LEVEL`.
    #[must_use = "return the NtStatus from your dispatch function"]
    #[inline]
    pub fn complete(self, status: NtStatus) -> NtStatus {
        self.irp.complete(status)
    }

    /// Sets `IoStatus.Information` to `bytes_transferred` and completes
    /// the request with `status`.
    ///
    /// Use this for `IRP_MJ_READ`, `IRP_MJ_WRITE`, and `IRP_MJ_DEVICE_CONTROL`
    /// when the driver transfers data into the output buffer.
    ///
    /// # Safety
    ///
    /// `info_ptr` must be the address of the `IoStatus.Information` field
    /// inside the IRP owned by this request. Obtain it via
    /// [`io_status_information_ptr`](Self::io_status_information_ptr).
    ///
    /// # IRQL
    ///
    /// Must be called at `IRQL <= DISPATCH_LEVEL`.
    #[must_use = "return the NtStatus from your dispatch function"]
    pub unsafe fn complete_with_info(
        self,
        status: NtStatus,
        bytes_transferred: usize,
        info_ptr: *mut usize,
    ) -> NtStatus {
        // SAFETY: caller provides a valid pointer to IoStatus.Information.
        unsafe { Irp::<C>::set_information_ptr(info_ptr, bytes_transferred) };
        self.irp.complete(status)
    }

    // ── Forwarding ────────────────────────────────────────────────────────────

    /// Consumes `self` and returns the raw `*mut IRP` pointer.
    ///
    /// Use **only** when forwarding to a lower driver:
    ///
    /// ```rust,ignore
    /// // SAFETY: lower_device is valid; IRP ownership transfers to IofCallDriver.
    /// unsafe {
    ///     let irp = request.into_raw_irp();
    ///     IoSkipCurrentIrpStackLocation(irp.cast());
    ///     IofCallDriver(lower_device, irp.cast());
    /// }
    /// ```
    ///
    /// After this call you must **not** access `request` again.
    ///
    /// # IRQL
    ///
    /// No IRQL constraint on this call itself. `IofCallDriver` must be called
    /// at `IRQL <= DISPATCH_LEVEL`.
    #[must_use = "raw IRP must be passed to IofCallDriver; dropping it hangs the I/O manager"]
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

    const OFFSETS: IoStackOffsets = IoStackOffsets::WDK_SYS_0_5_X64;

    fn dummy_request() -> IoRequest<'static, NoopCompleter> {
        // SAFETY: dummy non-null pointers — never dereferenced.
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
        // SAFETY: dummy pointers — never dereferenced.
        let req = unsafe { IoRequest::<NoopCompleter>::from_raw(ptr, 1usize as *const _) };
        assert_eq!(req.into_raw_irp(), ptr);
    }

    // ── IOCTL code reading ────────────────────────────────────────────────────

    /// Build a fake `IO_STACK_LOCATION` buffer with an IOCTL code at the
    /// offset specified by `WDK_SYS_0_5_X64`.
    fn stack_with_ioctl(code: u32) -> Vec<u8> {
        let size = OFFSETS.ioctl_code + 4;
        let mut buf = vec![0u8; size];
        buf[OFFSETS.ioctl_code..OFFSETS.ioctl_code + 4].copy_from_slice(&code.to_ne_bytes());
        buf
    }

    #[test]
    fn ioctl_code_returns_none_for_zero() {
        #[allow(clippy::useless_vec)]
        let buf = vec![0u8; OFFSETS.ioctl_code + 4];
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        let result = req.ioctl_code(&OFFSETS);
        let _ = req.complete(NtStatus::SUCCESS);
        assert!(result.is_none());
    }

    #[test]
    fn ioctl_code_returns_code() {
        let expected: u32 = 0x8000_2000;
        let buf = stack_with_ioctl(expected);
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        let result = req.ioctl_code(&OFFSETS);
        let _ = req.complete(NtStatus::SUCCESS);
        assert_eq!(result.unwrap().into_raw(), expected);
    }

    #[test]
    fn ioctl_code_decodes_correctly() {
        use crate::ioctl::{IoControlCode, RequiredAccess, TransferMethod};
        let code = IoControlCode::new(0x8000, 0x800, TransferMethod::Buffered, RequiredAccess::Any);
        let buf = stack_with_ioctl(code.into_raw());
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        let result = req.ioctl_code(&OFFSETS);
        let _ = req.complete(NtStatus::SUCCESS);
        let decoded = result.unwrap();
        assert_eq!(decoded.device_type(), 0x8000);
        assert_eq!(decoded.function(), 0x800);
        assert_eq!(decoded.method(), TransferMethod::Buffered);
        assert_eq!(decoded.access(), RequiredAccess::Any);
    }

    // ── Buffer lengths ────────────────────────────────────────────────────────

    fn stack_with_lengths(in_len: u32, out_len: u32) -> Vec<u8> {
        let size = OFFSETS
            .input_buffer_length
            .max(OFFSETS.output_buffer_length)
            + 4;
        let mut buf = vec![0u8; size];
        buf[OFFSETS.input_buffer_length..OFFSETS.input_buffer_length + 4]
            .copy_from_slice(&in_len.to_ne_bytes());
        buf[OFFSETS.output_buffer_length..OFFSETS.output_buffer_length + 4]
            .copy_from_slice(&out_len.to_ne_bytes());
        buf
    }

    #[test]
    fn input_buffer_length_read() {
        let buf = stack_with_lengths(42, 0);
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        let len = req.input_buffer_length(&OFFSETS);
        let _ = req.complete(NtStatus::SUCCESS);
        assert_eq!(len, 42);
    }

    #[test]
    fn output_buffer_length_read() {
        let buf = stack_with_lengths(0, 99);
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        let len = req.output_buffer_length(&OFFSETS);
        let _ = req.complete(NtStatus::SUCCESS);
        assert_eq!(len, 99);
    }

    #[test]
    fn both_lengths_read_independently() {
        let buf = stack_with_lengths(512, 1024);
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        let in_l = req.input_buffer_length(&OFFSETS);
        let out_l = req.output_buffer_length(&OFFSETS);
        let _ = req.complete(NtStatus::SUCCESS);
        assert_eq!(in_l, 512);
        assert_eq!(out_l, 1024);
    }

    #[test]
    fn lengths_are_zero_when_not_set() {
        let buf = vec![0u8; 256];
        let req =
            unsafe { IoRequest::<NoopCompleter>::from_raw(1usize as *mut _, buf.as_ptr().cast()) };
        assert_eq!(req.input_buffer_length(&OFFSETS), 0);
        assert_eq!(req.output_buffer_length(&OFFSETS), 0);
        let _ = req.complete(NtStatus::SUCCESS);
    }

    // ── complete_with_info ────────────────────────────────────────────────────

    #[test]
    fn complete_with_info_writes_bytes() {
        let mut info: usize = 0;
        let req = dummy_request();
        let _ =
            unsafe { req.complete_with_info(NtStatus::SUCCESS, 42, core::ptr::addr_of_mut!(info)) };
        assert_eq!(info, 42);
    }

    #[test]
    fn complete_with_info_zero_bytes() {
        let mut info: usize = 99;
        let req = dummy_request();
        let _ =
            unsafe { req.complete_with_info(NtStatus::SUCCESS, 0, core::ptr::addr_of_mut!(info)) };
        assert_eq!(info, 0);
    }

    #[test]
    fn complete_with_info_large_transfer() {
        let mut info: usize = 0;
        let req = dummy_request();
        let _ = unsafe {
            req.complete_with_info(NtStatus::SUCCESS, usize::MAX, core::ptr::addr_of_mut!(info))
        };
        assert_eq!(info, usize::MAX);
    }

    #[test]
    fn complete_returns_same_status_as_input() {
        let mut info: usize = 0;
        let req = dummy_request();
        let status = unsafe {
            req.complete_with_info(
                NtStatus::INVALID_PARAMETER,
                0,
                core::ptr::addr_of_mut!(info),
            )
        };
        assert_eq!(status, NtStatus::INVALID_PARAMETER);
    }
}
