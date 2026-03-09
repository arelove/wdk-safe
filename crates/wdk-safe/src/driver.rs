// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`KmdfDriver`] trait — implement this to define your driver's behaviour.

use crate::{Device, IoRequest, NtStatus};

/// The primary trait for a KMDF driver.
///
/// Implement this on a unit struct. Methods you do not override return
/// `STATUS_NOT_SUPPORTED` by default, which is the correct response for
/// major functions a driver does not handle.
///
/// # Example
///
/// ```rust
/// use wdk_safe::{Device, IoRequest, KmdfDriver, NtStatus};
///
/// struct MyDriver;
///
/// impl KmdfDriver for MyDriver {
///     fn on_device_control(_device: &Device, request: IoRequest<'_>) -> NtStatus {
///         request.complete(NtStatus::SUCCESS)
///     }
/// }
/// ```
pub trait KmdfDriver {
    /// `IRP_MJ_CREATE` — a handle to the device is opened.
    fn on_create(_device: &Device, request: IoRequest<'_>) -> NtStatus {
        request.complete(NtStatus::SUCCESS)
    }

    /// `IRP_MJ_CLOSE` — a handle to the device is closed.
    fn on_close(_device: &Device, request: IoRequest<'_>) -> NtStatus {
        request.complete(NtStatus::SUCCESS)
    }

    /// `IRP_MJ_READ` — data is read from the device.
    fn on_read(_device: &Device, request: IoRequest<'_>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_WRITE` — data is written to the device.
    fn on_write(_device: &Device, request: IoRequest<'_>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_DEVICE_CONTROL` — the primary IOCTL dispatch.
    fn on_device_control(_device: &Device, request: IoRequest<'_>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDriver;
    impl KmdfDriver for TestDriver {}

    #[test]
    fn default_create_succeeds() {
        let dev = unsafe { Device::from_raw(1usize as *mut _) };
        let req = unsafe { IoRequest::from_raw(1usize as *mut _, 1usize as *const _) };
        assert!(TestDriver::on_create(&dev, req).is_success());
    }

    #[test]
    fn default_read_not_supported() {
        let dev = unsafe { Device::from_raw(1usize as *mut _) };
        let req = unsafe { IoRequest::from_raw(1usize as *mut _, 1usize as *const _) };
        assert!(TestDriver::on_read(&dev, req).is_error());
    }
}