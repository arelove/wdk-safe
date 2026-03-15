# Contributing to wdk-safe

Thank you for your interest in contributing!

This project follows the same conventions as
[microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs).

## Development Environment

### Required Tools

| Tool | Version | Purpose |
|------|---------|---------|
| [eWDK NI](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/using-the-enterprise-wdk) | 22H2 | WDK headers and libs |
| [LLVM](https://github.com/llvm/llvm-project/releases/tag/llvmorg-17.0.6) | 17.0.6 | `bindgen` binding generation |
| `cargo-make` | latest | Driver packaging tasks |
| `typos-cli` | latest | Typo checking |
| `taplo-cli` | latest | TOML formatting |

```powershell
winget install -i LLVM.LLVM --version 17.0.6 --force  # add to PATH when prompted
cargo install --locked cargo-make --no-default-features --features tls-native
cargo install typos-cli
cargo install taplo-cli
```

### Runtime Test VM (Hyper-V)

Kernel-mode tests require a dedicated Windows VM:

1. Enable Hyper-V: `Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V`
2. Create a Windows 11 VM in Hyper-V Manager.
3. In the VM, enable test signing and kernel debugging:
   ```cmd
   bcdedit /set testsigning on
   bcdedit /debug on
   bcdedit /dbgsettings net hostip:<your-ip> port:50000
   ```
4. On the host, connect WinDbg: **File → Attach to kernel → Net**, port 50000.
5. Copy the built `.sys` file to the VM via the Hyper-V shared folder.
6. Load the driver in the VM:
   ```cmd
   sc create wdk-safe-test type= kernel binPath= C:\drivers\wdk_safe_test.sys
   sc start wdk-safe-test
   ```

## Code Style

All code must pass the following before a PR is merged:

```powershell
cargo fmt --all                          # format
cargo clippy --all-targets -- -D warnings # lint
cargo test -p wdk-safe -p wdk-safe-macros # unit tests
typos                                    # typo check
```

These are enforced by CI on every pull request.

### Conventions

- Every public item must have a doc comment (`///`).
- `unsafe` blocks must have a `// SAFETY:` comment explaining the invariant.
- New modules go in `crates/wdk-safe/src/`.
- Tests go in a `#[cfg(test)] mod tests` block at the bottom of the same file.

## Opening Issues and Discussions

- Bug reports and feature requests → [GitHub Issues](https://github.com/arelove/wdk-safe/issues)
- Design discussions and questions → [GitHub Discussions](https://github.com/arelove/wdk-safe/discussions)

For questions about the underlying WDK bindings, see
[microsoft/windows-drivers-rs discussions](https://github.com/microsoft/windows-drivers-rs/discussions).