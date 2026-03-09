// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Safe wrapper around a kernel `DEVICE_OBJECT`.
//!
//! `DEVICE_OBJECT` is represented as an opaque `*mut c_void` so this crate
//! has zero dependency on `wdk-sys` and can be tested on the host.

use core::marker::PhantomData;

/// A non-owning, safe reference to a kernel `DEVICE_OBJECT`.
///
/// The lifetime of the underlying object is managed by the I/O manager —
/// it is valid from driver load through all dispatch calls until
/// [`IoDeleteDevice`](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-iodeletedevice)
/// is called in `DriverUnload`.
///
/// `Device` is intentionally `!Send` because kernel device objects must only
/// be accessed within the synchronisation constraints of the calling IRQL.
/// If cross-thread access is needed, the driver is responsible for its own
/// synchronisation and must cast through the raw pointer.
#[derive(Debug)]
pub struct Device {
    raw: *mut core::ffi::c_void,
    _not_send: PhantomData<*mut ()>,
}

impl Device {
    /// Wraps a raw `DEVICE_OBJECT` pointer.
    ///
    /// # Safety
    ///
    /// - `raw` must be non-null and point to a valid, fully-initialised
    ///   `DEVICE_OBJECT`.
    /// - The pointer must remain valid for at least as long as this `Device` is
    ///   in scope.
    #[must_use]
    #[inline]
    pub unsafe fn from_raw(raw: *mut core::ffi::c_void) -> Self {
        debug_assert!(!raw.is_null(), "Device::from_raw: null pointer");
        Self {
            raw,
            _not_send: PhantomData,
        }
    }

    /// Returns the raw `*mut DEVICE_OBJECT` pointer.
    #[must_use]
    #[inline]
    pub const fn as_raw_ptr(&self) -> *mut core::ffi::c_void {
        self.raw
    }

    /// Returns `true` when the underlying pointer is non-null.
    ///
    /// Always `true` for correctly constructed `Device` values; exposed for
    /// use in `debug_assert!` chains in driver code.
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

    fn dummy_device() -> Device {
        // SAFETY: non-null dummy pointer — never dereferenced.
        unsafe { Device::from_raw(1usize as *mut _) }
    }

    #[test]
    fn as_raw_ptr_roundtrip() {
        let ptr = 0xDEAD_BEEFusize as *mut core::ffi::c_void;
        let dev = unsafe { Device::from_raw(ptr) };
        assert_eq!(dev.as_raw_ptr(), ptr);
    }

    #[test]
    fn is_valid_for_non_null() {
        assert!(dummy_device().is_valid());
    }
}
