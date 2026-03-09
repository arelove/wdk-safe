// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # wdk-safe
//!
//! Safe, idiomatic Rust abstractions for Windows kernel-mode driver
//! development. This crate is `no_std` compatible and has zero
//! dependencies on `wdk-sys` — kernel types are represented as raw
//! pointers wrapped in newtypes so the crate compiles and tests on
//! any host without a WDK installation.

#![cfg_attr(not(test), no_std)]
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
pub use request::IoRequest;
pub use wdk_safe_macros::define_ioctl;