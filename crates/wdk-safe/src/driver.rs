// Copyright (c) 2026 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`WdmDriver`] trait — implement this to define your driver's dispatch
//! behaviour.
//!
//! # Naming note
//!
//! The trait is called `WdmDriver` rather than `KmdfDriver` because it
//! operates at the **WDM** (Windows Driver Model) dispatch level: it deals
//! directly with `DEVICE_OBJECT` and `IRP` pointers. KMDF is a framework
//! **above** WDM and uses `WDFDEVICE` / `WDFREQUEST` handles instead.
//!
//! # Default implementations
//!
//! Every method has a default that completes the IRP with an appropriate
//! status so you only override what your driver handles:
//!
//! | Method | Default status |
//! |--------|---------------|
//! | `on_create`, `on_close`, `on_cleanup` | `STATUS_SUCCESS` |
//! | All others | `STATUS_NOT_SUPPORTED` |
//!
//! `STATUS_NOT_SUPPORTED` is the correct response for IRP major functions a
//! driver deliberately does not handle. Do not confuse it with
//! `STATUS_INVALID_DEVICE_REQUEST` — use the latter when the device type
//! does not support the operation at all (e.g. a non-readable device
//! receiving `IRP_MJ_READ`).
//!
//! # Filter drivers
//!
//! If you are writing a filter driver, **you must override `on_pnp` and
//! `on_power`** to forward those IRPs down the stack. The defaults return
//! `STATUS_NOT_SUPPORTED`, which will prevent the system from entering sleep
//! states and break `PnP` lifecycle management.
//!
//! See [`WdmFilterDriver`] for a separate trait with correct filter defaults.
//!
//! # Example
//!
//! ```rust
//! use wdk_safe::{irp::NoopCompleter, Device, IoRequest, WdmDriver, NtStatus};
//!
//! struct MyDriver;
//!
//! impl WdmDriver<NoopCompleter> for MyDriver {
//!     fn on_device_control(
//!         _device: &Device<'_>,
//!         request: IoRequest<'_, NoopCompleter>,
//!     ) -> NtStatus {
//!         // Handle IOCTL — complete with success and zero bytes transferred.
//!         request.complete(NtStatus::SUCCESS)
//!     }
//! }
//! ```

use crate::{irp::IrpCompleter, Device, IoRequest, NtStatus};

/// The primary trait for a WDM kernel-mode driver.
///
/// Implement this on a unit struct. Register the dispatch thunks generated
/// by [`dispatch_fn!`](crate::dispatch_fn) in `DriverEntry`. Only override
/// the IRP major functions your driver handles.
///
/// # Type parameter
///
/// `C` is the [`IrpCompleter`] used in dispatch callbacks. In production
/// this is a zero-sized type from your driver crate that calls
/// `IofCompleteRequest`. In tests use
/// [`NoopCompleter`](crate::irp::NoopCompleter).
///
/// # Filter drivers
///
/// For filter drivers that need to forward `PnP` and Power IRPs, see
/// [`WdmFilterDriver`] which provides correct forwarding defaults.
pub trait WdmDriver<C: IrpCompleter> {
    /// `IRP_MJ_CREATE` — a user-mode or kernel-mode client opened a handle
    /// to the device.
    ///
    /// # IRQL
    ///
    /// Called at `IRQL == PASSIVE_LEVEL`.
    ///
    /// # Default
    ///
    /// Completes with `STATUS_SUCCESS`.
    #[must_use]
    fn on_create(_device: &Device<'_>, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::SUCCESS)
    }

    /// `IRP_MJ_CLOSE` — the last handle to the device file was closed.
    ///
    /// # IRQL
    ///
    /// Called at `IRQL == PASSIVE_LEVEL`.
    ///
    /// # Default
    ///
    /// Completes with `STATUS_SUCCESS`.
    #[must_use]
    fn on_close(_device: &Device<'_>, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::SUCCESS)
    }

    /// `IRP_MJ_READ` — data is being read from the device.
    ///
    /// # IRQL
    ///
    /// Called at `IRQL <= DISPATCH_LEVEL` (typically `PASSIVE_LEVEL` or
    /// `APC_LEVEL`).
    ///
    /// # Default
    ///
    /// Completes with `STATUS_NOT_SUPPORTED`.
    #[must_use]
    fn on_read(_device: &Device<'_>, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_WRITE` — data is being written to the device.
    ///
    /// # IRQL
    ///
    /// Called at `IRQL <= DISPATCH_LEVEL`.
    ///
    /// # Default
    ///
    /// Completes with `STATUS_NOT_SUPPORTED`.
    #[must_use]
    fn on_write(_device: &Device<'_>, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_DEVICE_CONTROL` — user-mode IOCTL via `DeviceIoControl`.
    ///
    /// # IRQL
    ///
    /// Called at `IRQL == PASSIVE_LEVEL`.
    ///
    /// # Default
    ///
    /// Completes with `STATUS_NOT_SUPPORTED`.
    #[must_use]
    fn on_device_control(_device: &Device<'_>, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_INTERNAL_DEVICE_CONTROL` — kernel-mode internal IOCTL.
    ///
    /// Used by class/port driver pairs (e.g. HID class → HID miniport).
    ///
    /// # IRQL
    ///
    /// Called at `IRQL <= DISPATCH_LEVEL`.
    ///
    /// # Default
    ///
    /// Completes with `STATUS_NOT_SUPPORTED`.
    #[must_use]
    fn on_internal_device_control(_device: &Device<'_>, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_POWER` — system or device power state transition.
    ///
    /// # IRQL
    ///
    /// May be called at `IRQL == PASSIVE_LEVEL` or `IRQL == DISPATCH_LEVEL`
    /// depending on the power IRP type. A driver that passes power IRPs
    /// through must call `PoStartNextPowerIrp` + `IoSkipCurrentIrpStackLocation`
    /// + `PoCallDriver` rather than simply completing the IRP.
    ///
    /// # Default
    ///
    /// Completes with `STATUS_NOT_SUPPORTED`. **Override this** in any driver
    /// that is attached to a device stack — failing to pass power IRPs
    /// prevents system sleep/hibernate.
    ///
    /// For filter drivers, use [`WdmFilterDriver`] which provides the correct
    /// forwarding default.
    #[must_use]
    fn on_power(_device: &Device<'_>, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_PNP` — Plug-and-Play request.
    ///
    /// # IRQL
    ///
    /// Called at `IRQL == PASSIVE_LEVEL`.
    ///
    /// # Default
    ///
    /// Completes with `STATUS_NOT_SUPPORTED`. **Override this** in filter
    /// drivers — `PnP` IRPs must be forwarded down the stack.
    ///
    /// For filter drivers, use [`WdmFilterDriver`] which provides the correct
    /// forwarding default.
    #[must_use]
    fn on_pnp(_device: &Device<'_>, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_CLEANUP` — all handles to the device file were closed, but
    /// the file object still exists (outstanding I/O may remain).
    ///
    /// Use this to cancel pending IRPs associated with the file object.
    ///
    /// # IRQL
    ///
    /// Called at `IRQL == PASSIVE_LEVEL`.
    ///
    /// # Default
    ///
    /// Completes with `STATUS_SUCCESS`.
    #[must_use]
    fn on_cleanup(_device: &Device<'_>, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::SUCCESS)
    }

    /// `IRP_MJ_QUERY_INFORMATION` — query file/device information.
    ///
    /// # IRQL
    ///
    /// Called at `IRQL == PASSIVE_LEVEL`.
    ///
    /// # Default
    ///
    /// Completes with `STATUS_NOT_SUPPORTED`.
    #[must_use]
    fn on_query_information(_device: &Device<'_>, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }

    /// `IRP_MJ_SET_INFORMATION` — set file/device information.
    ///
    /// # IRQL
    ///
    /// Called at `IRQL == PASSIVE_LEVEL`.
    ///
    /// # Default
    ///
    /// Completes with `STATUS_NOT_SUPPORTED`.
    #[must_use]
    fn on_set_information(_device: &Device<'_>, request: IoRequest<'_, C>) -> NtStatus {
        request.complete(NtStatus::NOT_SUPPORTED)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irp::NoopCompleter;

    struct DefaultDriver;
    impl WdmDriver<NoopCompleter> for DefaultDriver {}

    fn dev() -> Device<'static> {
        // SAFETY: dummy non-null pointer — never dereferenced.
        unsafe { Device::from_raw(1usize as *mut _) }
    }

    fn req() -> IoRequest<'static, NoopCompleter> {
        // SAFETY: dummy non-null pointers — never dereferenced.
        unsafe { IoRequest::from_raw(1usize as *mut _, 1usize as *const _) }
    }

    // ── Default return values ─────────────────────────────────────────────────

    #[test]
    fn default_create_is_success() {
        assert!(DefaultDriver::on_create(&dev(), req()).is_success());
    }

    #[test]
    fn default_close_is_success() {
        assert!(DefaultDriver::on_close(&dev(), req()).is_success());
    }

    #[test]
    fn default_cleanup_is_success() {
        assert!(DefaultDriver::on_cleanup(&dev(), req()).is_success());
    }

    #[test]
    fn default_read_is_not_supported() {
        assert_eq!(DefaultDriver::on_read(&dev(), req()), NtStatus::NOT_SUPPORTED);
    }

    #[test]
    fn default_write_is_not_supported() {
        assert_eq!(DefaultDriver::on_write(&dev(), req()), NtStatus::NOT_SUPPORTED);
    }

    #[test]
    fn default_device_control_is_not_supported() {
        assert_eq!(
            DefaultDriver::on_device_control(&dev(), req()),
            NtStatus::NOT_SUPPORTED
        );
    }

    #[test]
    fn default_internal_device_control_is_not_supported() {
        assert_eq!(
            DefaultDriver::on_internal_device_control(&dev(), req()),
            NtStatus::NOT_SUPPORTED
        );
    }

    #[test]
    fn default_power_is_not_supported() {
        assert_eq!(DefaultDriver::on_power(&dev(), req()), NtStatus::NOT_SUPPORTED);
    }

    #[test]
    fn default_pnp_is_not_supported() {
        assert_eq!(DefaultDriver::on_pnp(&dev(), req()), NtStatus::NOT_SUPPORTED);
    }

    #[test]
    fn default_query_information_is_not_supported() {
        assert_eq!(
            DefaultDriver::on_query_information(&dev(), req()),
            NtStatus::NOT_SUPPORTED
        );
    }

    #[test]
    fn default_set_information_is_not_supported() {
        assert_eq!(
            DefaultDriver::on_set_information(&dev(), req()),
            NtStatus::NOT_SUPPORTED
        );
    }

    // ── Custom override ───────────────────────────────────────────────────────

    #[test]
    fn custom_override_is_called() {
        struct EchoDriver;
        impl WdmDriver<NoopCompleter> for EchoDriver {
            fn on_device_control(
                _device: &Device<'_>,
                request: IoRequest<'_, NoopCompleter>,
            ) -> NtStatus {
                request.complete(NtStatus::SUCCESS)
            }
        }
        assert_eq!(
            EchoDriver::on_device_control(&dev(), req()),
            NtStatus::SUCCESS
        );
        // Default methods still work.
        assert_eq!(EchoDriver::on_read(&dev(), req()), NtStatus::NOT_SUPPORTED);
    }

    #[test]
    fn custom_create_with_different_status() {
        struct StrictDriver;
        impl WdmDriver<NoopCompleter> for StrictDriver {
            fn on_create(_device: &Device<'_>, request: IoRequest<'_, NoopCompleter>) -> NtStatus {
                request.complete(NtStatus::ACCESS_DENIED)
            }
        }
        assert_eq!(
            StrictDriver::on_create(&dev(), req()),
            NtStatus::ACCESS_DENIED
        );
    }

    // ── All defaults are errors or success (no unexpected values) ─────────────

    #[test]
    fn not_supported_defaults_are_errors() {
        for status in [
            DefaultDriver::on_read(&dev(), req()),
            DefaultDriver::on_write(&dev(), req()),
            DefaultDriver::on_device_control(&dev(), req()),
            DefaultDriver::on_internal_device_control(&dev(), req()),
            DefaultDriver::on_power(&dev(), req()),
            DefaultDriver::on_pnp(&dev(), req()),
            DefaultDriver::on_query_information(&dev(), req()),
            DefaultDriver::on_set_information(&dev(), req()),
        ] {
            assert!(status.is_error(), "expected error, got {status:?}");
        }
    }

    #[test]
    fn success_defaults_are_successes() {
        for status in [
            DefaultDriver::on_create(&dev(), req()),
            DefaultDriver::on_close(&dev(), req()),
            DefaultDriver::on_cleanup(&dev(), req()),
        ] {
            assert!(status.is_success(), "expected success, got {status:?}");
        }
    }
}