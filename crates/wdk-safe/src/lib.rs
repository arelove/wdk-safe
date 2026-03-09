// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # wdk-safe
//!
//! Safe, idiomatic Rust abstractions for Windows kernel-mode driver
//! development, built on top of [`wdk-sys`](https://crates.io/crates/wdk-sys).
//!
//! ## Design goals
//!
//! | Problem in raw `wdk-sys`                              | Solution in `wdk-safe`                                  |
//! |-------------------------------------------------------|---------------------------------------------------------|
//! | `IRP` must be completed exactly once                  | [`Irp`] consumes itself on `complete` — double-complete is a compile error |
//! | IOCTL buffer types are untyped `*mut u8`              | [`define_ioctl!`] declares input/output types at compile time |
//! | Every kernel struct access requires `unsafe`          | Safe wrappers around `DEVICE_OBJECT`, `IO_STACK_LOCATION` |
//! | `STATUS_*` constants are bare `i32`                   | [`NtStatus`] newtype with `is_success()`, `is_error()`, `Debug` |
//!
//! ## `no_std` / host testing
//!
//! This crate has **zero dependency** on `wdk-sys`. Kernel types are
//! represented as opaque `*mut c_void` pointers. Driver crates (which do
//! link `wdk-sys`) cast their real pointers with `.cast()`.
//!
//! Unit tests run on any Windows host — no WDK installation required:
//!
//! ```powershell
//! cargo test -p wdk-safe -p wdk-safe-macros
//! ```
//!
//! [`Irp`]: irp::Irp
//! [`define_ioctl!`]: wdk_safe_macros::define_ioctl

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

pub mod device;
pub mod driver;
pub mod error;
pub mod ioctl;
pub mod irp;
pub mod request;

pub use device::Device;
pub use driver::KmdfDriver;
pub use error::NtStatus;
pub use ioctl::IoControlCode;
pub use irp::{IrpCompleter, NoopCompleter};
pub use request::IoRequest;
pub use wdk_safe_macros::define_ioctl;
