# Migration Guide: Phase 1 → Phase 2

`wdk-safe` 0.2 introduces a type parameter on `Irp`, `IoRequest`, and
`KmdfDriver` to make the `IoCompleteRequest` call injectable at compile
time. This allows the crate to remain dependency-free on `wdk-sys` while
driver crates provide the real kernel function at link time.

---

## What changed

### `Irp<C>` and `IoRequest<C: IrpCompleter>`

Both types now carry a `C: IrpCompleter` type parameter.

**Before (0.1)**
```rust
pub struct Irp<'irp> { /* ... */ }
pub struct IoRequest<'irp> { /* ... */ }
```

**After (0.2)**
```rust
pub struct Irp<C: IrpCompleter> { /* ... */ }
pub struct IoRequest<C: IrpCompleter> { /* ... */ }
```

### `KmdfDriver<C: IrpCompleter>`

The trait is now generic. All methods change their `IoRequest` parameter.

**Before**
```rust
impl KmdfDriver for MyDriver {
    fn on_create(_device: &Device, request: IoRequest<'_>) -> NtStatus { ... }
}
```

**After**
```rust
impl KmdfDriver<KernelCompleter> for MyDriver {
    fn on_create(_device: &Device, request: IoRequest<'_, KernelCompleter>) -> NtStatus { ... }
}
```

### `Irp::into_raw` (renamed from nothing)

The old `IoRequest::into_raw_irp` still exists. The inner `Irp` also now
exposes `into_raw` (replacing the private `as_raw_ptr`).

---

## Migration steps

### 1. Implement `IrpCompleter` in your driver crate

```rust
// In your driver crate (links wdk-sys):
use wdk_safe::IrpCompleter;

pub struct KernelCompleter;

impl IrpCompleter for KernelCompleter {
    unsafe fn complete(irp: *mut core::ffi::c_void, status: i32) {
        unsafe {
            let pirp = irp.cast::<wdk_sys::IRP>();
            (*pirp).IoStatus.__bindgen_anon_1.Status = status;
            wdk_sys::ntddk::IoCompleteRequest(pirp, wdk_sys::IO_NO_INCREMENT as i8);
        }
    }
}
```

### 2. Update `KmdfDriver` impl

Replace `KmdfDriver` with `KmdfDriver<KernelCompleter>` and update every
method signature:

```rust
// Before:
impl KmdfDriver for MyDriver {
    fn on_read(_dev: &Device, req: IoRequest<'_>) -> NtStatus { ... }
}

// After:
impl KmdfDriver<KernelCompleter> for MyDriver {
    fn on_read(_dev: &Device, req: IoRequest<'_, KernelCompleter>) -> NtStatus { ... }
}
```

### 3. Update dispatch thunks

```rust
// Before:
unsafe { IoRequest::from_raw(irp.cast(), stack.cast()) }

// After (only the type annotation changes — the call is identical):
unsafe { IoRequest::<KernelCompleter>::from_raw(irp.cast(), stack.cast()) }
```

### 4. Update unit tests

In tests, replace all `IoRequest<'_>` with `IoRequest<NoopCompleter>`:

```rust
use wdk_safe::NoopCompleter;

let req = unsafe { IoRequest::<NoopCompleter>::from_raw(ptr, stack) };
```

---

## Zero runtime cost

`IrpCompleter` is a trait with a single static dispatch method. The
compiler monomorphises `Irp<KernelCompleter>` to a struct that calls
`IoCompleteRequest` directly — identical codegen to the old hardcoded call.
`KernelCompleter` is a zero-sized type; `Irp<KernelCompleter>` is the same
size as `Irp` was before.