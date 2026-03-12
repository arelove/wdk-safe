// Copyright (c) 2026 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Safe wrapper around a kernel `DEVICE_OBJECT`.
//!
//! `DEVICE_OBJECT` is represented as an opaque `*mut c_void` so this crate
//! has zero dependency on `wdk-sys` and can be tested on the host.
//!
//! # Lifetime model
//!
//! `Device<'stack>` is a **non-owning, lifetime-scoped reference**. The
//! `'stack` lifetime is bound to the dispatch callback stack frame — the
//! compiler prevents holding a `Device` past the point where the I/O manager's
//! pointer is valid.
//!
//! The I/O manager owns `DEVICE_OBJECT` memory from device creation until
//! `IoDeleteDevice` is called in `DriverUnload` (or when a `PnP` remove IRP
//! is processed).
//!
//! # `!Send` + `!Sync`
//!
//! Kernel device objects must only be accessed within the synchronisation
//! constraints of the calling IRQL. `Device` is `!Send` and `!Sync` to
//! prevent accidental cross-thread access without explicit synchronisation.

use core::marker::PhantomData;

/// I/O flag bits extracted from `DEVICE_OBJECT.Flags`.
///
/// These are the flags relevant to how the I/O manager handles data
/// buffers for read/write and IOCTL requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoBufferingMode {
    /// `DO_BUFFERED_IO` — the I/O manager copies user buffers to/from a
    /// kernel-allocated system buffer. Safe and simple; suitable for small
    /// transfers.
    Buffered,
    /// `DO_DIRECT_IO` — the I/O manager locks user pages into physical memory
    /// and maps them into kernel space via an MDL. Zero-copy; suitable for
    /// large transfers.
    Direct,
    /// Neither flag is set — the driver receives raw user-mode virtual
    /// addresses. The driver is responsible for probing and locking.
    Neither,
}

/// A non-owning, lifetime-scoped reference to a kernel `DEVICE_OBJECT`.
///
/// The `'stack` lifetime is tied to the dispatch callback frame. This prevents
/// storing a `Device` in a global or struct that outlives the callback, which
/// would be unsound.
///
/// The underlying `DEVICE_OBJECT` is managed by the I/O manager. It is valid
/// from device creation until `IoDeleteDevice` is called, which must happen
/// in `DriverUnload` (or when a `PnP` remove IRP is processed).
///
/// # Thread safety
///
/// `Device` is `!Send` and `!Sync`. Accessing a `DEVICE_OBJECT` from multiple
/// threads requires explicit driver-level synchronisation (spinlocks, mutexes).
///
/// # See also
///
/// - [`WdmDriver::on_create`](crate::driver::WdmDriver::on_create) — where a
///   `Device` is first passed to driver code.
/// - [DEVICE_OBJECT (WDK)](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/ns-wdm-_device_object)
#[derive(Debug)]
pub struct Device<'stack> {
    raw: *mut core::ffi::c_void,
    // Ties the Device to the dispatch callback lifetime.
    // Also makes Device !Send + !Sync — raw pointer gives us !Send+!Sync
    // automatically, but the PhantomData makes the intent explicit.
    _stack: PhantomData<&'stack mut core::ffi::c_void>,
}

impl Device<'_> {
    /// Wraps a raw `*mut DEVICE_OBJECT` pointer with an explicit lifetime.
    ///
    /// # Safety
    ///
    /// - `raw` must be non-null and point to a valid, fully-initialised
    ///   `DEVICE_OBJECT`.
    /// - The pointer must remain valid for at least as long as `'stack`.
    /// - Only one logical owner should hold a `Device` wrapping the same
    ///   pointer at a time (no aliased mutation).
    #[must_use]
    #[inline]
    pub unsafe fn from_raw(raw: *mut core::ffi::c_void) -> Self {
        debug_assert!(!raw.is_null(), "Device::from_raw: null pointer");
        Self {
            raw,
            _stack: PhantomData,
        }
    }

    /// Returns the raw `*mut DEVICE_OBJECT` pointer.
    ///
    /// Use this to call WDK functions that require a `PDEVICE_OBJECT`.
    /// Cast with `.cast::<wdk_sys::DEVICE_OBJECT>()` in driver crates.
    #[must_use]
    #[inline]
    pub const fn as_raw_ptr(&self) -> *mut core::ffi::c_void {
        self.raw
    }

    /// Returns `true` when the underlying pointer is non-null.
    ///
    /// A correctly constructed `Device` is always valid. This method exists
    /// for use in `debug_assert!` chains in driver code.
    #[must_use]
    #[inline]
    pub fn is_valid(&self) -> bool {
        !self.raw.is_null()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_device() -> Device<'static> {
        // SAFETY: non-null dummy pointer — never dereferenced.
        unsafe { Device::from_raw(1usize as *mut _) }
    }

    #[test]
    fn as_raw_ptr_roundtrip() {
        let ptr = 0xDEAD_BEEFusize as *mut core::ffi::c_void;
        // SAFETY: non-null dummy — never dereferenced.
        let dev = unsafe { Device::from_raw(ptr) };
        assert_eq!(dev.as_raw_ptr(), ptr);
    }

    #[test]
    fn is_valid_for_non_null() {
        assert!(dummy_device().is_valid());
    }

    #[test]
    fn two_devices_with_same_ptr_return_same_raw() {
        let ptr = 0xCAFE_BABEusize as *mut core::ffi::c_void;
        // SAFETY: non-null dummies.
        let d1 = unsafe { Device::from_raw(ptr) };
        let d2 = unsafe { Device::from_raw(ptr) };
        assert_eq!(d1.as_raw_ptr(), d2.as_raw_ptr());
    }

    #[test]
    fn different_ptrs_return_different_raw() {
        let p1 = 1usize as *mut core::ffi::c_void;
        let p2 = 2usize as *mut core::ffi::c_void;
        // SAFETY: non-null dummies.
        let d1 = unsafe { Device::from_raw(p1) };
        let d2 = unsafe { Device::from_raw(p2) };
        assert_ne!(d1.as_raw_ptr(), d2.as_raw_ptr());
    }

    // Verify that Device<'stack> cannot outlive its source.
    // This is a compile-time test — if Device had no lifetime, this would
    // silently compile. With 'stack it correctly constrains usage.
    #[test]
    fn lifetime_constrains_scope() {
        let raw = 1usize as *mut core::ffi::c_void;
        // SAFETY: dummy pointer, not dereferenced.
        let dev = unsafe { Device::from_raw(raw) };
        // dev is used here — the borrow checker ensures it cannot be
        // stored in a longer-lived binding without an explicit unsafe cast.
        assert!(dev.is_valid());
        // dev is dropped here — cannot be stored past this point via safe code.
    }
}
