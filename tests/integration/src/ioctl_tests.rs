// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests for the `define_ioctl!` proc-macro.
//!
//! These live outside `wdk-safe-macros` because proc-macro crates cannot
//! depend on their companion library crate.

#![allow(missing_docs)]

use wdk_safe::{
    define_ioctl,
    ioctl::{IoControlCode, RequiredAccess, TransferMethod},
};

// ── Shared buffer types ───────────────────────────────────────────────────────

#[repr(C)]
pub struct Req {
    pub value: u32,
}

#[repr(C)]
pub struct Rsp {
    pub value: u32,
}

#[repr(C)]
pub struct EmptyReq;

#[repr(C)]
pub struct EmptyRsp;

// ── Minimal syntax (defaults) ─────────────────────────────────────────────────

define_ioctl!(IOCTL_ECHO, 0x8000u16, 0x800u16, Req => Rsp);

#[test]
fn echo_raw_value() {
    // (0x8000 << 16) | (0 << 14) | (0x800 << 2) | 0 = 0x8000_2000
    assert_eq!(IOCTL_ECHO.into_raw(), 0x8000_2000);
}

#[test]
fn echo_type_aliases_exist() {
    let _: IoctlEchoInput = Req { value: 1 };
    let _: IoctlEchoOutput = Rsp { value: 2 };
}

#[test]
fn echo_default_method_is_buffered() {
    assert_eq!(IOCTL_ECHO.method(), TransferMethod::Buffered);
}

#[test]
fn echo_default_access_is_any() {
    assert_eq!(IOCTL_ECHO.access(), RequiredAccess::Any);
}

#[test]
fn echo_device_type() {
    assert_eq!(IOCTL_ECHO.device_type(), 0x8000);
}

#[test]
fn echo_function_code() {
    assert_eq!(IOCTL_ECHO.function(), 0x800);
}

// ── Explicit method = InDirect, access = Read ─────────────────────────────────

define_ioctl!(
    IOCTL_READ_DATA,
    0x8000u16,
    0x801u16,
    Req => Rsp,
    method = InDirect,
    access = Read
);

#[test]
fn read_data_method_is_in_direct() {
    assert_eq!(IOCTL_READ_DATA.method(), TransferMethod::InDirect);
}

#[test]
fn read_data_access_is_read() {
    assert_eq!(IOCTL_READ_DATA.access(), RequiredAccess::Read);
}

#[test]
fn read_data_function_code() {
    assert_eq!(IOCTL_READ_DATA.function(), 0x801);
}

#[test]
fn read_data_type_aliases() {
    let _: IoctlReadDataInput = Req { value: 0 };
    let _: IoctlReadDataOutput = Rsp { value: 0 };
}

// ── method = OutDirect, access = Write ───────────────────────────────────────

define_ioctl!(
    IOCTL_WRITE_DATA,
    0x8000u16,
    0x802u16,
    Req => Rsp,
    method = OutDirect,
    access = Write
);

#[test]
fn write_data_method_is_out_direct() {
    assert_eq!(IOCTL_WRITE_DATA.method(), TransferMethod::OutDirect);
}

#[test]
fn write_data_access_is_write() {
    assert_eq!(IOCTL_WRITE_DATA.access(), RequiredAccess::Write);
}

// ── method = Neither, access = ReadWrite ──────────────────────────────────────

define_ioctl!(
    IOCTL_NEITHER_RW,
    0x8000u16,
    0x803u16,
    Req => Rsp,
    method = Neither,
    access = ReadWrite
);

#[test]
fn neither_rw_method_is_neither() {
    assert_eq!(IOCTL_NEITHER_RW.method(), TransferMethod::Neither);
}

#[test]
fn neither_rw_access_is_read_write() {
    assert_eq!(IOCTL_NEITHER_RW.access(), RequiredAccess::ReadWrite);
}

// ── Explicit Buffered + Any (same as default) ─────────────────────────────────

define_ioctl!(
    IOCTL_EXPLICIT_DEFAULTS,
    0x8000u16,
    0x804u16,
    Req => Rsp,
    method = Buffered,
    access = Any
);

#[test]
fn explicit_defaults_match_minimal_defaults() {
    // Both should produce METHOD_BUFFERED | FILE_ANY_ACCESS
    let explicit_raw = IOCTL_EXPLICIT_DEFAULTS.into_raw();
    let implicit_raw = IoControlCode::new(
        0x8000,
        0x804,
        TransferMethod::Buffered,
        RequiredAccess::Any,
    )
    .into_raw();
    assert_eq!(explicit_raw, implicit_raw);
}

// ── Trailing comma allowed ────────────────────────────────────────────────────

define_ioctl!(
    IOCTL_TRAILING_COMMA,
    0x8000u16,
    0x805u16,
    Req => Rsp,
    method = Buffered,
    access = Any,   // trailing comma
);

#[test]
fn trailing_comma_compiles() {
    assert_eq!(IOCTL_TRAILING_COMMA.device_type(), 0x8000);
}

// ── PascalCase derivation ─────────────────────────────────────────────────────

define_ioctl!(IOCTL_MY_MULTI_WORD, 0x8000u16, 0x810u16, Req => Rsp);

#[test]
fn multi_word_pascal_case() {
    // IOCTL_MY_MULTI_WORD → IoctlMyMultiWordInput / IoctlMyMultiWordOutput
    let _: IoctlMyMultiWordInput = Req { value: 0 };
    let _: IoctlMyMultiWordOutput = Rsp { value: 0 };
}

define_ioctl!(IOCTL_A, 0x8000u16, 0x820u16, Req => Rsp);

#[test]
fn single_word_after_ioctl_prefix() {
    let _: IoctlAInput = Req { value: 0 };
    let _: IoctlAOutput = Rsp { value: 0 };
}

// ── Unit struct input/output types ───────────────────────────────────────────

define_ioctl!(IOCTL_EMPTY, 0x8000u16, 0x830u16, EmptyReq => EmptyRsp);

#[test]
fn unit_struct_types_compile() {
    let _: IoctlEmptyInput = EmptyReq;
    let _: IoctlEmptyOutput = EmptyRsp;
}

// ── Different device types ────────────────────────────────────────────────────

define_ioctl!(IOCTL_OTHER_DEVICE, 0x0022u16, 0x800u16, Req => Rsp);

#[test]
fn other_device_type_preserved() {
    assert_eq!(IOCTL_OTHER_DEVICE.device_type(), 0x0022);
}

#[test]
fn other_device_is_microsoft_range() {
    assert!(IOCTL_OTHER_DEVICE.is_microsoft_device_type());
}

// ── Uniqueness ────────────────────────────────────────────────────────────────

#[test]
fn different_function_codes_are_unique() {
    assert_ne!(IOCTL_ECHO.into_raw(), IOCTL_READ_DATA.into_raw());
    assert_ne!(IOCTL_READ_DATA.into_raw(), IOCTL_WRITE_DATA.into_raw());
    assert_ne!(IOCTL_WRITE_DATA.into_raw(), IOCTL_NEITHER_RW.into_raw());
}

#[test]
fn different_device_types_are_unique() {
    assert_ne!(IOCTL_ECHO.into_raw(), IOCTL_OTHER_DEVICE.into_raw());
}

// ── IoControlCode is a const ──────────────────────────────────────────────────

#[test]
fn ioctl_constants_are_copy() {
    let a = IOCTL_ECHO;
    let b = a; // Copy
    assert_eq!(a, b);
}

#[test]
fn ioctl_constants_implement_eq() {
    assert_eq!(IOCTL_ECHO, IOCTL_ECHO);
    assert_ne!(IOCTL_ECHO, IOCTL_READ_DATA);
}