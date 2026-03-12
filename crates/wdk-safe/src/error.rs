// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`NtStatus`] — an idiomatic Rust newtype over Windows `NTSTATUS` codes.
//!
//! # NTSTATUS bit layout
//!
//! A `NTSTATUS` is a 32-bit integer packed as:
//!
//! ```text
//! Bits 31–30  Severity  (00=Success, 01=Informational, 10=Warning, 11=Error)
//! Bit  29     Customer  (0=Microsoft, 1=application-defined)
//! Bit  28     Reserved
//! Bits 27–16  Facility
//! Bits 15– 0  Code
//! ```
//!
//! The `NT_SUCCESS(s)` C macro is simply `(NTSTATUS)(s) >= 0` — i.e. the
//! severity bits are `00`. [`NtStatus::is_success`] mirrors this exactly.

/// A strongly-typed wrapper around a Windows `NTSTATUS` code.
///
/// # Severity bits (31–30)
///
/// | Bits | Meaning       | Predicate                       |
/// |------|---------------|---------------------------------|
/// | `00` | Success       | [`is_success`](Self::is_success) |
/// | `01` | Informational | [`is_informational`](Self::is_informational) |
/// | `10` | Warning       | [`is_warning`](Self::is_warning) |
/// | `11` | Error         | [`is_error`](Self::is_error)    |
///
/// # Examples
///
/// ```rust
/// use wdk_safe::NtStatus;
///
/// assert!(NtStatus::SUCCESS.is_success());
/// assert!(NtStatus::UNSUCCESSFUL.is_error());
/// assert!(!NtStatus::SUCCESS.is_error());
/// assert_eq!(NtStatus::SUCCESS.into_raw(), 0);
///
/// // Round-trip through raw value
/// let raw = 0xC000_0005_u32 as i32;
/// assert_eq!(NtStatus::from_raw(raw).into_raw(), raw);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct NtStatus(i32);

impl NtStatus {
    // ── Success codes ─────────────────────────────────────────────────────────

    /// `STATUS_SUCCESS` (`0x0000_0000`) — operation completed successfully.
    pub const SUCCESS: Self = Self(0x0000_0000_i32);

    /// `STATUS_PENDING` (`0x0000_0103`) — operation is pending completion.
    ///
    /// Note: `NT_SUCCESS(STATUS_PENDING)` is `true` — severity bits are `00`.
    pub const PENDING: Self = Self(0x0000_0103_i32);

    // ── Warning codes ─────────────────────────────────────────────────────────

    /// `STATUS_BUFFER_OVERFLOW` (`0x8000_0005`) — data was truncated because
    /// the output buffer was too small.
    ///
    /// This is a **warning** (`NT_SUCCESS` returns `false`), not an error.
    /// Distinguish from [`BUFFER_TOO_SMALL`](Self::BUFFER_TOO_SMALL) which is
    /// an error.
    #[allow(clippy::cast_possible_wrap)]
    pub const BUFFER_OVERFLOW: Self = Self(0x8000_0005_u32 as i32);

    // ── Error codes ───────────────────────────────────────────────────────────

    /// `STATUS_UNSUCCESSFUL` (`0xC000_0001`) — generic failure.
    #[allow(clippy::cast_possible_wrap)]
    pub const UNSUCCESSFUL: Self = Self(0xC000_0001_u32 as i32);

    /// `STATUS_NOT_IMPLEMENTED` (`0xC000_0002`) — function not implemented.
    #[allow(clippy::cast_possible_wrap)]
    pub const NOT_IMPLEMENTED: Self = Self(0xC000_0002_u32 as i32);

    /// `STATUS_INVALID_PARAMETER` (`0xC000_000D`) — invalid parameter passed.
    #[allow(clippy::cast_possible_wrap)]
    pub const INVALID_PARAMETER: Self = Self(0xC000_000D_u32 as i32);

    /// `STATUS_ACCESS_DENIED` (`0xC000_0022`) — access denied.
    #[allow(clippy::cast_possible_wrap)]
    pub const ACCESS_DENIED: Self = Self(0xC000_0022_u32 as i32);

    /// `STATUS_BUFFER_TOO_SMALL` (`0xC000_0023`) — provided buffer is too
    /// small to contain the required data.
    ///
    /// This is an **error** (unlike [`BUFFER_OVERFLOW`](Self::BUFFER_OVERFLOW)
    /// which is a warning and means data was partially returned).
    #[allow(clippy::cast_possible_wrap)]
    pub const BUFFER_TOO_SMALL: Self = Self(0xC000_0023_u32 as i32);

    /// `STATUS_OBJECT_NAME_NOT_FOUND` (`0xC000_0034`) — object not found.
    #[allow(clippy::cast_possible_wrap)]
    pub const OBJECT_NAME_NOT_FOUND: Self = Self(0xC000_0034_u32 as i32);

    /// `STATUS_INSUFFICIENT_RESOURCES` (`0xC000_009A`) — not enough kernel
    /// memory to complete the operation.
    #[allow(clippy::cast_possible_wrap)]
    pub const INSUFFICIENT_RESOURCES: Self = Self(0xC000_009A_u32 as i32);

    /// `STATUS_DEVICE_NOT_READY` (`0xC000_00A3`) — device is not ready.
    #[allow(clippy::cast_possible_wrap)]
    pub const DEVICE_NOT_READY: Self = Self(0xC000_00A3_u32 as i32);

    /// `STATUS_NOT_SUPPORTED` (`0xC000_00BB`) — request is not supported by
    /// the driver.
    ///
    /// This is the correct return value for IRP major functions a driver
    /// deliberately does not handle.
    #[allow(clippy::cast_possible_wrap)]
    pub const NOT_SUPPORTED: Self = Self(0xC000_00BB_u32 as i32);

    /// `STATUS_INVALID_DEVICE_REQUEST` (`0xC000_0010`) — invalid request sent
    /// to the device.
    ///
    /// Use this instead of `NOT_SUPPORTED` when the device exists but the
    /// specific request makes no sense for it (e.g. `IRP_MJ_READ` on a
    /// non-readable device type).
    #[allow(clippy::cast_possible_wrap)]
    pub const INVALID_DEVICE_REQUEST: Self = Self(0xC000_0010_u32 as i32);

    /// `STATUS_DELETE_PENDING` (`0xC000_0056`) — device is being deleted.
    #[allow(clippy::cast_possible_wrap)]
    pub const DELETE_PENDING: Self = Self(0xC000_0056_u32 as i32);

    /// `STATUS_NO_MEMORY` (`0xC000_0017`) — insufficient virtual memory.
    #[allow(clippy::cast_possible_wrap)]
    pub const NO_MEMORY: Self = Self(0xC000_0017_u32 as i32);

    // ── Constructors ──────────────────────────────────────────────────────────

    /// Creates an [`NtStatus`] from a raw `i32` NTSTATUS value.
    ///
    /// Use this when converting values received from WDK functions.
    #[must_use]
    #[inline]
    pub const fn from_raw(status: i32) -> Self {
        Self(status)
    }

    /// Returns the underlying raw `i32` NTSTATUS value.
    ///
    /// Use this when passing a status back to the kernel (e.g. as the return
    /// value of a dispatch routine).
    #[must_use]
    #[inline]
    pub const fn into_raw(self) -> i32 {
        self.0
    }

    // ── Severity predicates ───────────────────────────────────────────────────

    /// Returns `true` if this status represents a successful outcome.
    ///
    /// Mirrors the kernel `NT_SUCCESS(Status)` C macro: severity bits `== 00`,
    /// i.e. the signed value is `>= 0`. Note that `STATUS_PENDING` is a
    /// success value.
    #[must_use]
    #[inline]
    pub const fn is_success(self) -> bool {
        self.0 >= 0
    }

    /// Returns `true` if this is an informational status (severity bits `01`).
    #[must_use]
    #[inline]
    #[allow(clippy::cast_sign_loss)]
    pub const fn is_informational(self) -> bool {
        (self.0 as u32) >> 30 == 0b01
    }

    /// Returns `true` if this is a warning status (severity bits `10`).
    ///
    /// `NT_SUCCESS` is `false` for warnings.
    #[must_use]
    #[inline]
    #[allow(clippy::cast_sign_loss)]
    pub const fn is_warning(self) -> bool {
        (self.0 as u32) >> 30 == 0b10
    }

    /// Returns `true` if this status is an error (severity bits `11`).
    ///
    /// Mirrors the kernel `NT_ERROR(Status)` C macro.
    #[must_use]
    #[inline]
    #[allow(clippy::cast_sign_loss)]
    pub const fn is_error(self) -> bool {
        (self.0 as u32) >= 0xC000_0000
    }

    /// Returns the severity field (bits 31–30) as a [`Severity`] enum.
    #[must_use]
    #[inline]
    #[allow(clippy::cast_sign_loss)]
    pub const fn severity(self) -> Severity {
        match (self.0 as u32) >> 30 {
            0b00 => Severity::Success,
            0b01 => Severity::Informational,
            0b10 => Severity::Warning,
            _ => Severity::Error,
        }
    }
}

/// The severity class of an [`NtStatus`] value.
///
/// Derived from bits 31–30 of the NTSTATUS code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// Severity bits `00` — operation succeeded. `NT_SUCCESS` is `true`.
    Success,
    /// Severity bits `01` — informational message.
    Informational,
    /// Severity bits `10` — warning; partial success. `NT_SUCCESS` is `false`.
    Warning,
    /// Severity bits `11` — operation failed. `NT_SUCCESS` is `false`.
    Error,
}

impl core::fmt::Debug for NtStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        #[allow(clippy::cast_sign_loss)]
        let raw = self.0 as u32;
        write!(f, "NtStatus({raw:#010X} / {sev:?})", sev = self.severity())
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

    #[test]
    fn success_severity_is_success() {
        assert_eq!(NtStatus::SUCCESS.severity(), Severity::Success);
    }

    // ── STATUS_PENDING ────────────────────────────────────────────────────────

    #[test]
    fn pending_is_success() {
        // STATUS_PENDING has severity 00 — NT_SUCCESS returns true.
        assert!(NtStatus::PENDING.is_success());
    }

    #[test]
    fn pending_is_not_error() {
        assert!(!NtStatus::PENDING.is_error());
    }

    #[test]
    fn pending_severity_is_success() {
        assert_eq!(NtStatus::PENDING.severity(), Severity::Success);
    }

    // ── Warning ───────────────────────────────────────────────────────────────

    #[test]
    fn buffer_overflow_is_warning() {
        assert!(NtStatus::BUFFER_OVERFLOW.is_warning());
        assert!(!NtStatus::BUFFER_OVERFLOW.is_success());
        assert!(!NtStatus::BUFFER_OVERFLOW.is_error());
    }

    #[test]
    fn warning_severity_detected_by_raw() {
        // STATUS_BUFFER_OVERFLOW = 0x80000005 — severity bits 10.
        #[allow(clippy::cast_possible_wrap)]
        let warning = NtStatus::from_raw(0x8000_0005_u32 as i32);
        assert!(warning.is_warning());
        assert!(!warning.is_error());
        assert!(!warning.is_success());
        assert_eq!(warning.severity(), Severity::Warning);
    }

    // ── Informational ─────────────────────────────────────────────────────────

    #[test]
    fn informational_severity_detected() {
        // 0x40000000 — severity bits 01 (Informational).
        // NOTE: NT_SUCCESS(0x40000000) is TRUE — severity 01 is >= 0.
        // is_success() intentionally mirrors NT_SUCCESS exactly.
        #[allow(clippy::cast_possible_wrap)]
        let info = NtStatus::from_raw(0x4000_0000_u32 as i32);
        assert!(info.is_informational());
        assert!(!info.is_error());
        assert!(!info.is_warning());
        assert!(info.is_success()); // NT_SUCCESS is true for informational codes
        assert_eq!(info.severity(), Severity::Informational);
    }

    // ── Error constants ───────────────────────────────────────────────────────

    #[test]
    fn unsuccessful_is_error() {
        assert!(NtStatus::UNSUCCESSFUL.is_error());
        assert!(!NtStatus::UNSUCCESSFUL.is_success());
    }

    #[test]
    fn not_supported_is_error() {
        assert!(NtStatus::NOT_SUPPORTED.is_error());
    }

    #[test]
    fn not_implemented_is_error() {
        assert!(NtStatus::NOT_IMPLEMENTED.is_error());
    }

    #[test]
    fn invalid_parameter_is_error() {
        assert!(NtStatus::INVALID_PARAMETER.is_error());
    }

    #[test]
    fn buffer_too_small_is_error() {
        assert!(NtStatus::BUFFER_TOO_SMALL.is_error());
        // Distinct from BUFFER_OVERFLOW (warning)
        assert_ne!(NtStatus::BUFFER_TOO_SMALL, NtStatus::BUFFER_OVERFLOW);
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

    #[test]
    fn invalid_device_request_is_error() {
        assert!(NtStatus::INVALID_DEVICE_REQUEST.is_error());
    }

    #[test]
    fn delete_pending_is_error() {
        assert!(NtStatus::DELETE_PENDING.is_error());
    }

    #[test]
    fn no_memory_is_error() {
        assert!(NtStatus::NO_MEMORY.is_error());
    }

    #[test]
    fn object_name_not_found_is_error() {
        assert!(NtStatus::OBJECT_NAME_NOT_FOUND.is_error());
    }

    // ── Error severity variant ────────────────────────────────────────────────

    #[test]
    fn error_codes_have_error_severity() {
        for status in [
            NtStatus::UNSUCCESSFUL,
            NtStatus::NOT_SUPPORTED,
            NtStatus::INVALID_PARAMETER,
            NtStatus::ACCESS_DENIED,
        ] {
            assert_eq!(status.severity(), Severity::Error, "for {status:?}");
        }
    }

    // ── Raw roundtrips ────────────────────────────────────────────────────────

    #[test]
    fn raw_roundtrip_success() {
        assert_eq!(NtStatus::from_raw(0).into_raw(), 0);
    }

    #[test]
    fn raw_roundtrip_error() {
        #[allow(clippy::cast_possible_wrap)]
        let raw = 0xC000_0001_u32 as i32;
        assert_eq!(NtStatus::from_raw(raw).into_raw(), raw);
    }

    #[test]
    fn raw_roundtrip_arbitrary() {
        for raw_u32 in [0u32, 0x4000_0000, 0x8000_0005, 0xC000_0001, 0xFFFF_FFFF] {
            #[allow(clippy::cast_possible_wrap)]
            let raw = raw_u32 as i32;
            assert_eq!(NtStatus::from_raw(raw).into_raw(), raw);
        }
    }

    // ── Formatting ────────────────────────────────────────────────────────────

    #[test]
    fn debug_format_success() {
        let s = format!("{:?}", NtStatus::SUCCESS);
        assert!(s.contains("0x00000000"), "got: {s}");
        assert!(s.contains("Success"), "got: {s}");
    }

    #[test]
    fn debug_format_error() {
        let s = format!("{:?}", NtStatus::UNSUCCESSFUL);
        assert!(s.contains("0xC0000001"), "got: {s}");
        assert!(s.contains("Error"), "got: {s}");
    }

    #[test]
    fn display_format_is_hex() {
        assert_eq!(format!("{}", NtStatus::SUCCESS), "0x00000000");
        #[allow(clippy::cast_possible_wrap)]
        let err = NtStatus::from_raw(0xC000_0001_u32 as i32);
        assert_eq!(format!("{err}"), "0xC0000001");
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
        let b = a; // Copy — must not move
        assert_eq!(a, b);
    }

    #[test]
    fn hash_consistent_with_eq() {
        use core::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        fn hash(s: NtStatus) -> u64 {
            let mut h = DefaultHasher::new();
            s.hash(&mut h);
            h.finish()
        }

        // Equal values must have equal hashes
        assert_eq!(hash(NtStatus::SUCCESS), hash(NtStatus::from_raw(0)));
        // Unequal values should (almost certainly) differ
        assert_ne!(hash(NtStatus::SUCCESS), hash(NtStatus::UNSUCCESSFUL));
    }
}