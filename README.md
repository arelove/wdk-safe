# wdk-safe

[![CI](https://github.com/arelove/wdk-safe/actions/workflows/ci.yml/badge.svg)](https://github.com/arelove/wdk-safe/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Safe, idiomatic Rust abstractions for Windows kernel-mode driver development,
built on top of [`wdk-sys`](https://crates.io/crates/wdk-sys) from
[microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs).

> **Status: experimental.** APIs are unstable. Not recommended for production
> use. Community experimentation and feedback welcome.

---

## Relationship to `windows-drivers-rs`

`wdk-safe` is **not** a fork or competitor. It is an experimental safe API
layer built directly on `wdk-sys` from that project. Think of it as one
possible answer to the question: *"What should the ergonomic safe wrapper
above `wdk-sys` look like?"*

The crate deliberately has **zero dependency on `wdk-sys`** in its core logic.
This allows the entire test suite to run on any Windows host without a WDK
installation.

---

## Motivation

Writing kernel drivers against raw `wdk-sys` works, but pervasive `unsafe`
means the compiler cannot catch invariant violations that would cause BSODs.
`wdk-safe` encodes those invariants in Rust's type system:

| Kernel invariant | How `wdk-safe` encodes it |
|---|---|
| An IRP must be completed **exactly once** | `Irp<C>` consumes itself on `complete()` — double-complete is a **compile error** |
| Forgetting to complete an IRP hangs the system | `#[must_use]` + drop bomb fires in debug builds |
| `IoCompleteRequest` needs `wdk-sys` — tests don't | `IrpCompleter` trait injected at compile time — zero-cost, testable without WDK |
| IOCTL buffers are untyped `*mut u8` | `define_ioctl!` declares input/output types at the call site |
| `NTSTATUS` is semantically different from `i32` | `NtStatus` newtype with severity-bit predicates |

---

## Design overview

### `Irp<C: IrpCompleter>` — linear IRP ownership

The `IrpCompleter` trait abstracts `IoCompleteRequest`. A driver crate
implements it once as a zero-sized type:

```rust,ignore
pub struct KernelCompleter;

impl IrpCompleter for KernelCompleter {
    unsafe fn complete(irp: *mut core::ffi::c_void, status: i32) {
        unsafe {
            let pirp = irp.cast::<wdk_sys::IRP>();
            (*pirp).IoStatus.__bindgen_anon_1.Status = status;
            wdk_sys::ntddk::IofCompleteRequest(pirp, wdk_sys::IO_NO_INCREMENT as i8);
        }
    }
}
```

The type parameter propagates through `Irp<C>` and `IoRequest<C>` with zero
runtime cost — `KernelCompleter` is a ZST, so `Irp<KernelCompleter>` is the
same size as a raw pointer.

### `WdmDriver<C>` — override only what you handle

```rust
use wdk_safe::{irp::NoopCompleter, Device, IoRequest, WdmDriver, NtStatus};

struct MyDriver;

impl WdmDriver<NoopCompleter> for MyDriver {
    fn on_device_control(
        _device: &Device,
        request: IoRequest<'_, NoopCompleter>,
    ) -> NtStatus {
        request.complete(NtStatus::SUCCESS)
    }
    // All other IRP majors default to STATUS_NOT_SUPPORTED.
    // on_create / on_close / on_cleanup default to STATUS_SUCCESS.
}
```

> **Naming note:** The trait is called `WdmDriver` because it operates at the
> WDM dispatch level — directly with `DEVICE_OBJECT` and `IRP` pointers —
> not through KMDF abstractions.

### `define_ioctl!` — type-safe IOCTL declarations

```rust
use wdk_safe::define_ioctl;

#[repr(C)] pub struct EchoRequest  { pub value: u32 }
#[repr(C)] pub struct EchoResponse { pub value: u32 }

// Minimal — defaults to METHOD_BUFFERED, FILE_ANY_ACCESS.
define_ioctl!(IOCTL_ECHO, 0x8000u16, 0x800u16, EchoRequest => EchoResponse);

// Full — explicit method and access flags.
define_ioctl!(
    IOCTL_READ_DATA,
    0x8000u16, 0x801u16,
    EchoRequest => EchoResponse,
    method = InDirect,
    access = Read,
);
```

The macro generates:
- `pub const IOCTL_ECHO: IoControlCode` — a validated code constant
- `pub type IoctlEchoInput = EchoRequest` — input buffer type alias
- `pub type IoctlEchoOutput = EchoResponse` — output buffer type alias

### `IoStackOffsets` — no magic numbers for field access

```rust,ignore
use wdk_safe::ioctl::IoStackOffsets;

let code   = request.ioctl_code(&IoStackOffsets::WDK_SYS_0_5_X64);
let in_len = request.input_buffer_length(&IoStackOffsets::WDK_SYS_0_5_X64);
```

---

## Safety guarantees

See [`SAFETY.md`](docs/SAFETY.md) for the full contract. Key points:

- `Irp::complete` is the only path to `IoCompleteRequest` through this crate;
  calling it **consumes** the `Irp` so it cannot be called twice.
- `IoRequest` is `!Send` — it must not cross thread boundaries without
  driver-provided synchronisation.
- All `unsafe` blocks carry `// SAFETY:` comments.
- The crate enforces `unsafe_op_in_unsafe_fn = deny` workspace-wide.
- IRQL constraints are documented on every method that requires them.

---

## Non-goals

- **Not a KMDF wrapper.** This crate does not wrap `WDFDEVICE`, `WDFREQUEST`,
  or `WDFQUEUE`. It operates at the WDM dispatch level.
- **Not a replacement for `wdk-sys`.** It wraps it.
- **No async/await.** Kernel Rust async is a separate research area.
- **No allocation abstractions.** Use `wdk-alloc` directly.

---

## Examples

Three complete, buildable WDM driver examples are included. Each builds into
a signed `.sys` + `.inf` package that can be installed directly in a VM.

| Example | What it demonstrates |
|---|---|
| [`null-device`](examples/null-device/) | Minimal WDM driver skeleton — `DriverEntry`, `DriverUnload`, `IRP_MJ_CREATE/CLOSE/WRITE` lifecycle |
| [`ioctl-echo`](examples/ioctl-echo/) | `define_ioctl!`, type-safe IOCTL dispatch, buffered I/O, user↔kernel round-trip |
| [`hid-filter`](examples/hid-filter/) | WDM upper filter over a HID device — IRP pass-through, filter stack attachment |

### Building

| Tool | Version | Install |
|------|---------|---------|
| [eWDK](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/using-the-enterprise-wdk) | 25H2 (26100.x) | Download from Microsoft |
| [LLVM](https://github.com/llvm/llvm-project/releases/tag/llvmorg-17.0.6) | 17.0.6 | `winget install -i LLVM.LLVM --version 17.0.6` |
| `cargo-make` | latest | `cargo install cargo-make` |

```powershell
# Inside an eWDK developer prompt:
cd examples/null-device/null-device
cargo make
```

### Testing in a VM (Windows 11, test-signing mode)

```cmd
:: 1. Enable test-signing (reboot required on first run)
bcdedit /set testsigning on

:: 2. Import the test certificate (once per VM)
certutil -addstore "Root" WDRLocalTestCert.cer
certutil -f -addstore "TrustedPublisher" WDRLocalTestCert.cer

:: 3. Register the driver package
pnputil /add-driver null_device.inf /install

:: 4. Find the DriverStore path and start the driver
dir /s /b "C:\Windows\System32\DriverStore\FileRepository\null_device*\null_device.sys"
sc create null-device type= kernel binPath= "<path from above>"
sc start null-device

:: 5. Exercise it
echo hello > \\.\WdkSafeNull

:: 6. Stop
sc stop null-device
```

Use [DebugView](https://learn.microsoft.com/en-us/sysinternals/downloads/debugview)
(Capture → Capture Kernel) to observe driver lifecycle messages in real time:

```
[null-device] DriverEntry -- loading
[null-device] DriverEntry -- ready
[null-device] IRP_MJ_CREATE
[null-device] IRP_MJ_WRITE -- discarding
[null-device] IRP_MJ_CLOSE
[null-device] DriverUnload -- cleaning up
```

### Testing `ioctl-echo` — usermode round-trip

A test utility is included at
[`examples/ioctl-echo/ioctl-echo-test`](examples/ioctl-echo/ioctl-echo-test/).
Build on the host (no WDK needed), copy the `.exe` to the VM:

```powershell
# On host
cd examples/ioctl-echo/ioctl-echo-test
cargo build --release
# Copy target/release/ioctl_echo_test.exe to the VM
```

```cmd
:: In the VM (ioctl-echo driver must be running)
ioctl_echo_test.exe
ioctl_echo_test.exe 12345
ioctl_echo_test.exe 0xDEADBEEF
```

Expected output:

```
=== ioctl-echo test ===
Device : \\.\WdkSafeEcho
IOCTL  : 0x80002000
Send   : 0xDEADBEEF (3735928559)
Opened device handle: OK
Received: 0xDEADBEEF (3735928559)
Bytes returned: 4
✓ PASS — echo correct
```

DebugView will show:

```
[ioctl-echo] IRP_MJ_CREATE
[ioctl-echo] echoing value
[ioctl-echo] IRP_MJ_CLOSE
```

---

## Testing (host, no WDK required)

Unit and integration tests run on any Windows host:

```powershell
cargo test -p wdk-safe -p wdk-safe-macros
```

The `test-utils` feature exposes `TrackingCompleter` for testing dispatch logic
without a running kernel:

```toml
[dev-dependencies]
wdk-safe = { ..., features = ["test-utils"] }
```

---

## Repository layout

```
wdk-safe/
├── crates/
│   ├── wdk-safe/              # Core safe abstractions
│   │   └── src/
│   │       ├── lib.rs         # Public API surface
│   │       ├── error.rs       # NtStatus newtype
│   │       ├── ioctl.rs       # IoControlCode, IoStackOffsets
│   │       ├── irp.rs         # Irp<C>, IrpCompleter, NoopCompleter
│   │       ├── request.rs     # IoRequest<C>
│   │       ├── device.rs      # Device (non-owning DEVICE_OBJECT ref)
│   │       ├── driver.rs      # WdmDriver<C> trait
│   │       └── thunk.rs       # dispatch_fn! macro
│   └── wdk-safe-macros/       # Proc-macros (define_ioctl!)
├── examples/
│   ├── null-device/           # Minimal WDM driver skeleton
│   ├── ioctl-echo/            # IOCTL round-trip demo
│   └── hid-filter/            # WDM HID keyboard filter driver
├── tests/
│   └── integration/           # Host-runnable macro integration tests
├── docs/
│   ├── SAFETY.md              # Safety contract and invariants
│   └── SECURITY.md
└── Migration.md               # Upgrade notes between versions
```

---

## Related projects

- [microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs) — official WDK Rust bindings this crate builds upon
- [microsoft/Windows-rust-driver-samples](https://github.com/microsoft/Windows-rust-driver-samples) — official driver samples

---

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

Copyright (c) 2026 arelove