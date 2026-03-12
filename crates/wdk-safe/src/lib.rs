// Copyright (c) 2026 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # wdk-safe
//!
//! Safe, idiomatic Rust abstractions for Windows kernel-mode driver
//! development, built on top of [`wdk-sys`](https://crates.io/crates/wdk-sys).
//!
//! ## Relationship to `windows-drivers-rs`
//!
//! `wdk-safe` is **not** a fork or competitor of
//! [`windows-drivers-rs`](https://github.com/microsoft/windows-drivers-rs).
//! It is an experimental safe API layer built directly on `wdk-sys` from that
//! project. Think of it as one possible answer to: *"What should the ergonomic
//! safe wrapper above `wdk-sys` look like?"*
//!
//! ## Design goals
//!
//! | Kernel invariant | Encoding in `wdk-safe` |
//! |---|---|
//! | An IRP must be completed exactly once | [`Irp<C>`] consumes itself on `complete()` — double-complete is a **compile error** |
//! | IRP completion needs `IoCompleteRequest` | [`IrpCompleter`] trait injected at link time — zero-cost, testable without WDK |
//! | IOCTL buffers have specific types | [`define_ioctl!`] declares them at the call site — no untyped `*mut u8` |
//! | `NTSTATUS` semantics differ from `i32` | [`NtStatus`] newtype with severity-bit methods |
//! | `Device` must not outlive its dispatch callback | [`Device<'stack>`] lifetime prevents unsound storage |
//!
//! ## `no_std` and host testing
//!
//! This crate has **zero dependency on `wdk-sys`**. Kernel types are
//! represented as opaque `*mut c_void` pointers. Driver crates cast their real
//! pointers with `.cast()`.
//!
//! The entire test suite runs on any Windows host without a WDK installation:
//!
//! ```powershell
//! cargo test -p wdk-safe -p wdk-safe-macros
//! ```
//!
//! ## Safety
//!
//! See [`SAFETY.md`](../SAFETY.md) for the full safety contract. Every
//! `unsafe` block in this crate has a `// SAFETY:` comment explaining the
//! invariant being upheld.
//!
//! [`Irp<C>`]: irp::Irp
//! [`IrpCompleter`]: irp::IrpCompleter
//! [`define_ioctl!`]: wdk_safe_macros::define_ioctl
//! [`Device<'stack>`]: device::Device

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

pub mod device;
pub mod driver;
pub mod error;
pub mod ioctl;
pub mod irp;
pub mod request;
pub mod thunk;

pub use device::Device;
pub use driver::WdmDriver;
pub use error::NtStatus;
pub use ioctl::IoControlCode;
pub use irp::{IrpCompleter, NoopCompleter};
// Re-export test utilities when the feature is enabled. This allows driver
// crates to write their own unit tests without duplicating these helpers.
#[cfg(any(test, feature = "test-utils"))]
pub use irp::{TrackingCompleter, TRACKING_COMPLETE_CALLED};
pub use request::IoRequest;
pub use wdk_safe_macros::define_ioctl;
