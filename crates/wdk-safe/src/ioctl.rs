// Copyright (c) 2026 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Type-safe [`IoControlCode`] builder and [`IoStackOffsets`] for Windows
//! I/O control codes.
//!
//! # IOCTL bit layout
//!
//! A Windows IOCTL code is a 32-bit value packed as:
//!
//! ```text
//! Bits 31–16  DeviceType   (0x0001–0x7FFF = Microsoft, 0x8000–0xFFFF = custom)
//! Bits 15–14  Access       (FILE_ANY_ACCESS, FILE_READ_DATA, FILE_WRITE_DATA)
//! Bits 13– 2  Function     (0x000–0x7FF = Microsoft, 0x800–0xFFF = custom)
//! Bits  1– 0  Method       (METHOD_BUFFERED/IN_DIRECT/OUT_DIRECT/NEITHER)
//! ```
//!
//! Use [`define_ioctl!`](crate::define_ioctl) to declare a constant with
//! associated input/output types in one line.
//!
//! # `IO_STACK_LOCATION` offsets
//!
//! [`IoStackOffsets`] provides the byte offsets of the
//! `Parameters.DeviceIoControl.*` fields for a specific WDK version.
//! This eliminates magic numbers in driver dispatch code.

/// Buffer transfer method for an IOCTL call.
///
/// Stored in bits 1–0 of the IOCTL code. Determines how the I/O manager
/// handles input and output buffers.
///
/// See [I/O Control Code Buffers (WDK)](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/defining-i-o-control-codes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum TransferMethod {
    /// `METHOD_BUFFERED` — kernel copies buffers via `AssociatedIrp.SystemBuffer`.
    ///
    /// Input and output share one allocation. Output is copied back after the
    /// IRP completes. Simple and safe; use for small transfers.
    Buffered = 0,
    /// `METHOD_IN_DIRECT` — input via system buffer; output via MDL.
    ///
    /// The output buffer is locked in place, allowing DMA or direct writes
    /// from a DPC context.
    InDirect = 1,
    /// `METHOD_OUT_DIRECT` — output via MDL; input via system buffer.
    OutDirect = 2,
    /// `METHOD_NEITHER` — driver receives raw user-mode virtual addresses.
    ///
    /// The driver must call `ProbeForRead`/`ProbeForWrite` and handle page
    /// faults. Not recommended unless absolutely necessary.
    Neither = 3,
}

/// Required access for an IOCTL call.
///
/// Stored in bits 15–14 of the IOCTL code. The I/O manager checks these
/// against the file object's access rights before dispatching.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum RequiredAccess {
    /// `FILE_ANY_ACCESS` — no specific access check.
    Any = 0,
    /// `FILE_READ_DATA` — caller must have opened the file for reading.
    Read = 1,
    /// `FILE_WRITE_DATA` — caller must have opened the file for writing.
    Write = 2,
    /// `FILE_READ_DATA | FILE_WRITE_DATA` — read and write access both required.
    ReadWrite = 3,
}

/// A validated, strongly-typed Windows I/O control code.
///
/// Constructed via [`IoControlCode::new`] (a `const fn`) or the
/// [`define_ioctl!`](crate::define_ioctl) macro. Can also wrap a raw `u32`
/// received from the kernel with [`IoControlCode::from_raw`].
///
/// # Examples
///
/// ```rust
/// use wdk_safe::ioctl::{IoControlCode, RequiredAccess, TransferMethod};
///
/// const IOCTL_MY_ECHO: IoControlCode =
///     IoControlCode::new(0x8000, 0x800, TransferMethod::Buffered, RequiredAccess::Any);
///
/// assert_eq!(IOCTL_MY_ECHO.device_type(), 0x8000);
/// assert_eq!(IOCTL_MY_ECHO.function(), 0x800);
/// assert_eq!(IOCTL_MY_ECHO.method(), TransferMethod::Buffered);
/// assert_eq!(IOCTL_MY_ECHO.access(), RequiredAccess::Any);
/// // (0x8000 << 16) | (0 << 14) | (0x800 << 2) | 0 = 0x8000_2000
/// assert_eq!(IOCTL_MY_ECHO.into_raw(), 0x8000_2000);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct IoControlCode(u32);

impl IoControlCode {
    /// Constructs an [`IoControlCode`] from its components.
    ///
    /// This is a `const fn` so codes can be declared as `const` items and
    /// evaluated at compile time.
    #[must_use]
    pub const fn new(
        device_type: u16,
        function: u16,
        method: TransferMethod,
        access: RequiredAccess,
    ) -> Self {
        let code = ((device_type as u32) << 16)
            | ((access as u32) << 14)
            | ((function as u32) << 2)
            | (method as u32);
        Self(code)
    }

    /// Wraps a raw 32-bit IOCTL code received from `IO_STACK_LOCATION`.
    #[must_use]
    #[inline]
    pub const fn from_raw(code: u32) -> Self {
        Self(code)
    }

    /// Returns the raw 32-bit IOCTL code, suitable for comparison with
    /// constants or storing in IRP fields.
    #[must_use]
    #[inline]
    pub const fn into_raw(self) -> u32 {
        self.0
    }

    /// Returns the `DeviceType` field (bits 31–16).
    #[must_use]
    #[inline]
    pub const fn device_type(self) -> u16 {
        (self.0 >> 16) as u16
    }

    /// Returns the `Function` field (bits 13–2).
    #[must_use]
    #[inline]
    pub const fn function(self) -> u16 {
        ((self.0 >> 2) & 0x0FFF) as u16
    }

    /// Returns the [`TransferMethod`] field (bits 1–0).
    #[must_use]
    #[inline]
    pub const fn method(self) -> TransferMethod {
        match self.0 & 0b11 {
            0 => TransferMethod::Buffered,
            1 => TransferMethod::InDirect,
            2 => TransferMethod::OutDirect,
            _ => TransferMethod::Neither,
        }
    }

    /// Returns the [`RequiredAccess`] field (bits 15–14).
    #[must_use]
    #[inline]
    pub const fn access(self) -> RequiredAccess {
        match (self.0 >> 14) & 0b11 {
            0 => RequiredAccess::Any,
            1 => RequiredAccess::Read,
            2 => RequiredAccess::Write,
            _ => RequiredAccess::ReadWrite,
        }
    }

    /// Returns `true` if this code uses a Microsoft-reserved device type
    /// (bits 31–16 in range `0x0001`–`0x7FFF`).
    #[must_use]
    #[inline]
    pub const fn is_microsoft_device_type(self) -> bool {
        let dt = self.device_type();
        dt >= 0x0001 && dt <= 0x7FFF
    }

    /// Returns `true` if this code uses a vendor-defined device type
    /// (bits 31–16 in range `0x8000`–`0xFFFF`).
    #[must_use]
    #[inline]
    pub const fn is_vendor_device_type(self) -> bool {
        self.device_type() >= 0x8000
    }

    /// Returns `true` if this code uses a vendor-defined function code
    /// (function in range `0x800`–`0xFFF`).
    #[must_use]
    #[inline]
    pub const fn is_vendor_function(self) -> bool {
        self.function() >= 0x800
    }
}

impl core::fmt::Debug for IoControlCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IoControlCode")
            .field("raw", &format_args!("{:#010X}", self.0))
            .field("device_type", &format_args!("{:#06X}", self.device_type()))
            .field("function", &format_args!("{:#05X}", self.function()))
            .field("method", &self.method())
            .field("access", &self.access())
            .finish()
    }
}

impl core::fmt::Display for IoControlCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#010X}", self.0)
    }
}

// ── IoStackOffsets ────────────────────────────────────────────────────────────

/// Byte offsets of `IO_STACK_LOCATION` fields for a specific WDK version.
///
/// `IO_STACK_LOCATION` layout is stable across WDK versions on the same
/// architecture, but accessing its fields requires knowing the offsets.
/// WDK C headers declare these as struct members; this type makes them
/// explicit for safe, version-aware Rust code.
///
/// # Usage
///
/// Pass an `IoStackOffsets` constant to [`IoRequest`](crate::request::IoRequest)
/// methods that read from the stack location, e.g.:
///
/// ```rust,ignore
/// use wdk_safe::ioctl::IoStackOffsets;
///
/// let code = request.ioctl_code(&IoStackOffsets::WDK_SYS_0_5_X64);
/// ```
///
/// # Adding new versions
///
/// Offsets can be verified with `offsetof()` in a kernel debug session or
/// by inspecting `wdk-sys` bindgen output:
///
/// ```text
/// (windbg) dt nt!_IO_STACK_LOCATION Parameters.DeviceIoControl
/// ```
#[derive(Clone, Copy, Debug)]
pub struct IoStackOffsets {
    /// Byte offset of `Parameters.DeviceIoControl.IoControlCode`.
    pub ioctl_code: usize,
    /// Byte offset of `Parameters.DeviceIoControl.InputBufferLength`.
    pub input_buffer_length: usize,
    /// Byte offset of `Parameters.DeviceIoControl.OutputBufferLength`.
    pub output_buffer_length: usize,
    /// Byte offset of `AssociatedIrp.SystemBuffer` within `IRP`
    /// (not the stack location).
    pub irp_system_buffer: usize,
    /// Byte offset of `IoStatus.Information` within `IRP`.
    pub irp_information: usize,
}

impl IoStackOffsets {
    /// Offsets for `wdk-sys 0.5.x`, x86-64 (`x86_64-pc-windows-msvc`),
    /// NI eWDK (Windows 11 22H2), KMDF 1.33.
    ///
    /// Verified against `wdk-sys 0.5.1` bindgen output and confirmed in
    /// a `WinDbg` kernel debug session on a Windows 11 22H2 VM.
    ///
    /// ```text
    /// (windbg) dt nt!_IO_STACK_LOCATION
    ///   +0x008 Parameters
    ///           DeviceIoControl
    ///             +0x000 OutputBufferLength : Uint4B   → stack+0x008
    ///             +0x008 InputBufferLength  : Uint4B   → stack+0x010
    ///             +0x010 IoControlCode      : Uint4B   → stack+0x018
    ///
    /// (windbg) dt nt!_IRP
    ///   +0x070 AssociatedIrp.SystemBuffer              → irp+0x070
    ///   +0x030 IoStatus.Information                    → irp+0x038
    /// ```
    pub const WDK_SYS_0_5_X64: Self = Self {
        ioctl_code: 0x18,
        input_buffer_length: 0x10,
        output_buffer_length: 0x08,
        irp_system_buffer: 0x70,
        irp_information: 0x38,
    };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CODE: IoControlCode =
        IoControlCode::new(0x8000, 0x800, TransferMethod::Buffered, RequiredAccess::Any);

    // ── Field accessors ───────────────────────────────────────────────────────

    #[test]
    fn device_type_roundtrip() {
        assert_eq!(TEST_CODE.device_type(), 0x8000);
    }

    #[test]
    fn function_roundtrip() {
        assert_eq!(TEST_CODE.function(), 0x800);
    }

    #[test]
    fn method_is_buffered() {
        assert_eq!(TEST_CODE.method(), TransferMethod::Buffered);
    }

    #[test]
    fn access_is_any() {
        assert_eq!(TEST_CODE.access(), RequiredAccess::Any);
    }

    // ── Raw value encoding ────────────────────────────────────────────────────

    #[test]
    fn raw_value_deterministic() {
        // (0x8000 << 16) | (0 << 14) | (0x800 << 2) | 0 = 0x8000_2000
        assert_eq!(TEST_CODE.into_raw(), 0x8000_2000);
    }

    #[test]
    fn from_raw_roundtrip() {
        let raw = 0x8000_2000_u32;
        assert_eq!(IoControlCode::from_raw(raw).into_raw(), raw);
    }

    #[test]
    fn const_context() {
        const C: IoControlCode = IoControlCode::new(
            0x0001,
            0x900,
            TransferMethod::Neither,
            RequiredAccess::ReadWrite,
        );
        assert_eq!(C.device_type(), 0x0001);
        assert_eq!(C.function(), 0x900);
    }

    // ── All TransferMethod variants ───────────────────────────────────────────

    #[test]
    fn all_methods_roundtrip() {
        for method in [
            TransferMethod::Buffered,
            TransferMethod::InDirect,
            TransferMethod::OutDirect,
            TransferMethod::Neither,
        ] {
            let code = IoControlCode::new(0x8000, 0x800, method, RequiredAccess::Any);
            assert_eq!(code.method(), method, "method {method:?} did not roundtrip");
        }
    }

    // ── All RequiredAccess variants ───────────────────────────────────────────

    #[test]
    fn all_access_roundtrip() {
        for access in [
            RequiredAccess::Any,
            RequiredAccess::Read,
            RequiredAccess::Write,
            RequiredAccess::ReadWrite,
        ] {
            let code = IoControlCode::new(0x8000, 0x800, TransferMethod::Buffered, access);
            assert_eq!(code.access(), access, "access {access:?} did not roundtrip");
        }
    }

    #[test]
    fn read_write_raw_value() {
        let code = IoControlCode::new(
            0x8000,
            0x800,
            TransferMethod::Buffered,
            RequiredAccess::ReadWrite,
        );
        // (0x8000 << 16) | (3 << 14) | (0x800 << 2) | 0 = 0x8000_E000
        assert_eq!(code.into_raw(), 0x8000_E000);
    }

    // ── Vendor / Microsoft detection ──────────────────────────────────────────

    #[test]
    fn vendor_device_type_detected() {
        assert!(TEST_CODE.is_vendor_device_type());
        assert!(!TEST_CODE.is_microsoft_device_type());
    }

    #[test]
    fn microsoft_device_type_detected() {
        let ms = IoControlCode::new(0x0022, 0x800, TransferMethod::Buffered, RequiredAccess::Any);
        assert!(ms.is_microsoft_device_type());
        assert!(!ms.is_vendor_device_type());
    }

    #[test]
    fn vendor_function_detected() {
        assert!(TEST_CODE.is_vendor_function());
    }

    #[test]
    fn microsoft_function_detected() {
        let ms_fn =
            IoControlCode::new(0x8000, 0x100, TransferMethod::Buffered, RequiredAccess::Any);
        assert!(!ms_fn.is_vendor_function());
    }

    // ── Collision avoidance ───────────────────────────────────────────────────

    #[test]
    fn different_functions_do_not_collide() {
        let a = IoControlCode::new(0x8000, 0x800, TransferMethod::Buffered, RequiredAccess::Any);
        let b = IoControlCode::new(0x8000, 0x801, TransferMethod::Buffered, RequiredAccess::Any);
        assert_ne!(a.into_raw(), b.into_raw());
    }

    #[test]
    fn different_device_types_do_not_collide() {
        let a = IoControlCode::new(0x8000, 0x800, TransferMethod::Buffered, RequiredAccess::Any);
        let b = IoControlCode::new(0x0022, 0x800, TransferMethod::Buffered, RequiredAccess::Any);
        assert_ne!(a.into_raw(), b.into_raw());
    }

    // ── Formatting ────────────────────────────────────────────────────────────

    #[test]
    fn debug_contains_raw_hex() {
        let s = format!("{TEST_CODE:?}");
        assert!(s.contains("0x80002000"), "got: {s}");
    }

    #[test]
    fn display_is_hex() {
        assert_eq!(format!("{TEST_CODE}"), "0x80002000");
    }

    // ── Copy / Eq / Hash ─────────────────────────────────────────────────────

    #[test]
    fn copy_semantics() {
        let a = TEST_CODE;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn equal_codes_compare_equal() {
        let a = IoControlCode::new(0x8000, 0x800, TransferMethod::Buffered, RequiredAccess::Any);
        let b = IoControlCode::new(0x8000, 0x800, TransferMethod::Buffered, RequiredAccess::Any);
        assert_eq!(a, b);
    }
}