// Copyright (c) 2026 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ownership-based wrapper for the Windows I/O Request Packet (`IRP`).
//!
//! # Ownership model
//!
//! An [`Irp<C>`] is a **linear type**: the holder is responsible for
//! exactly one of:
//!
//! 1. **Complete** — call [`Irp::complete`], which calls `C::complete`
//!    (`IoCompleteRequest`) and consumes `self`. The I/O manager then owns the
//!    IRP.
//! 2. **Forward** — call [`Irp::into_raw`], extract the pointer, call
//!    `IoSkipCurrentIrpStackLocation` + `IoCallDriver`. Ownership transfers to
//!    the lower driver.
//!
//! Forgetting to do either is a debug-build assertion failure (drop bomb).
//! Doing both is a compile error (use of moved value).
//!
//! # IRQL constraints
//!
//! [`Irp::complete`] must be called at `IRQL <= DISPATCH_LEVEL`.\
//! [`Irp::into_raw`] has no IRQL constraint — it is a no-op at runtime.
//!
//! # Kernel integration without `wdk-sys`
//!
//! This crate does not link `wdk-sys` so it can be tested on the host.
//! Kernel functions are injected at link time via the [`IrpCompleter`] trait.
//! Driver crates implement the trait once:
//!
//! ```rust,ignore
//! pub struct KernelCompleter;
//! impl IrpCompleter for KernelCompleter {
//!     unsafe fn complete(irp: *mut core::ffi::c_void, status: i32) {
//!         unsafe {
//!             let pirp = irp.cast::<wdk_sys::IRP>();
//!             (*pirp).IoStatus.__bindgen_anon_1.Status = status;
//!             wdk_sys::ntddk::IofCompleteRequest(pirp, wdk_sys::IO_NO_INCREMENT as i8);
//!         }
//!     }
//! }
//! ```
//!
//! [`IoSkipCurrentIrpStackLocation`]: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-ioskipcurrentirpstacklocation
//! [`IoCallDriver`]: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-iocalldriver

use core::marker::PhantomData;

use crate::NtStatus;

// ── RawIrp ────────────────────────────────────────────────────────────────────

/// Opaque newtype over a raw `*mut IRP` kernel pointer.
///
/// Using a distinct newtype (rather than `*mut c_void`) prevents accidental
/// confusion with other raw pointers in driver dispatch code.
///
/// # Send impl
///
/// The kernel guarantees that IRP pointers are accessible from any thread in
/// the correct synchronisation context. The driver is responsible for
/// upholding IRQL rules; this type merely carries the pointer.
#[derive(Debug)]
#[repr(transparent)]
pub struct RawIrp(pub *mut core::ffi::c_void);

// SAFETY: See module-level comment — the driver must uphold IRQL invariants.
unsafe impl Send for RawIrp {}

// ── IrpCompleter
// ──────────────────────────────────────────────────────────────

/// Abstracts the `IoCompleteRequest` / `IofCompleteRequest` kernel function.
///
/// Implement this on a **zero-sized type** in your driver crate (which links
/// `wdk-sys`). The type parameter propagates to [`Irp<C>`] and
/// [`IoRequest<C>`](crate::request::IoRequest), making the kernel call
/// transparent to the type system.
///
/// # Example — production implementation
///
/// ```rust,ignore
/// use wdk_safe::IrpCompleter;
///
/// /// Zero-sized type that calls IofCompleteRequest via wdk-sys.
/// pub struct KernelCompleter;
///
/// impl IrpCompleter for KernelCompleter {
///     /// # Safety
///     ///
///     /// - `irp` must be non-null, valid, and not yet completed.
///     /// - Called at IRQL <= DISPATCH_LEVEL.
///     unsafe fn complete(irp: *mut core::ffi::c_void, status: i32) {
///         unsafe {
///             let pirp = irp.cast::<wdk_sys::IRP>();
///             (*pirp).IoStatus.__bindgen_anon_1.Status = status;
///             wdk_sys::ntddk::IofCompleteRequest(pirp, wdk_sys::IO_NO_INCREMENT as i8);
///         }
///     }
/// }
/// ```
///
/// For host-side unit tests use [`NoopCompleter`] or `TrackingCompleter` (feature `test-utils`).
pub trait IrpCompleter {
    /// Completes the IRP by calling `IoCompleteRequest`.
    ///
    /// # Safety
    ///
    /// - `irp` must be non-null and point to a valid, fully-initialised `IRP`.
    /// - The IRP must **not** have been completed already.
    /// - Must be called at `IRQL <= DISPATCH_LEVEL`.
    unsafe fn complete(irp: *mut core::ffi::c_void, status: i32);
}

// ── NoopCompleter
// ─────────────────────────────────────────────────────────────

/// A no-op [`IrpCompleter`] for host-side unit tests.
///
/// `complete` is intentionally empty — it never dereferences the IRP pointer.
/// All `Irp<NoopCompleter>` tests can use dummy non-null values safely.
pub struct NoopCompleter;

impl IrpCompleter for NoopCompleter {
    #[inline]
    unsafe fn complete(_irp: *mut core::ffi::c_void, _status: i32) {
        // Intentionally empty — no WDK available on the test host.
    }
}

// ── TrackingCompleter
// ─────────────────────────────────────────────────────────

/// An [`IrpCompleter`] that records whether `complete` was called.
///
/// Useful in tests that need to verify completion actually happens,
/// beyond just checking the return value of the dispatch method.
///
/// # Usage
///
/// ```rust
/// use std::sync::atomic::Ordering;
///
/// use wdk_safe::{
///     irp::{TrackingCompleter, TRACKING_COMPLETE_CALLED},
///     Irp, NtStatus,
/// };
///
/// TRACKING_COMPLETE_CALLED.store(false, Ordering::SeqCst);
///
/// let irp = unsafe { wdk_safe::irp::Irp::<TrackingCompleter>::from_raw(1usize as *mut _) };
/// let _ = irp.complete(NtStatus::SUCCESS);
///
/// assert!(TRACKING_COMPLETE_CALLED.load(Ordering::SeqCst));
/// ```
///
/// # Thread safety
///
/// [`TRACKING_COMPLETE_CALLED`] is a global `AtomicBool`. Tests that use it
/// must be run single-threaded (e.g. `cargo test -- --test-threads=1`) or
/// reset the flag explicitly before each test.
#[cfg(any(test, feature = "test-utils"))]
pub struct TrackingCompleter;

/// Shared flag written by `TrackingCompleter` (feature `test-utils`).
///
/// Reset to `false` at the start of each test that checks it.
#[cfg(any(test, feature = "test-utils"))]
pub static TRACKING_COMPLETE_CALLED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

#[cfg(any(test, feature = "test-utils"))]
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
/// 1. Calling [`Irp::complete`] — completes via `C::complete` and transfers
///    ownership to the I/O manager.
/// 2. Calling [`Irp::into_raw`] — transfers ownership to a lower driver via
///    `IoSkipCurrentIrpStackLocation` + `IoCallDriver`.
///
/// Dropping an `Irp` without either triggers a `debug_assert!` panic in debug
/// builds. In release builds, the drop is silent — the IRP will hang, which is
/// always a bug but will not immediately crash.
///
/// # Type parameter
///
/// `C: IrpCompleter` is the zero-sized type that calls `IoCompleteRequest`.\
/// Production: driver-provided `KernelCompleter`.\
/// Tests: [`NoopCompleter`] or `TrackingCompleter` (feature `test-utils`).
///
/// # IRQL constraints
///
/// - [`complete`](Irp::complete) must be called at `IRQL <= DISPATCH_LEVEL`.
/// - [`into_raw`](Irp::into_raw) has no IRQL constraint.
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
    /// This is intentionally `pub(crate)`. External code should use
    /// [`IoRequest::into_raw_irp`](crate::request::IoRequest::into_raw_irp)
    /// for forwarding.
    #[must_use]
    #[inline]
    pub(crate) const fn as_raw_ptr(&self) -> *mut core::ffi::c_void {
        self.raw.0
    }

    /// Transfers ownership out of the type system and returns the raw pointer.
    ///
    /// The caller is solely responsible for completing the IRP or passing it
    /// to a lower driver that will complete it. The drop bomb is disarmed.
    ///
    /// Prefer [`IoRequest::into_raw_irp`](crate::request::IoRequest::into_raw_irp)
    /// in dispatch code.
    #[must_use = "raw IRP must be forwarded to IoCallDriver or completed; \
                  dropping it hangs the I/O manager"]
    #[inline]
    pub const fn into_raw(self) -> *mut core::ffi::c_void {
        let ptr = self.raw.0;
        // Disarm the drop bomb — ownership transferred to the caller.
        core::mem::forget(self);
        ptr
    }

    /// Writes `bytes` into an `IoStatus.Information`-shaped memory location.
    ///
    /// `info_ptr` must be the address of the `Information` field inside the
    /// IRP owned by `self`. Callers obtain this via
    /// [`IoRequest::complete_with_info`](crate::request::IoRequest::complete_with_info).
    #[inline]
    pub(crate) unsafe fn set_information_ptr(info_ptr: *mut usize, bytes: usize) {
        // SAFETY: caller (IoRequest::complete_with_info) provides a valid
        // pointer to IoStatus.Information inside the owned IRP.
        unsafe { info_ptr.write(bytes) };
    }

    /// Completes the IRP with `status` and consumes `self`.
    ///
    /// Calls `C::complete` (typically `IofCompleteRequest`) and transfers
    /// ownership to the I/O manager. Returns `status` so callers can return
    /// it directly from a dispatch function.
    ///
    /// # IRQL
    ///
    /// Must be called at `IRQL <= DISPATCH_LEVEL`.
    #[must_use = "return the NtStatus from your dispatch function"]
    pub fn complete(self, status: NtStatus) -> NtStatus {
        let raw = self.raw.0;
        // Disarm the drop bomb before calling C::complete so that if
        // C::complete somehow panics (which it must not in kernel mode),
        // the drop impl does not fire a second assert.
        core::mem::forget(self);
        // SAFETY: `raw` is non-null and exclusively owned (invariant of Irp).
        // We have not completed it before (also an invariant).
        unsafe { C::complete(raw, status.into_raw()) };
        status
    }
}

impl<C: IrpCompleter> Drop for Irp<'_, C> {
    fn drop(&mut self) {
        // Reaching here means the IRP was neither completed nor forwarded.
        // In debug builds, fire a loud assertion so the developer finds the
        // bug immediately. In release builds we cannot safely complete the
        // IRP (we don't know the correct status), so we let it leak — the
        // system will eventually hang or bugcheck, which is the correct
        // outcome for this programming error.
        debug_assert!(
            false,
            "Irp dropped without being completed or forwarded — \
             this WILL hang the I/O manager. Call `complete` or `into_raw`."
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use core::sync::atomic::Ordering;

    use super::*;

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
        assert!(irp.complete(NtStatus::UNSUCCESSFUL).is_error());
    }

    #[test]
    fn complete_propagates_exact_status() {
        let irp = unsafe { Irp::<NoopCompleter>::from_raw(dummy()) };
        #[allow(clippy::cast_possible_wrap)]
        let custom = NtStatus::from_raw(0xC000_00BB_u32 as i32);
        assert_eq!(irp.complete(custom).into_raw(), custom.into_raw());
    }

    #[test]
    fn complete_invokes_completer() {
        TRACKING_COMPLETE_CALLED.store(false, Ordering::SeqCst);
        let irp = unsafe { Irp::<TrackingCompleter>::from_raw(dummy()) };
        let _ = irp.complete(NtStatus::SUCCESS);
        assert!(
            TRACKING_COMPLETE_CALLED.load(Ordering::SeqCst),
            "IrpCompleter::complete was not called"
        );
    }

    #[test]
    fn complete_all_status_values() {
        for status in [
            NtStatus::SUCCESS,
            NtStatus::PENDING,
            NtStatus::NOT_SUPPORTED,
            NtStatus::INVALID_PARAMETER,
            NtStatus::UNSUCCESSFUL,
        ] {
            let irp = unsafe { Irp::<NoopCompleter>::from_raw(dummy()) };
            assert_eq!(irp.complete(status), status);
        }
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
        TRACKING_COMPLETE_CALLED.store(false, Ordering::SeqCst);
        let irp = unsafe { Irp::<TrackingCompleter>::from_raw(dummy()) };
        let _ = irp.into_raw();
        assert!(
            !TRACKING_COMPLETE_CALLED.load(Ordering::SeqCst),
            "IrpCompleter::complete must NOT be called by into_raw"
        );
    }

    // ── Pointer identity ──────────────────────────────────────────────────────

    #[test]
    fn as_raw_ptr_matches_input() {
        let ptr = 0xDEAD_C0DEusize as *mut core::ffi::c_void;
        let irp = unsafe { Irp::<NoopCompleter>::from_raw(ptr) };
        let recovered = irp.as_raw_ptr();
        let _ = irp.complete(NtStatus::SUCCESS);
        assert_eq!(recovered, ptr);
    }

    #[test]
    fn two_irps_stay_distinct() {
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

    // ── TrackingCompleter ─────────────────────────────────────────────────────

    #[test]
    fn tracking_completer_starts_false() {
        TRACKING_COMPLETE_CALLED.store(false, Ordering::SeqCst);
        assert!(!TRACKING_COMPLETE_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn tracking_completer_records_call() {
        TRACKING_COMPLETE_CALLED.store(false, Ordering::SeqCst);
        // SAFETY: dummy pointer, TrackingCompleter never dereferences it.
        unsafe { TrackingCompleter::complete(dummy(), 0) };
        assert!(TRACKING_COMPLETE_CALLED.load(Ordering::SeqCst));
    }
}
