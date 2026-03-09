// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Type-safe [`IoControlCode`] builder for Windows I/O control codes.
//!
//! A Windows IOCTL code is a 32-bit value packed as:
//!
//! ```text
//! Bits 31–16  DeviceType
//! Bits 15–14  Access
//! Bits 13– 2  Function   (>= 0x800 for custom codes)
//! Bits  1– 0  Method
//! ```

/// Buffer transfer method for an IOCTL call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum TransferMethod {
    /// `METHOD_BUFFERED` — kernel copies buffers via a system buffer.
    Buffered = 0,
    /// `METHOD_IN_DIRECT` — input mapped; output direct.
    InDirect = 1,
    /// `METHOD_OUT_DIRECT` — output mapped; input buffered.
    OutDirect = 2,
    /// `METHOD_NEITHER` — driver gets raw user-mode virtual addresses.
    Neither = 3,
}

/// Required access for an IOCTL call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum RequiredAccess {
    /// `FILE_ANY_ACCESS` — no specific access required.
    Any = 0,
    /// `FILE_READ_DATA` — caller must have read access.
    Read = 1,
    /// `FILE_WRITE_DATA` — caller must have write access.
    Write = 2,
    /// Read and write access both required.
    ReadWrite = 3,
}

/// A validated, strongly-typed Windows I/O control code.
///
/// # Examples
///
/// ```rust
/// use wdk_safe::ioctl::{IoControlCode, RequiredAccess, TransferMethod};
///
/// const IOCTL_ECHO: IoControlCode = IoControlCode::new(
///     0x8000,
///     0x800,
///     TransferMethod::Buffered,
///     RequiredAccess::Any,
/// );
///
/// assert_eq!(IOCTL_ECHO.device_type(), 0x8000);
/// assert_eq!(IOCTL_ECHO.function(), 0x800);
/// ```
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IoControlCode(u32);

impl IoControlCode {
    /// Constructs an [`IoControlCode`] from its components.
    ///
    /// This is a `const fn` so codes can be declared as `const` items.
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

    /// Wraps a raw 32-bit IOCTL code received from the kernel.
    #[must_use]
    #[inline]
    pub const fn from_raw(code: u32) -> Self {
        Self(code)
    }

    /// Returns the raw 32-bit IOCTL code.
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
}

impl core::fmt::Debug for IoControlCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IoControlCode")
            .field("device_type", &format_args!("{:#06X}", self.device_type()))
            .field("function", &format_args!("{:#05X}", self.function()))
            .field("method", &self.method())
            .field("raw", &format_args!("{:#010X}", self.0))
            .finish()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_IOCTL: IoControlCode =
        IoControlCode::new(0x8000, 0x800, TransferMethod::Buffered, RequiredAccess::Any);

    #[test]
    fn device_type_roundtrip() {
        assert_eq!(TEST_IOCTL.device_type(), 0x8000);
    }

    #[test]
    fn function_roundtrip() {
        assert_eq!(TEST_IOCTL.function(), 0x800);
    }

    #[test]
    fn method_is_buffered() {
        assert_eq!(TEST_IOCTL.method(), TransferMethod::Buffered);
    }

    #[test]
    fn raw_value_is_deterministic() {
        // (0x8000 << 16) | (0 << 14) | (0x800 << 2) | 0 = 0x80002000
        assert_eq!(TEST_IOCTL.into_raw(), 0x8000_2000);
    }

    #[test]
    fn from_raw_roundtrip() {
        let raw = 0x8000_2000u32;
        assert_eq!(IoControlCode::from_raw(raw).into_raw(), raw);
    }

    #[test]
    fn const_in_const_context() {
        const CODE: IoControlCode =
            IoControlCode::new(0x0001, 0x900, TransferMethod::Neither, RequiredAccess::ReadWrite);
        assert_eq!(CODE.device_type(), 0x0001);
    }
}