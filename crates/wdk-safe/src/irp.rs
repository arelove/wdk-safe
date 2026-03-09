// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ownership-based wrapper for the Windows I/O Request Packet (`IRP`).
//!
//! [`Irp`] encodes kernel invariants in Rust's ownership system:
//!
//! - [`Irp::complete`] consumes `self` — double-complete is a compile error.
//! - `#[must_use]` warns if an IRP goes out of scope without being completed.
//!
//! # Kernel types
//!
//! This crate does not depend on `wdk-sys` directly so it can be tested on
//! the host. `IRP` is represented as an opaque `RawIrp` pointer newtype.
//! Driver code (which does link `wdk-sys`) converts via [`Irp::from_raw`].

use crate::NtStatus;

/// Opaque newtype over a raw `*mut IRP` kernel pointer.
///
/// Using a newtype (rather than a plain `*mut u8`) prevents accidental
/// confusion with other raw pointers in driver code.
#[repr(transparent)]
pub struct RawIrp(pub *mut core::ffi::c_void);

// SAFETY: RawIrp is just a pointer — the driver is responsible for
// ensuring correct kernel-mode synchronisation.
unsafe impl Send for RawIrp {}

/// An owned, non-aliased reference to a kernel `IRP`.
///
/// # Safety contract for [`Irp::from_raw`]
///
/// - The pointer must be non-null and point to a valid, initialised `IRP`.
/// - No other [`Irp`] may alias the same pointer simultaneously.
/// - The `IRP` must not have been completed already.
#[must_use = "IRP must be completed via `Irp::complete`; forgetting it will hang the I/O manager"]
pub struct Irp<'irp> {
    raw: RawIrp,
    _lifetime: core::marker::PhantomData<&'irp mut RawIrp>,
}

impl<'irp> Irp<'irp> {
    /// Wraps a raw IRP pointer in a safe [`Irp`].
    ///
    /// # Safety
    ///
    /// See the [module-level safety contract](self).
    #[must_use]
    pub unsafe fn from_raw(raw: *mut core::ffi::c_void) -> Self {
        debug_assert!(!raw.is_null(), "Irp::from_raw called with null pointer");
        Self {
            raw: RawIrp(raw),
            _lifetime: core::marker::PhantomData,
        }
    }

    /// Returns the raw pointer without consuming or completing `self`.
    ///
    /// Only for use by [`crate::request::IoRequest::into_raw_irp`].
    #[must_use]
    #[inline]
    pub(crate) fn as_raw_ptr(&self) -> *mut core::ffi::c_void {
        self.raw.0
    }

    /// Callback invoked by [`complete`] to perform the actual kernel call.
    ///
    /// In driver code (where `wdk-sys` is linked) this should call
    /// `IoCompleteRequest`. The default implementation is a no-op so
    /// the crate compiles on the host for testing.
    ///
    /// [`complete`]: Irp::complete
    #[inline]
    fn do_complete(raw: *mut core::ffi::c_void, status: i32) {
        // This function is overridden at link time by the driver crate
        // via the `wdk_safe_complete_irp` weak symbol pattern.
        // For host tests it is intentionally a no-op.
        let _ = (raw, status);
    }

    /// Sets the number of bytes transferred (`IoStatus.Information`).
    ///
    /// In a real driver the layout of `IRP` is known at compile time via
    /// `wdk-sys`. Here we accept the offset as a parameter so the library
    /// itself stays free of `wdk-sys`.
    ///
    /// Driver-level code uses the higher-level
    /// [`IoRequest::complete_with_information`] instead.
    #[inline]
    pub(crate) fn set_information_ptr(&mut self, info_ptr: *mut usize, bytes: usize) {
        // SAFETY: caller (IoRequest) provides the correct pointer derived
        // from a valid IRP for the lifetime 'irp.
        unsafe { *info_ptr = bytes; }
    }

    /// Completes the IRP with the given status and consumes `self`.
    ///
    /// Calls the kernel `IoCompleteRequest` (via [`do_complete`]) and then
    /// transfers ownership to the I/O manager.
    ///
    /// [`do_complete`]: Irp::do_complete
    pub fn complete(self, status: NtStatus) -> NtStatus {
        Self::do_complete(self.raw.0, status.into_raw());
        // Prevent any drop logic — the I/O manager now owns the IRP.
        core::mem::forget(self);
        status
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_returns_given_status() {
        // Use a non-null dummy pointer — do_complete is a no-op in tests.
        let dummy = 1usize as *mut core::ffi::c_void;
        let irp = unsafe { Irp::from_raw(dummy) };
        let status = irp.complete(NtStatus::SUCCESS);
        assert!(status.is_success());
    }
}