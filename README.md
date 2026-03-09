# wdk-safe

[![CI](https://github.com/arelove/wdk-safe/actions/workflows/ci.yml/badge.svg)](https://github.com/arelove/wdk-safe/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Safe, idiomatic Rust abstractions for Windows kernel-mode driver development,
built on top of [`wdk-sys`](https://crates.io/crates/wdk-sys) from
[microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs).

> **Status: early development.** APIs are unstable. Not recommended for
> production use. Community experimentation and feedback welcome.

---

## Motivation

`wdk-sys` gives you complete access to the Windows Driver Kit API from Rust.
Writing drivers directly against it works, but requires pervasive `unsafe`
code and manually enforcing kernel invariants that the compiler could catch.

`wdk-safe` wraps that layer so the **compiler** enforces correct driver
patterns at compile time:

| Problem in raw `wdk-sys` | Solution in `wdk-safe` |
|--------------------------|------------------------|
| `IRP` must be completed exactly once — forgetting it hangs the system | `Irp` consumes itself on `complete()` — double-complete is a compile error |
| IOCTL buffer types are untyped `*mut u8` | `define_ioctl!` declares input/output types checked at compile time |
| Every kernel struct access requires `unsafe` | Safe wrappers for `DEVICE_OBJECT`, `IO_STACK_LOCATION`, etc. |
| `STATUS_*` are bare `i32` constants | `NtStatus` newtype with `is_success()`, `is_error()`, `Debug` |

---

## Current Progress

### ✅ Phase 1 — Core library (complete, 17 tests passing)

| Module | What it provides |
|--------|-----------------|
| `error` | `NtStatus` — strongly-typed NTSTATUS with `is_success()`, `is_error()` |
| `ioctl` | `IoControlCode` — const-constructible IOCTL code builder |
| `irp` | `Irp` — ownership-based IRP wrapper, compiler-enforced completion |
| `device` | `Device` — safe `DEVICE_OBJECT` reference |
| `request` | `IoRequest` — dispatch abstraction wrapping `Irp` + stack location |
| `driver` | `KmdfDriver` trait — implement this for your driver |
| `wdk-safe-macros` | `define_ioctl!` — type-safe IOCTL declaration macro |

### 🚧 Phase 2 — HID keyboard filter example (in progress)

A complete KMDF upper-filter driver for HID keyboards that demonstrates
every abstraction in the library. Logs each keystroke via `DbgPrint`
(visible in WinDbg). Built on top of `wdk-safe` with zero `unsafe` in
driver dispatch code.

### 📋 Phase 3 — Planned

- Integration tests via `DeviceIoControl` from user-mode test client
- Hyper-V test VM automation
- Publish to crates.io

---

## Usage

```toml
[dependencies]
wdk-safe = { git = "https://github.com/arelove/wdk-safe" }
```

Implement `KmdfDriver` for your driver struct:

```rust
use wdk_safe::{Device, IoRequest, KmdfDriver, NtStatus};

struct MyDriver;

impl KmdfDriver for MyDriver {
    fn on_device_control(_device: &Device, request: IoRequest<'_>) -> NtStatus {
        request.complete(NtStatus::SUCCESS)
    }
}
```

Declare type-safe IOCTLs with the `define_ioctl!` macro:

```rust
use wdk_safe::define_ioctl;

#[repr(C)]
pub struct EchoRequest  { pub value: u32 }
#[repr(C)]
pub struct EchoResponse { pub value: u32 }

define_ioctl!(IOCTL_ECHO, 0x8000u16, 0x800u16, EchoRequest => EchoResponse);
```

---

## Building

Unit tests run on any Windows host — no WDK required:

```powershell
cargo test -p wdk-safe -p wdk-safe-macros
```

Building the HID filter example requires:

| Tool | Version | Install |
|------|---------|---------|
| [eWDK NI](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/using-the-enterprise-wdk) | 22H2 | Download from Microsoft |
| [LLVM](https://github.com/llvm/llvm-project/releases/tag/llvmorg-17.0.6) | 17.0.6 | `winget install -i LLVM.LLVM --version 17.0.6` |
| cargo-make | latest | `cargo install cargo-make` |

```powershell
# Inside an eWDK developer prompt:
cd examples/hid-filter/hid-filter
cargo make
```

---

## Repository Layout

```
wdk-safe/
├── crates/
│   ├── wdk-safe/          # Core safe abstractions
│   └── wdk-safe-macros/   # Procedural macros (define_ioctl!)
├── examples/
│   └── hid-filter/        # HID keyboard filter driver (WIP)
└── tests/
    └── integration/       # User-mode integration tests (planned)
```

---

## Related Projects

- [microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs) — official WDK Rust bindings this crate builds upon
- [microsoft/Windows-rust-driver-samples](https://github.com/microsoft/Windows-rust-driver-samples) — official driver samples

---

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Copyright (c) 2025 arelove