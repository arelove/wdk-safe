# Safety Contract for `wdk-safe`

This document describes the safety invariants maintained by the library,
where `unsafe` boundaries lie, what the library guarantees, and what the
caller must guarantee.

---

## `Irp<C: IrpCompleter>`

### Library guarantees

- `Irp::complete` calls `C::complete` **exactly once** and consumes `self`
  via `mem::forget`. It is structurally impossible to call `complete` twice on
  the same `Irp` — the second call is a compile-time "use of moved value" error.
- `Irp::into_raw` transfers ownership out of the type system **without** calling
  `C::complete`. The drop bomb is disarmed via `mem::forget`.
- In **debug builds**, dropping an `Irp` without completing or forwarding
  it fires a `debug_assert!(false, ...)` with a descriptive message.
- `RawIrp` implements `Send` because the kernel guarantees IRP pointers are
  accessible from any thread in the correct synchronisation context.

### Caller must guarantee (`Irp::from_raw`)

- The raw pointer is **non-null** and points to a valid, fully-initialised `IRP`.
- The IRP has **not been completed** already.
- No other `Irp` wraps the same pointer simultaneously (no aliasing).
- `C::complete` will be called at `IRQL <= DISPATCH_LEVEL`.

---

## `IoRequest<'irp, C>`

### Library guarantees

- `IoRequest` is `!Send` and `!Sync` — it must not be moved across thread
  boundaries without explicit driver-level synchronisation.
- `#[must_use]` on the type and the drop bomb on the inner `Irp` enforce that
  every `IoRequest` is either completed or forwarded.
- `complete` and `complete_with_info` delegate to `Irp::complete`, upholding
  the single-completion invariant.

### Caller must guarantee (`IoRequest::from_raw`)

- `irp` is the IRP pointer from the dispatch callback; valid for the duration
  of the call.
- `stack` equals `IoGetCurrentIrpStackLocation(irp)` and is valid for the same
  lifetime.
- The `IoStackOffsets` argument to buffer-reading methods contains correct byte
  offsets for the running WDK version. Use the provided constant
  `IoStackOffsets::WDK_SYS_0_5_X64` or verify offsets in a kernel debug session.

### Caller must guarantee (`IoRequest::system_buffer`)

- This method must only be called for `METHOD_BUFFERED` requests.
- The returned pointer is valid for the duration of the IRP.
  Do not store it past IRP completion.

---

## `Device`

### Library guarantees

- `Device` is `!Send` and `!Sync`. It does not manage the lifetime of the
  underlying `DEVICE_OBJECT`.

### Caller must guarantee (`Device::from_raw`)

- The pointer is non-null and remains valid at least as long as the `Device`
  value is in scope.
- The `DEVICE_OBJECT` is fully initialised (past `DO_DEVICE_INITIALIZING`).

---

## `IoControlCode` / `define_ioctl!`

### Library guarantees

- `IoControlCode::new` is a `const fn`; the encoded bit layout matches the
  Windows `CTL_CODE` macro exactly (bits 31–16 DeviceType, 15–14 Access,
  13–2 Function, 1–0 Method). Verified by exhaustive tests.
- `define_ioctl!` generates correctly-typed constants and type aliases.
  Passing the wrong buffer type to a dispatch handler is a compile error.

### No kernel interaction

`IoControlCode` is a pure value type. No kernel calls are made.

---

## `IrpCompleter` implementations

### `NoopCompleter`

Never dereferences the IRP pointer. Safe to use with any non-null pointer
value in tests.

### `TrackingCompleter`

Never dereferences the IRP pointer. Writes `true` to a global `AtomicBool`.
Tests using this must account for global state (reset the flag before each test
or use `--test-threads=1`).

---

## IRQL constraint table

| Site | Required IRQL |
|------|--------------|
| `Irp::complete` | `<= DISPATCH_LEVEL` |
| `IoRequest::complete` | `<= DISPATCH_LEVEL` |
| `IoRequest::complete_with_info` | `<= DISPATCH_LEVEL` |
| `IoRequest::from_raw` | `<= DISPATCH_LEVEL` |
| `IoRequest::ioctl_code` | Any (pure memory read) |
| `IoRequest::input/output_buffer_length` | Any (pure memory read) |
| `IoRequest::system_buffer` | Any (pure memory read) |
| `Irp::into_raw` | Any |
| `Device::from_raw` | Any |
| `WdmDriver::on_create` | `PASSIVE_LEVEL` |
| `WdmDriver::on_close` | `PASSIVE_LEVEL` |
| `WdmDriver::on_cleanup` | `PASSIVE_LEVEL` |
| `WdmDriver::on_read` | `<= DISPATCH_LEVEL` |
| `WdmDriver::on_write` | `<= DISPATCH_LEVEL` |
| `WdmDriver::on_device_control` | `PASSIVE_LEVEL` |
| `WdmDriver::on_internal_device_control` | `<= DISPATCH_LEVEL` |
| `WdmDriver::on_power` | `PASSIVE_LEVEL` or `DISPATCH_LEVEL` (depends on power IRP type) |
| `WdmDriver::on_pnp` | `PASSIVE_LEVEL` |

---

## `unsafe` boundary summary

| Boundary | Who provides the guarantee |
|----------|---------------------------|
| `Irp::from_raw` | Caller (dispatch thunk) |
| `IoRequest::from_raw` | Caller (dispatch thunk) |
| `Device::from_raw` | Caller (dispatch thunk) |
| `Irp::complete` → `C::complete` | `IrpCompleter` implementor |
| `IoRequest::ioctl_code` stack read | Caller provides correct `IoStackOffsets` |
| `IoRequest::system_buffer` IRP read | Caller guarantees `METHOD_BUFFERED` |
| `IoRequest::complete_with_info` write | Caller provides valid `info_ptr` |
| `dispatch_fn!` generated thunks | I/O manager (valid per WDK contract) |
