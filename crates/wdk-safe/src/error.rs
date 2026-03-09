// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`NtStatus`] — an idiomatic Rust newtype over Windows `NTSTATUS` codes.

/// A strongly-typed wrapper around a Windows `NTSTATUS` code.
///
/// `NTSTATUS` is a 32-bit integer. Values >= 0 indicate success
/// (mirrors the `NT_SUCCESS` C macro). Values with bits 31-30 == `11`
/// are errors.
///
/// # Severity bits (31–30)
///
/// | Bits | Meaning   |
/// |------|-----------|
/// | `00` | Success   |
/// | `01` | Informational |
/// | `10` | Warning   |
/// | `11` | Error     |
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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct NtStatus(i32);

impl NtStatus {
    /// `STATUS_SUCCESS` (0x00000000) — operation completed successfully.
    pub const SUCCESS: Self = Self(0x0000_0000_i32);

    /// `STATUS_PENDING` (0x00000103) — operation is pending.
    pub const PENDING: Self = Self(0x0000_0103_i32);

    /// `STATUS_UNSUCCESSFUL` (0xC0000001) — operation was unsuccessful.
    #[allow(clippy::cast_possible_wrap)]
    pub const UNSUCCESSFUL: Self = Self(0xC000_0001_u32 as i32);

    /// `STATUS_INVALID_PARAMETER` (0xC000000D) — invalid parameter passed.
    #[allow(clippy::cast_possible_wrap)]
    pub const INVALID_PARAMETER: Self = Self(0xC000_000D_u32 as i32);

    /// `STATUS_NOT_SUPPORTED` (0xC00000BB) — request is not supported.
    #[allow(clippy::cast_possible_wrap)]
    pub const NOT_SUPPORTED: Self = Self(0xC000_00BB_u32 as i32);

    /// `STATUS_BUFFER_TOO_SMALL` (0xC0000023) — buffer too small for result.
    #[allow(clippy::cast_possible_wrap)]
    pub const BUFFER_TOO_SMALL: Self = Self(0xC000_0023_u32 as i32);

    /// `STATUS_ACCESS_DENIED` (0xC0000022) — access denied.
    #[allow(clippy::cast_possible_wrap)]
    pub const ACCESS_DENIED: Self = Self(0xC000_0022_u32 as i32);

    /// `STATUS_INSUFFICIENT_RESOURCES` (0xC000009A) — insufficient resources.
    #[allow(clippy::cast_possible_wrap)]
    pub const INSUFFICIENT_RESOURCES: Self = Self(0xC000_009A_u32 as i32);

    /// `STATUS_DEVICE_NOT_READY` (0xC00000A3) — device not ready.
    #[allow(clippy::cast_possible_wrap)]
    pub const DEVICE_NOT_READY: Self = Self(0xC000_00A3_u32 as i32);

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
    /// Mirrors the kernel `NT_SUCCESS(Status)` C macro: severity bits == `00`.
    #[must_use]
    #[inline]
    pub const fn is_success(self) -> bool {
        self.0 >= 0
    }

    /// Returns `true` if this is an informational status (severity bits = `01`).
    #[must_use]
    #[inline]
    #[allow(clippy::cast_sign_loss)]
    pub const fn is_informational(self) -> bool {
        (self.0 as u32) >> 30 == 0b01
    }

    /// Returns `true` if this is a warning status (severity bits = `10`).
    #[must_use]
    #[inline]
    #[allow(clippy::cast_sign_loss)]
    pub const fn is_warning(self) -> bool {
        (self.0 as u32) >> 30 == 0b10
    }

    /// Returns `true` if this status is an error (severity bits = `11`).
    #[must_use]
    #[inline]
    #[allow(clippy::cast_sign_loss)]
    pub const fn is_error(self) -> bool {
        (self.0 as u32) >= 0xC000_0000
    }
}

impl core::fmt::Debug for NtStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // SAFETY: intentional bit-pattern reinterpret — NTSTATUS is logically u32.
        #[allow(clippy::cast_sign_loss)]
        let raw = self.0 as u32;
        write!(f, "NtStatus({raw:#010X})")
    }
}

impl core::fmt::Display for NtStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        #[allow(clippy::cast_sign_loss)]
        let raw = self.0 as u32;
        write!(f, "{raw:#010X}")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
// These run on the host with `cargo test -p wdk-safe` — no WDK needed.

#[cfg(test)]
mod tests {
    use super::*;

    // ── Success ───────────────────────────────────────────────────────────────

    #[test]
    fn success_is_success() {
        assert!(NtStatus::SUCCESS.is_success());
    }

    #[test]
    fn success_is_not_error() {
        assert!(!NtStatus::SUCCESS.is_error());
    }

    #[test]
    fn success_is_not_warning() {
        assert!(!NtStatus::SUCCESS.is_warning());
    }

    #[test]
    fn success_is_not_informational() {
        assert!(!NtStatus::SUCCESS.is_informational());
    }

    // ── Pending (success severity) ────────────────────────────────────────────

    #[test]
    fn pending_is_success() {
        // STATUS_PENDING has severity 00 — NT_SUCCESS returns true.
        assert!(NtStatus::PENDING.is_success());
    }

    #[test]
    fn pending_is_not_error() {
        assert!(!NtStatus::PENDING.is_error());
    }

    // ── Warning severity ──────────────────────────────────────────────────────

    #[test]
    fn warning_severity_detected() {
        // STATUS_BUFFER_OVERFLOW = 0x80000005 — severity bits 10.
        #[allow(clippy::cast_possible_wrap)]
        let warning = NtStatus::from_raw(0x8000_0005_u32 as i32);
        assert!(warning.is_warning());
        assert!(!warning.is_error());
        assert!(!warning.is_success());
    }

    // ── Informational severity ────────────────────────────────────────────────

    #[test]
    fn informational_severity_detected() {
        // 0x40000000 — severity bits 01.
        #[allow(clippy::cast_possible_wrap)]
        let info = NtStatus::from_raw(0x4000_0000_u32 as i32);
        assert!(info.is_informational());
        assert!(!info.is_error());
        assert!(!info.is_warning());
    }

    // ── Error constants ───────────────────────────────────────────────────────

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
    fn buffer_too_small_is_error() {
        assert!(NtStatus::BUFFER_TOO_SMALL.is_error());
    }

    #[test]
    fn access_denied_is_error() {
        assert!(NtStatus::ACCESS_DENIED.is_error());
    }

    #[test]
    fn insufficient_resources_is_error() {
        assert!(NtStatus::INSUFFICIENT_RESOURCES.is_error());
    }

    #[test]
    fn device_not_ready_is_error() {
        assert!(NtStatus::DEVICE_NOT_READY.is_error());
    }

    // ── Roundtrips ────────────────────────────────────────────────────────────

    #[test]
    fn raw_roundtrip() {
        assert_eq!(NtStatus::from_raw(0).into_raw(), 0);
    }

    #[test]
    fn raw_roundtrip_error() {
        #[allow(clippy::cast_possible_wrap)]
        let raw = 0xC000_0001_u32 as i32;
        assert_eq!(NtStatus::from_raw(raw).into_raw(), raw);
    }

    // ── Formatting ────────────────────────────────────────────────────────────

    #[test]
    fn debug_format_success() {
        let s = format!("{:?}", NtStatus::SUCCESS);
        assert!(s.contains("0x00000000"), "got: {s}");
    }

    #[test]
    fn debug_format_error() {
        let s = format!("{:?}", NtStatus::UNSUCCESSFUL);
        assert!(s.contains("0xC0000001"), "got: {s}");
    }

    #[test]
    fn display_format() {
        let s = format!("{}", NtStatus::SUCCESS);
        assert_eq!(s, "0x00000000");
    }

    // ── Equality & Hash ───────────────────────────────────────────────────────

    #[test]
    fn equality_same_value() {
        assert_eq!(NtStatus::SUCCESS, NtStatus::from_raw(0));
    }

    #[test]
    fn inequality_different_values() {
        assert_ne!(NtStatus::SUCCESS, NtStatus::UNSUCCESSFUL);
    }

    #[test]
    fn copy_semantics() {
        let a = NtStatus::SUCCESS;
        let b = a; // Copy
        assert_eq!(a, b);
    }
}