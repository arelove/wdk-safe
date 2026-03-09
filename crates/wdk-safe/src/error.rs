// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`NtStatus`] — an idiomatic Rust newtype over Windows `NTSTATUS` codes.

/// A strongly-typed wrapper around a Windows `NTSTATUS` code.
///
/// `NTSTATUS` is a 32-bit integer. Values >= 0 indicate success
/// (mirrors the `NT_SUCCESS` C macro). Values with bits 31-30 == `11`
/// are errors.
///
/// # Examples
///
/// ```rust
/// use wdk_safe::NtStatus;
///
/// assert!(NtStatus::SUCCESS.is_success());
/// assert!(!NtStatus::UNSUCCESSFUL.is_success());
/// assert_eq!(NtStatus::SUCCESS.into_raw(), 0);
/// ```
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct NtStatus(i32);

impl NtStatus {
    /// `STATUS_SUCCESS` (0x00000000) — operation completed successfully.
    pub const SUCCESS: Self = Self(0x0000_0000_u32 as i32);

    /// `STATUS_UNSUCCESSFUL` (0xC0000001) — operation was unsuccessful.
    pub const UNSUCCESSFUL: Self = Self(0xC000_0001_u32 as i32);

    /// `STATUS_INVALID_PARAMETER` (0xC000000D) — invalid parameter passed.
    pub const INVALID_PARAMETER: Self = Self(0xC000_000D_u32 as i32);

    /// `STATUS_NOT_SUPPORTED` (0xC00000BB) — request is not supported.
    pub const NOT_SUPPORTED: Self = Self(0xC000_00BB_u32 as i32);

    /// Creates an [`NtStatus`] from a raw `i32` NTSTATUS value.
    #[must_use]
    #[inline]
    pub const fn from_raw(status: i32) -> Self {
        Self(status)
    }

    /// Returns the underlying raw `i32` NTSTATUS value.
    #[must_use]
    #[inline]
    pub const fn into_raw(self) -> i32 {
        self.0
    }

    /// Returns `true` if this status represents a successful outcome.
    ///
    /// Mirrors the kernel `NT_SUCCESS(Status)` C macro.
    #[must_use]
    #[inline]
    pub const fn is_success(self) -> bool {
        self.0 >= 0
    }

    /// Returns `true` if this status is an error (severity bits = `11`).
    #[must_use]
    #[inline]
    pub const fn is_error(self) -> bool {
        (self.0 as u32) >= 0xC000_0000
    }
}

impl core::fmt::Debug for NtStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "NtStatus({:#010X})", self.0 as u32)
    }
}

impl core::fmt::Display for NtStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#010X}", self.0 as u32)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
// These run on the host with `cargo test -p wdk-safe` — no WDK needed.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_is_success() {
        assert!(NtStatus::SUCCESS.is_success());
    }

    #[test]
    fn success_is_not_error() {
        assert!(!NtStatus::SUCCESS.is_error());
    }

    #[test]
    fn unsuccessful_is_not_success() {
        assert!(!NtStatus::UNSUCCESSFUL.is_success());
    }

    #[test]
    fn unsuccessful_is_error() {
        assert!(NtStatus::UNSUCCESSFUL.is_error());
    }

    #[test]
    fn not_supported_is_error() {
        assert!(NtStatus::NOT_SUPPORTED.is_error());
    }

    #[test]
    fn invalid_parameter_is_error() {
        assert!(NtStatus::INVALID_PARAMETER.is_error());
    }

    #[test]
    fn raw_roundtrip() {
        assert_eq!(NtStatus::from_raw(0).into_raw(), 0);
    }

    #[test]
    fn debug_format_is_hex() {
        let s = format!("{:?}", NtStatus::SUCCESS);
        assert!(s.contains("0x00000000"), "got: {s}");
    }
}