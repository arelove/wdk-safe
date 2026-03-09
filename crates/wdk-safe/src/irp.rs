// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ownership-based wrapper for the Windows I/O Request Packet (`IRP`).
//!
//! [`Irp`] encodes kernel invariants in Rust's ownership system:
//!
//! - [`Irp::complete`] consumes `self` — double-complete is a compile error.
//! - `#[must_use]` warns if an IRP goes out of scope without being completed.
//! - [`Irp::into_raw`] transfers ownership out of the type system, for use with
//!   [`IoSkipCurrentIrpStackLocation`] + [`IoCallDriver`] forwarding.
//!
//! # Kernel integration
//!
//! This crate does not link `wdk-sys` directly so it can be tested on the
//! host without a WDK installation. Kernel functions are injected at
//! link time via the [`IrpCompleter`] trait. Driver crates implement the
//! trait and pass it as a type parameter where needed — see
//! [`IoRequest`](crate::request::IoRequest) for the higher-level API.
//!
//! [`IoSkipCurrentIrpStackLocation`]: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-ioskipcurrentirpstacklocation
//! [`IoCallDriver`]: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-iocalldriver

use core::marker::PhantomData;

use crate::NtStatus;

// ── RawIrp ────────────────────────────────────────────────────────────────────

/// Opaque newtype over a raw `*mut IRP` kernel pointer.
///
/// Using a distinct newtype (rather than a plain `*mut c_void`) prevents
/// accidental confusion with other raw pointers in driver code.
#[derive(Debug)]
#[repr(transparent)]
pub struct RawIrp(pub *mut core::ffi::c_void);

// SAFETY: The kernel guarantees IRP pointers are accessible from any thread
// in the correct synchronisation context. The driver is responsible for
// upholding those context rules; we merely forward the pointer.
unsafe impl Send for RawIrp {}

// ── IrpCompleter trait
// ────────────────────────────────────────────────────────

/// Abstracts the `IoCompleteRequest` kernel function.
///
/// Implement this on a zero-sized type in your driver crate:
///
/// ```rust,ignore
/// use wdk_safe::irp::IrpCompleter;
///
/// pub struct KernelCompleter;
///
/// impl IrpCompleter for KernelCompleter {
///     unsafe fn complete(irp: *mut core::ffi::c_void, status: i32) {
///         use wdk_sys::{ntddk::IoCompleteRequest, IO_NO_INCREMENT};
///         // SAFETY: caller guarantees irp is valid and not yet completed.
///         unsafe {
///             (*irp.cast::<wdk_sys::IRP>()).IoStatus.__bindgen_anon_1.Status = status;
///             IoCompleteRequest(irp.cast(), IO_NO_INCREMENT as i8);
///         }
///     }
/// }
/// ```
///
/// For host-side unit tests use [`NoopCompleter`].
pub trait IrpCompleter {
    /// Calls `IoCompleteRequest` with `IO_NO_INCREMENT`.
    ///
    /// # Safety
    ///
    /// - `irp` must be non-null, valid, and **not** yet completed.
    /// - Must be called at `IRQL <= DISPATCH_LEVEL`.
    unsafe fn complete(irp: *mut core::ffi::c_void, status: i32);
}

/// A no-op [`IrpCompleter`] for host-side unit tests.
///
/// Does nothing — the IRP pointer is a dummy value in tests.
pub struct NoopCompleter;

impl IrpCompleter for NoopCompleter {
    #[inline]
    unsafe fn complete(_irp: *mut core::ffi::c_void, _status: i32) {
        // Intentionally empty: no WDK available on the test host.
    }
}

/// A tracking [`IrpCompleter`] that records whether `complete` was called.
///
/// Useful for verifying that completion actually happens in tests that
/// need to assert on side-effects beyond the return value.
#[cfg(test)]
pub(crate) struct TrackingCompleter;

#[cfg(test)]
pub(crate) static TRACKING_COMPLETE_CALLED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
impl IrpCompleter for TrackingCompleter {
    unsafe fn complete(_irp: *mut core::ffi::c_void, _status: i32) {
        TRACKING_COMPLETE_CALLED.store(true, core::sync::atomic::Ordering::SeqCst);
    }
}

// ── Irp ───────────────────────────────────────────────────────────────────────

/// An owned, non-aliased reference to a kernel `IRP`.
///
/// # Ownership contract
///
/// The holder of an `Irp` is responsible for exactly one of:
///
/// 1. Calling [`Irp::complete`] — completes the IRP via `IoCompleteRequest`.
/// 2. Calling [`Irp::into_raw`] — transfers ownership to a lower driver via
///    `IoSkipCurrentIrpStackLocation` + `IoCallDriver`.
///
/// Dropping an `Irp` without doing either triggers a `debug_assert!` in
/// debug builds and is always a bug (the I/O manager will hang).
///
/// # Type parameter
///
/// `C` is the [`IrpCompleter`] that supplies the actual `IoCompleteRequest`
/// call. Production code uses a driver-provided type; tests use
/// [`NoopCompleter`].
///
/// # Safety contract for [`Irp::from_raw`]
///
/// - The pointer must be non-null and point to a valid, initialised `IRP`.
/// - No other `Irp` may alias the same pointer simultaneously.
/// - The IRP must not have been completed already.
#[must_use = "IRP must be completed via `complete` or forwarded via `into_raw`; \
              forgetting it hangs the I/O manager"]
pub struct Irp<'irp, C: IrpCompleter> {
    raw: RawIrp,
    _marker: PhantomData<(&'irp mut RawIrp, C)>,
}

impl<C: IrpCompleter> Irp<'_, C> {
    /// Wraps a raw IRP pointer in a safe [`Irp`].
    ///
    /// # Safety
    ///
    /// See the [type-level safety contract](Irp).
    // The type itself is already `#[must_use]`; a second attribute would be
    // redundant (`clippy::double_must_use`).
    #[inline]
    pub unsafe fn from_raw(raw: *mut core::ffi::c_void) -> Self {
        debug_assert!(!raw.is_null(), "Irp::from_raw: null IRP pointer");
        Self {
            raw: RawIrp(raw),
            _marker: PhantomData,
        }
    }

    /// Returns the raw pointer without completing or consuming `self`.
    ///
    /// Only exposed to [`crate::request::IoRequest`]; call
    /// [`IoRequest::into_raw_irp`](crate::request::IoRequest::into_raw_irp)
    /// from driver code instead.
    #[must_use]
    #[inline]
    pub(crate) const fn as_raw_ptr(&self) -> *mut core::ffi::c_void {
        self.raw.0
    }

    /// Transfers ownership out of the type system.
    ///
    /// Returns the raw pointer so the caller can pass it to
    /// `IoSkipCurrentIrpStackLocation` + `IoCallDriver`.
    ///
    /// **The caller is solely responsible for completing the IRP or passing
    /// it to a lower driver that will complete it.**
    #[must_use = "raw IRP must be forwarded to IoCallDriver or completed; \
                  dropping it hangs the I/O manager"]
    #[inline]
    pub const fn into_raw(self) -> *mut core::ffi::c_void {
        let ptr = self.raw.0;
        // Do NOT run drop — ownership is now with the caller.
        core::mem::forget(self);
        ptr
    }

    /// Writes `bytes` into `IoStatus.Information` at `info_ptr`.
    ///
    /// `info_ptr` must be the address of the `Information` field inside this
    /// IRP. Driver code uses the higher-level
    /// [`IoRequest::complete_with_information`](crate::request::IoRequest::complete_with_information).
    #[inline]
    pub(crate) fn set_information_ptr(info_ptr: *mut usize, bytes: usize) {
        // SAFETY: caller (IoRequest) provides the correct pointer derived from
        // a valid IRP that we own for lifetime 'irp.
        unsafe { info_ptr.write(bytes) };
    }

    /// Completes the IRP with `status` and consumes `self`.
    ///
    /// Calls `C::complete` (typically `IoCompleteRequest`) and transfers
    /// ownership to the I/O manager.
    #[must_use]
    pub fn complete(self, status: NtStatus) -> NtStatus {
        let raw = self.raw.0;
        // Prevent drop — the I/O manager owns the IRP from here.
        core::mem::forget(self);
        // SAFETY: raw is non-null and exclusively owned (invariant of Irp).
        // We have not completed it before (also an invariant).
        unsafe { C::complete(raw, status.into_raw()) };
        status
    }
}

impl<C: IrpCompleter> Drop for Irp<'_, C> {
    fn drop(&mut self) {
        // If we reach here the IRP was neither completed nor forwarded.
        // In debug builds, scream loudly so the developer notices.
        debug_assert!(
            false,
            "Irp dropped without being completed or forwarded — \
             this will hang the I/O manager. Call `complete` or `into_raw`."
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Dummy non-null pointer used in tests — never dereferenced.
    fn dummy() -> *mut core::ffi::c_void {
        1usize as *mut core::ffi::c_void
    }

    // ── Completion ────────────────────────────────────────────────────────────

    #[test]
    fn complete_returns_success() {
        let irp = unsafe { Irp::<NoopCompleter>::from_raw(dummy()) };
        assert!(irp.complete(NtStatus::SUCCESS).is_success());
    }

    #[test]
    fn complete_returns_error() {
        let irp = unsafe { Irp::<NoopCompleter>::from_raw(dummy()) };
        let status = irp.complete(NtStatus::UNSUCCESSFUL);
        assert!(status.is_error());
    }

    #[test]
    fn complete_propagates_custom_status() {
        let irp = unsafe { Irp::<NoopCompleter>::from_raw(dummy()) };
        #[allow(clippy::cast_possible_wrap)]
        let custom = NtStatus::from_raw(0xC000_00BB_u32 as i32);
        assert_eq!(irp.complete(custom).into_raw(), custom.into_raw());
    }

    #[test]
    fn complete_invokes_completer() {
        TRACKING_COMPLETE_CALLED.store(false, core::sync::atomic::Ordering::SeqCst);
        let irp = unsafe { Irp::<TrackingCompleter>::from_raw(dummy()) };
        let _ = irp.complete(NtStatus::SUCCESS);
        assert!(
            TRACKING_COMPLETE_CALLED.load(core::sync::atomic::Ordering::SeqCst),
            "IrpCompleter::complete was not called"
        );
    }

    // ── Forwarding ────────────────────────────────────────────────────────────

    #[test]
    fn into_raw_recovers_pointer() {
        let ptr = dummy();
        let irp = unsafe { Irp::<NoopCompleter>::from_raw(ptr) };
        assert_eq!(irp.into_raw(), ptr);
    }

    #[test]
    fn into_raw_does_not_call_completer() {
        TRACKING_COMPLETE_CALLED.store(false, core::sync::atomic::Ordering::SeqCst);
        let irp = unsafe { Irp::<TrackingCompleter>::from_raw(dummy()) };
        let _ = irp.into_raw();
        assert!(
            !TRACKING_COMPLETE_CALLED.load(core::sync::atomic::Ordering::SeqCst),
            "IrpCompleter::complete must NOT be called by into_raw"
        );
    }

    // ── Pointer identity ──────────────────────────────────────────────────────

    #[test]
    fn as_raw_ptr_matches_from_raw_input() {
        let ptr = 0xDEAD_C0DEusize as *mut core::ffi::c_void;
        let irp = unsafe { Irp::<NoopCompleter>::from_raw(ptr) };
        let recovered = irp.as_raw_ptr();
        let _ = irp.complete(NtStatus::SUCCESS);
        assert_eq!(recovered, ptr);
    }

    #[test]
    fn different_pointers_stay_distinct() {
        let ptr_a = 1usize as *mut core::ffi::c_void;
        let ptr_b = 2usize as *mut core::ffi::c_void;
        let irp_a = unsafe { Irp::<NoopCompleter>::from_raw(ptr_a) };
        let irp_b = unsafe { Irp::<NoopCompleter>::from_raw(ptr_b) };
        assert_ne!(irp_a.as_raw_ptr(), irp_b.as_raw_ptr());
        let _ = irp_a.complete(NtStatus::SUCCESS);
        let _ = irp_b.complete(NtStatus::SUCCESS);
    }

    // ── NoopCompleter ─────────────────────────────────────────────────────────

    #[test]
    fn noop_completer_is_safe_to_call() {
        // SAFETY: dummy pointer, NoopCompleter never dereferences it.
        unsafe { NoopCompleter::complete(dummy(), 0) };
    }
}
