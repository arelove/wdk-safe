// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Build script for the `hid-filter` kernel-mode driver.
//!
//! Delegates to [`wdk_build::configure_wdk_binary_build`] which sets up the
//! correct linker flags, library paths, and WDK metadata for a KMDF driver,
//! and emits additional `/ALTERNATENAME` directives to stub out
//! `fma`/`fmaf` symbols that `compiler_builtins` references but that do not
//! exist in kernel-mode import libraries.

fn main() -> Result<(), wdk_build::ConfigError> {
    // ── Kernel-mode float intrinsic stubs ────────────────────────────────────
    //
    // `compiler_builtins 0.1.160` unconditionally compiles `math::libm_math`
    // on x86_64-pc-windows-msvc, which references the C runtime symbols `fma`
    // and `fmaf`.  These symbols do not exist in kernel-mode libraries
    // (ntoskrnl.lib / hal.lib), causing LNK2019 at link time.
    //
    // The fix: emit linker `/ALTERNATENAME` directives that redirect
    // `fma` → `__fma_stub` and `fmaf` → `__fmaf_stub`, then define those
    // stubs as no-op functions that the compiler_builtins code will never
    // actually call (floating-point fused-multiply-add is not used at runtime
    // in kernel code — the references are dead code pulled in by the generic
    // trait impl).
    //
    // `/ALTERNATENAME:sym=fallback` is the MSVC linker's "weak symbol"
    // mechanism: if `sym` is unresolved, the linker uses `fallback` instead.
    println!("cargo:rustc-link-arg=/ALTERNATENAME:fma=__wdk_fma_stub");
    println!("cargo:rustc-link-arg=/ALTERNATENAME:fmaf=__wdk_fmaf_stub");

    wdk_build::configure_wdk_binary_build()
}
