# wdk-safe
[![CI](https://github.com/arelove/wdk-safe/actions/workflows/ci.yml/badge.svg)](https://github.com/arelove/wdk-safe/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Tests](https://img.shields.io/badge/tests-91%20passing-brightgreen)](#testing)

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
| `STATUS_*` are bare `i32` constants | `NtStatus` newtype with `is_success()`, `is_error()`, severity bits |

---

## Current Progress

### ✅ Phase 1 — Core library (complete)

| Module | What it provides |
|--------|-----------------|
| `error` | `NtStatus` — strongly-typed NTSTATUS: `is_success()`, `is_error()`, `is_warning()`, `is_informational()`, `Hash`, `Copy` |
| `ioctl` | `IoControlCode` — const-constructible IOCTL builder with `method()` and `access()` decoders |
| `irp` | `Irp<C>` — ownership-based IRP wrapper with drop bomb; `IrpCompleter` trait; `NoopCompleter` for tests |
| `device` | `Device` — safe non-owning `DEVICE_OBJECT` reference |
| `request` | `IoRequest<C>` — dispatch abstraction: IOCTL code, buffer lengths, system buffer, `complete_with_information` |
| `driver` | `KmdfDriver<C>` trait — override only the IRP major functions you handle |
| `wdk-safe-macros` | `define_ioctl!` — type-safe IOCTL macro with optional `method` and `access` parameters |

### ✅ Phase 2 — HID keyboard filter example (complete)

A complete KMDF upper-filter driver for HID keyboards demonstrating every
abstraction in the library. Logs each keystroke via `DbgPrint` (visible in
WinDbg / DebugView). Zero `unsafe` in driver dispatch code.

Key implementation details:
- `KernelCompleter` — implements `IrpCompleter` via `IofCompleteRequest`
- `FilterDeviceExtension` — per-device state, no global mutable state
- `AddDevice` — `IoAttachDeviceToDeviceStack` + stack size accounting
- `dispatch_thunk!` macro — eliminates boilerplate dispatch functions
- Power and PnP forwarding down the device stack

**Build results (eWDK 26100.6584, KMDF 1.33, Rust nightly):**
```
cargo build  →  SUCCESS, 0 errors, 0 warnings
cargo make   →  SUCCESS — .sys signed, inf2cat OK, infverif VALID
```

**Key INF/INX lessons learned** (for future reference):

- wdk-build looks for `hid_filter.inx` (underscore), not `hid-filter.inx`
- `DestinationDirs` must use DIRID `13` (driver store) — required by `infverif /w` on Win11
- `[Manufacturer]` decoration: `NT$ARCH$.10.0...16299` (three dots = build number wildcard)
- `AddService` flag must be `0x00000002` (`SPSVCINST_ASSOCSERVICE`) — required by `infverif /w`
- Install section names (`[HidFilter_Install]`, `.HW`, `.Services`) must **not** carry the
  `.NT$ARCH$` suffix — infverif looks up the exact name from `[Standard.*]` and fails if it
  finds only the decorated variant

### 🚧 Phase 3 — In progress

- [x] `cargo build` — SUCCESS in eWDK developer prompt
- [x] `cargo make` — `.sys` + `.inf` packaged, signed, infverif VALID
- [ ] Install in Hyper-V test VM and verify via WinDbg / DebugView
- [ ] Integration tests — user-mode client sends `DeviceIoControl`
- [ ] Open Discussion in `microsoft/windows-drivers-rs`
- [ ] Publish to crates.io

---

## Testing

Unit tests run on any Windows host — no WDK installation required:

```powershell
cargo test -p wdk-safe -p wdk-safe-macros
```

```
running 76 tests ... ok   (unit tests: error, ioctl, irp, request, device, driver)
running 15 tests ... ok   (macro integration: define_ioctl! all variants)
Doc-tests: 3 passed, 2 ignored (kernel-only examples requiring wdk-sys)
```

The two `ignored` doc-tests are kernel-only code examples in documentation
(they use `wdk-sys` types unavailable on the host). They are intentionally
marked `rust,ignore` — the code is valid and shown in rustdoc, but cannot
be compiled without a WDK installation.

---

## Usage

```toml
[dependencies]
wdk-safe = { git = "https://github.com/arelove/wdk-safe" }
```

Implement `KmdfDriver` for your driver struct:

```rust
use wdk_safe::{Device, IoRequest, KmdfDriver, NtStatus};
use wdk_safe::irp::NoopCompleter;

struct MyDriver;

impl KmdfDriver<NoopCompleter> for MyDriver {
    fn on_device_control(_device: &Device, request: IoRequest<'_, NoopCompleter>) -> NtStatus {
        request.complete(NtStatus::SUCCESS)
    }
}
```

Declare type-safe IOCTLs with the `define_ioctl!` macro:

```rust
use wdk_safe::define_ioctl;

#[repr(C)] pub struct EchoRequest  { pub value: u32 }
#[repr(C)] pub struct EchoResponse { pub value: u32 }

// Minimal — defaults to METHOD_BUFFERED, FILE_ANY_ACCESS
define_ioctl!(IOCTL_ECHO, 0x8000u16, 0x800u16, EchoRequest => EchoResponse);

// Explicit transfer method and required access
define_ioctl!(
    IOCTL_READ_DATA,
    0x8000u16, 0x801u16,
    EchoRequest => EchoResponse,
    method = InDirect,
    access = Read,
);
```

The macro generates:
- `pub const IOCTL_ECHO: IoControlCode` — the validated code constant
- `pub type IoctlEchoInput = EchoRequest` — input buffer type alias
- `pub type IoctlEchoOutput = EchoResponse` — output buffer type alias

---

## Building

Unit tests run on any Windows host — no WDK required:

```powershell
cargo test -p wdk-safe -p wdk-safe-macros
```

Building the HID filter example requires the full WDK toolchain:

| Tool | Version | Install |
|------|---------|---------|
| [eWDK](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/using-the-enterprise-wdk) | 25H2 (26100.x) | Download from Microsoft |
| [LLVM](https://github.com/llvm/llvm-project/releases/tag/llvmorg-17.0.6) | 17.0.6 | `winget install -i LLVM.LLVM --version 17.0.6` |
| cargo-make | latest | `cargo install cargo-make` |

```powershell
# Inside an eWDK developer prompt:
cd examples/hid-filter/hid-filter
cargo make
```

Successful output ends with:
```
infverif → INF is VALID
[cargo-make] INFO - Build Done in ~3 seconds.
```

The package is produced in `examples/target/debug/hid_filter_package/`:
```
hid_filter.sys   — signed kernel driver binary
hid_filter.inf   — stamped INF (DriverVer filled by stampinf)
hid_filter.cat   — signed catalog
hid_filter.pdb   — debug symbols
```

---

## Repository Layout

```
wdk-safe/
├── crates/
│   ├── wdk-safe/          # Core safe abstractions (91 tests)
│   └── wdk-safe-macros/   # Procedural macros (define_ioctl!)
├── examples/
│   └── hid-filter/        # Complete KMDF HID keyboard filter driver
└── tests/
    └── integration/       # User-mode integration tests (Phase 3)
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

Copyright (c) 2025 arelove