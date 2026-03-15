// Copyright (c) 2026 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Dispatch thunk helper for generating WDM major-function callbacks.
//!
//! # Problem
//!
//! Windows expects dispatch routines of the form:
//!
//! ```c
//! NTSTATUS DispatchRead(PDEVICE_OBJECT device, PIRP irp);
//! ```
//!
//! Writing these by hand for every major function is boilerplate-heavy and
//! error-prone (easy to forget `IoGetCurrentIrpStackLocation`, easy to use
//! wrong ABI). The [`crate::dispatch_fn`] macro generates the boilerplate correctly
//! and routes to the appropriate [`WdmDriver`](crate::WdmDriver) method.
//!
//! # ABI note
//!
//! On MSVC kernel targets, `MajorFunction` array entries and `AddDevice` are
//! typed as `unsafe extern "C"`. Although `extern "C"` and `extern "system"`
//! are identical at the machine level on x86-64 and `AArch64`, the Rust type
//! checker compares ABI strings literally — so `"C"` is required here.
//!
//! # Example
//!
//! ```rust,ignore
//! use wdk_safe::dispatch_fn;
//!
//! dispatch_fn!(
//!     pub dispatch_read = HidFilterDriver, on_read, KernelCompleter,
//!     irp_stack = irp_current_stack
//! );
//! dispatch_fn!(
//!     pub dispatch_create = HidFilterDriver, on_create, KernelCompleter,
//!     irp_stack = irp_current_stack
//! );
//!
//! // In DriverEntry:
//! unsafe {
//!     (*driver).MajorFunction[IRP_MJ_READ as usize] = Some(dispatch_read);
//!     (*driver).MajorFunction[IRP_MJ_CREATE as usize] = Some(dispatch_create);
//! }
//! ```

/// Generates a `unsafe extern "C" fn(PDEVICE_OBJECT, PIRP) -> NTSTATUS`
/// dispatch thunk that bridges the raw WDM ABI to a [`crate::WdmDriver`] method.
///
/// # Syntax
///
/// ```rust,ignore
/// dispatch_fn!(
///     $vis $fn_name = $DriverType, $method, $CompleterType,
///     irp_stack = $irp_stack_fn
/// );
/// ```
///
/// Where:
/// - `$vis` — visibility (`pub`, `pub(crate)`, or empty).
/// - `$fn_name` — the name of the generated function.
/// - `$DriverType` — a type implementing [`crate::WdmDriver`]`<$CompleterType>`.
/// - `$method` — the [`crate::WdmDriver`] method to call (e.g. `on_read`).
/// - `$CompleterType` — the [`IrpCompleter`](crate::IrpCompleter) type.
/// - `$irp_stack_fn` — a function with signature `unsafe fn(*mut IRP) -> *mut
///   IO_STACK_LOCATION` that implements `IoGetCurrentIrpStackLocation`. This
///   must be in scope at the call site and is passed explicitly to avoid magic
///   scope requirements.
///
/// # Safety contract of the generated function
///
/// - `device` and `irp` must be valid non-null pointers supplied by the I/O
///   manager at the correct IRQL.
/// - `$irp_stack_fn` must return the correct `IO_STACK_LOCATION` for `irp`.
///
/// [`WdmDriver`]: crate::WdmDriver
#[macro_export]
macro_rules! dispatch_fn {
    ($vis:vis $name:ident = $driver:ty, $method:ident, $completer:ty, irp_stack = $irp_stack_fn:expr) => {
        $vis unsafe extern "C" fn $name(
            device: wdk_sys::PDEVICE_OBJECT,
            irp: wdk_sys::PIRP,
        ) -> wdk_sys::NTSTATUS {
            let status = unsafe {
                let stack = $irp_stack_fn(irp);
                let req = $crate::IoRequest::<$completer>::from_raw(
                    irp.cast(),
                    stack.cast(),
                );
                let dev = $crate::Device::from_raw(device.cast());
                <$driver as $crate::WdmDriver<$completer>>::$method(&dev, req)
            };
            status.into_raw()
        }
    };
}
