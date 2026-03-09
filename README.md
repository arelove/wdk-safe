# wdk-safe

[![CI](https://github.com/arelove/wdk-safe/actions/workflows/ci.yml/badge.svg)](https://github.com/arelove/wdk-safe/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/wdk-safe.svg)](https://crates.io/crates/wdk-safe)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Safe, idiomatic Rust abstractions for Windows kernel-mode driver development,
built on top of [`wdk-sys`](https://crates.io/crates/wdk-sys) raw FFI bindings
from [microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs).

## Motivation

`wdk-sys` provides complete, low-level bindings to the Windows Driver Kit (WDK)
API. Writing drivers directly against these bindings is possible but requires
pervasive `unsafe` code. `wdk-safe` provides a higher-level layer where the
Rust compiler enforces correct driver patterns at compile time:

| Problem in raw `wdk-sys` | Solution in `wdk-safe` |
|--------------------------|------------------------|
| `IRP` must be completed exactly once; forgetting it hangs the system | `Irp` consumes itself on `complete()` — double-complete is a compile error |
| IOCTL buffer types are untyped `*mut u8` | `define_ioctl!` macro declares input/output types checked at compile time |
| Every access to kernel structures requires `unsafe` | Safe wrappers for `DEVICE_OBJECT`, `IO_STACK_LOCATION`, etc. |

## Status

> **Early development.** APIs are unstable and subject to change.
> Not recommended for production use. Community experimentation welcome.

## Crate Layout

| Crate | Description |
|-------|-------------|
| `wdk-safe` | Core safe abstractions |
| `wdk-safe-macros` | Procedural macros (`define_ioctl!`) |

## Usage

Add to your driver's `Cargo.toml`:

```toml
[dependencies]
wdk-safe = "0.1.0"

[build-dependencies]
wdk-build = "0.5.1"
```

Implement [`KmdfDriver`](crates/wdk-safe/src/driver.rs) for your driver struct:

```rust
use wdk_safe::{define_ioctl, Device, IoRequest, KmdfDriver, NtStatus};

#[repr(C)]
pub struct EchoRequest  { pub value: u32 }
#[repr(C)]
pub struct EchoResponse { pub value: u32 }

define_ioctl!(IOCTL_ECHO, 0x8000u16, 0x800u16, EchoRequest => EchoResponse);

struct EchoDriver;

impl KmdfDriver for EchoDriver {
    fn on_device_control(_device: &Device, request: IoRequest) -> NtStatus {
        // Type-safe dispatch — no raw pointer arithmetic
        request.complete(NtStatus::SUCCESS)
    }
}
```

## Building

Requirements:

- [eWDK NI (22H2)](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/using-the-enterprise-wdk)
  — open an eWDK developer prompt before building
- [LLVM 17.0.6](https://github.com/llvm/llvm-project/releases/tag/llvmorg-17.0.6)
  — required by `bindgen` for binding generation
- [`cargo-make`](https://github.com/sagiegurari/cargo-make)

```powershell
# Install tools (run once)
winget install -i LLVM.LLVM --version 17.0.6 --force
cargo install --locked cargo-make --no-default-features --features tls-native

# Build (inside eWDK developer prompt)
cargo make
```

## Testing

Unit tests (no WDK required, run on host):

```powershell
cargo test -p wdk-safe -p wdk-safe-macros
```

Runtime tests require a Hyper-V test VM with kernel debugging enabled.
See [CONTRIBUTING.md](CONTRIBUTING.md) for the full setup guide.

## Related Projects

- [microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs) — the official WDK Rust bindings this crate builds upon
- [microsoft/Windows-rust-driver-samples](https://github.com/microsoft/Windows-rust-driver-samples) — official driver samples

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.