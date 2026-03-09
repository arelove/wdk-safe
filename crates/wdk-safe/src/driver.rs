// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`KmdfDriver`] trait — implement this to define your driver's behaviour.
//!
//! The trait is generic over `C: IrpCompleter` so it compiles and tests on
//! the host without a WDK installation while still producing zero-cost
//! abstractions in the kernel binary.

use crate::{irp::IrpCompleter, Device, IoRequest, NtStatus};

/// The primary trait for a KMDF kernel-mode driver.
///
/// Implement this on a unit struct and register the generated dispatch
/// functions in `DriverEntry`. Methods you do not override return
/// `STATUS_NOT_SUPPORTED`, which is the correct response for major functions
/// a driver deliberately does not handle.
///
/// # Type parameter
///
/// `C` is the [`IrpCompleter`] used in dispatch callbacks. In production this
/// is a zero-sized type from your driver crate that calls
/// `IoCompleteRequest`; in tests use
/// [`NoopCompleter`](`crate::irp::NoopCompleter`).
///
/// # Example
///
/// ```rust
/// use wdk_safe::{irp::NoopCompleter, Device, IoRequest, KmdfDriver, NtStatus};
///
/// struct MyDriver;
///
/// impl KmdfDriver<NoopCompleter> for MyDriver {
///     fn on_device_control(_device: &Device, request: IoRequest<'_, NoopCompleter>) -> NtStatus {
///         request.complete(NtStatus::SUCCESS)
///     }
/// }
/// ```
pub trait KmdfDriver<C: IrpCompleter> {
    /// `IRP_MJ_CREATE` — a handle to the device was opened.
    ///
    /// Default: complete with `STATUS_SUCCESS`.
    #[must_use]
    fn on_create(_device: &Device, request: IoRequest<C>) -> NtStatus {
        request.complete(NtStatus::SUCCESS)
    }

    /// `IRP_MJ_CLOSE` — the last handle to the device was closed.
    ///
    /// Default: complete with `STATUS_SUCCESS`.
    #[must_use]
    fn on_close(_device: &Device, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::SUCCESS)
    }

    /// `IRP_MJ_READ` — data is being read from the device.
    ///
    /// Default: complete with `STATUS_NOT_SUPPORTED`.
    #[must_use]
    fn on_read(_device: &Device, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_WRITE` — data is being written to the device.
    ///
    /// Default: complete with `STATUS_NOT_SUPPORTED`.
    #[must_use]
    fn on_write(_device: &Device, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_DEVICE_CONTROL` — user-mode IOCTL via `DeviceIoControl`.
    ///
    /// Default: complete with `STATUS_NOT_SUPPORTED`.
    #[must_use]
    fn on_device_control(_device: &Device, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_INTERNAL_DEVICE_CONTROL` — kernel-mode IOCTL.
    ///
    /// Used by class/port driver pairs (e.g. HID class → HID port).
    /// Default: complete with `STATUS_NOT_SUPPORTED`.
    #[must_use]
    fn on_internal_device_control(_device: &Device, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_POWER` — system power state transition.
    ///
    /// Default: complete with `STATUS_NOT_SUPPORTED`.
    /// A real driver that passes power IRPs through must call
    /// `PoStartNextPowerIrp` + `IoSkipCurrentIrpStackLocation` +
    /// `PoCallDriver` instead.
    #[must_use]
    fn on_power(_device: &Device, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_PNP` — Plug-and-Play request.
    ///
    /// Default: complete with `STATUS_NOT_SUPPORTED`.
    #[must_use]
    fn on_pnp(_device: &Device, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_CLEANUP` — all handles to the device file were closed.
    ///
    /// Default: complete with `STATUS_SUCCESS`.
    #[must_use]
    fn on_cleanup(_device: &Device, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::SUCCESS)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irp::NoopCompleter;

    struct TestDriver;
    impl KmdfDriver<NoopCompleter> for TestDriver {}

    fn dev() -> Device {
        // SAFETY: dummy non-null pointer — never dereferenced.
        unsafe { Device::from_raw(1usize as *mut _) }
    }

    fn req() -> IoRequest<'static, NoopCompleter> {
        // SAFETY: dummy non-null pointers — never dereferenced.
        unsafe { IoRequest::from_raw(1usize as *mut _, 1usize as *const _) }
    }

    #[test]
    fn default_create_succeeds() {
        assert!(TestDriver::on_create(&dev(), req()).is_success());
    }

    #[test]
    fn default_close_succeeds() {
        assert!(TestDriver::on_close(&dev(), req()).is_success());
    }

    #[test]
    fn default_cleanup_succeeds() {
        assert!(TestDriver::on_cleanup(&dev(), req()).is_success());
    }

    #[test]
    fn default_read_not_supported() {
        assert!(TestDriver::on_read(&dev(), req()).is_error());
    }

    #[test]
    fn default_write_not_supported() {
        assert!(TestDriver::on_write(&dev(), req()).is_error());
    }

    #[test]
    fn default_device_control_not_supported() {
        assert!(TestDriver::on_device_control(&dev(), req()).is_error());
    }

    #[test]
    fn default_internal_device_control_not_supported() {
        assert!(TestDriver::on_internal_device_control(&dev(), req()).is_error());
    }

    #[test]
    fn default_power_not_supported() {
        assert!(TestDriver::on_power(&dev(), req()).is_error());
    }

    #[test]
    fn default_pnp_not_supported() {
        assert!(TestDriver::on_pnp(&dev(), req()).is_error());
    }

    #[test]
    fn custom_override_is_called() {
        struct EchoDriver;
        impl KmdfDriver<NoopCompleter> for EchoDriver {
            fn on_device_control(
                _device: &Device,
                request: IoRequest<'_, NoopCompleter>,
            ) -> NtStatus {
                request.complete(NtStatus::SUCCESS)
            }
        }
        assert!(EchoDriver::on_device_control(&dev(), req()).is_success());
    }
}
