# Migration Guide

## 0.1 → 0.2

Version 0.2 introduces two breaking changes and one rename.

---

### 1. `KmdfDriver` renamed to `WdmDriver`

The trait is now named `WdmDriver` because it operates at the WDM dispatch
level (raw `DEVICE_OBJECT` / `IRP` pointers), not the KMDF framework level
(`WDFDEVICE` / `WDFREQUEST`).

```rust
// Before (0.1):
impl KmdfDriver<KernelCompleter> for MyDriver { ... }

// After (0.2):
impl WdmDriver<KernelCompleter> for MyDriver { ... }
```

The re-export `wdk_safe::KmdfDriver` has been removed. Update all imports
and impl blocks.

---

### 2. `IoRequest` buffer/IOCTL accessors now take `&IoStackOffsets`

The raw offset parameters have been replaced with a typed `IoStackOffsets`
struct to eliminate magic numbers.

```rust
// Before (0.1):
let code = request.ioctl_code_at_offset(0x18);
let in_len = request.input_buffer_length_at_offset(0x10);

// After (0.2):
use wdk_safe::ioctl::IoStackOffsets;

let code = request.ioctl_code(&IoStackOffsets::WDK_SYS_0_5_X64);
let in_len = request.input_buffer_length(&IoStackOffsets::WDK_SYS_0_5_X64);
```

For driver crates targeting a different WDK version, define your own constant:

```rust
use wdk_safe::ioctl::IoStackOffsets;

// Offsets verified for your WDK version / architecture:
const MY_OFFSETS: IoStackOffsets = IoStackOffsets {
    ioctl_code: 0x18,
    input_buffer_length: 0x10,
    output_buffer_length: 0x08,
    irp_system_buffer: 0x70,
    irp_information: 0x38,
};
```

---

### 3. `complete_with_information` renamed to `complete_with_info`

```rust
// Before (0.1):
request.complete_with_information(status, bytes, info_ptr)

// After (0.2):
unsafe { request.complete_with_info(status, bytes, info_ptr) }
```

The method is now `unsafe` because the caller must supply a valid pointer to
`IoStatus.Information` inside the IRP.

---

### 4. `dispatch_fn!` macro moved to `wdk_safe`

The dispatch thunk macro is now in the library instead of being copied into
each driver. Remove your local macro definition and use the library version:

```rust
// Before (0.1): you had a local macro_rules! dispatch_thunk!
// After (0.2):
wdk_safe::dispatch_fn!(dispatch_read = MyDriver::on_read [KernelCompleter]);
```

---

### 5. `test-utils` feature for `TrackingCompleter`

`TrackingCompleter` and `TRACKING_COMPLETE_CALLED` are now gated behind the
`test-utils` feature flag:

```toml
[dev-dependencies]
wdk-safe = { ..., features = ["test-utils"] }
```
