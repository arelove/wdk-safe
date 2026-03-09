// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Safe wrapper around a kernel `DEVICE_OBJECT`.
//!
//! `DEVICE_OBJECT` is represented as an opaque `*mut c_void` so this crate
//! does not depend on `wdk-sys` and can be tested on the host.

use core::marker::PhantomData;

/// A non-owning, safe reference to a kernel `DEVICE_OBJECT`.
///
/// The underlying object is owned by the I/O manager and remains valid
/// for the lifetime of the driver.
pub struct Device {
    raw: *mut core::ffi::c_void,
    /// Prevent Device from being sent across threads without synchronisation.
    _not_send: PhantomData<*mut ()>,
}

impl Device {
    /// Wraps a raw `DEVICE_OBJECT` pointer.
    ///
    /// # Safety
    ///
    /// - `raw` must be non-null and point to a valid `DEVICE_OBJECT`.
    /// - The pointer must remain valid for the lifetime of this `Device`.
    #[must_use]
    pub unsafe fn from_raw(raw: *mut core::ffi::c_void) -> Self {
        debug_assert!(!raw.is_null(), "Device::from_raw called with null pointer");
        Self { raw, _not_send: PhantomData }
    }

    /// Returns the raw pointer to the underlying `DEVICE_OBJECT`.
    #[must_use]
    #[inline]
    pub fn as_raw_ptr(&self) -> *mut core::ffi::c_void {
        self.raw
    }
}