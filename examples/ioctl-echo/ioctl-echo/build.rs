// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Build script for the `ioctl-echo` kernel-mode driver.
//!
//! Stubs out `fma`/`fmaf` C runtime symbols that `compiler_builtins`
//! references but that do not exist in kernel-mode import libraries
//! (ntoskrnl.lib / hal.lib), then delegates to `wdk_build`.

fn main() -> Result<(), wdk_build::ConfigError> {
    println!("cargo:rustc-link-arg=/ALTERNATENAME:fma=__wdk_fma_stub");
    println!("cargo:rustc-link-arg=/ALTERNATENAME:fmaf=__wdk_fmaf_stub");
    wdk_build::configure_wdk_binary_build()
}