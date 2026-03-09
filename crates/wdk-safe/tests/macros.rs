// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests for the `define_ioctl!` proc-macro.
//!
//! These live in `wdk-safe` (not `wdk-safe-macros`) because proc-macro
//! crates cannot depend on their companion library crate.

#![allow(missing_docs)]

use wdk_safe::{
    define_ioctl,
    ioctl::{RequiredAccess, TransferMethod},
};

#[repr(C)]
pub struct Req {
    pub value: u32,
}
#[repr(C)]
pub struct Rsp {
    pub value: u32,
}

// ── Minimal syntax (default method + access)
// ──────────────────────────────────

define_ioctl!(IOCTL_ECHO, 0x8000u16, 0x800u16, Req => Rsp);

#[test]
fn ioctl_echo_raw_value() {
    // (0x8000 << 16) | (0 << 14) | (0x800 << 2) | 0 = 0x8000_2000
    assert_eq!(IOCTL_ECHO.into_raw(), 0x8000_2000);
}

#[test]
fn type_aliases_exist() {
    let _: IoctlEchoInput = Req { value: 1 };
    let _: IoctlEchoOutput = Rsp { value: 2 };
}

#[test]
fn default_method_is_buffered() {
    assert_eq!(IOCTL_ECHO.method(), TransferMethod::Buffered);
}

#[test]
fn default_access_is_any() {
    assert_eq!(IOCTL_ECHO.access(), RequiredAccess::Any);
}

// ── Explicit method = InDirect, access = Read
// ─────────────────────────────────

define_ioctl!(
    IOCTL_READ_DATA,
    0x8000u16,
    0x801u16,
    Req => Rsp,
    method = InDirect,
    access = Read
);

#[test]
fn ioctl_read_data_method_is_in_direct() {
    assert_eq!(IOCTL_READ_DATA.method(), TransferMethod::InDirect);
}

#[test]
fn ioctl_read_data_access_is_read() {
    assert_eq!(IOCTL_READ_DATA.access(), RequiredAccess::Read);
}

#[test]
fn ioctl_read_data_function_code() {
    assert_eq!(IOCTL_READ_DATA.function(), 0x801);
}

#[test]
fn ioctl_read_data_type_aliases_exist() {
    let _: IoctlReadDataInput = Req { value: 0 };
    let _: IoctlReadDataOutput = Rsp { value: 0 };
}

// ── Explicit method = OutDirect, access = Write
// ───────────────────────────────

define_ioctl!(
    IOCTL_WRITE_DATA,
    0x8000u16,
    0x802u16,
    Req => Rsp,
    method = OutDirect,
    access = Write
);

#[test]
fn ioctl_write_data_method_is_out_direct() {
    assert_eq!(IOCTL_WRITE_DATA.method(), TransferMethod::OutDirect);
}

#[test]
fn ioctl_write_data_access_is_write() {
    assert_eq!(IOCTL_WRITE_DATA.access(), RequiredAccess::Write);
}

// ── Explicit method = Neither, access = ReadWrite
// ─────────────────────────────

define_ioctl!(
    IOCTL_NEITHER_RW,
    0x8000u16,
    0x803u16,
    Req => Rsp,
    method = Neither,
    access = ReadWrite
);

#[test]
fn ioctl_neither_rw_method_is_neither() {
    assert_eq!(IOCTL_NEITHER_RW.method(), TransferMethod::Neither);
}

#[test]
fn ioctl_neither_rw_access_is_read_write() {
    assert_eq!(IOCTL_NEITHER_RW.access(), RequiredAccess::ReadWrite);
}

// ── PascalCase name derivation
// ────────────────────────────────────────────────

define_ioctl!(IOCTL_MY_MULTI_WORD, 0x8000u16, 0x810u16, Req => Rsp);

#[test]
fn multi_word_pascal_case_aliases() {
    // IOCTL_MY_MULTI_WORD → IoctlMyMultiWordInput / IoctlMyMultiWordOutput
    let _: IoctlMyMultiWordInput = Req { value: 0 };
    let _: IoctlMyMultiWordOutput = Rsp { value: 0 };
}

// ── Constants are distinct
// ─────────────────────────────────────────────────────

#[test]
fn different_function_codes_produce_different_raw_values() {
    assert_ne!(IOCTL_ECHO.into_raw(), IOCTL_READ_DATA.into_raw());
    assert_ne!(IOCTL_READ_DATA.into_raw(), IOCTL_WRITE_DATA.into_raw());
}

#[test]
fn device_type_preserved() {
    assert_eq!(IOCTL_ECHO.device_type(), 0x8000);
    assert_eq!(IOCTL_READ_DATA.device_type(), 0x8000);
}
