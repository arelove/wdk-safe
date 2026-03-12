# Security Policy

## Scope

`wdk-safe` is a kernel-mode driver library. Security vulnerabilities in
kernel code can lead to privilege escalation, BSOD, or system compromise.
We take security reports seriously.

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.2.x   | ✅ Yes    |
| 0.1.x   | ❌ No     |

## Reporting a vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Please report security issues by emailing the maintainer directly at the
address listed on the GitHub profile, or by using
[GitHub's private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing/privately-reporting-a-security-vulnerability)
on this repository.

Include:

1. A description of the vulnerability and its potential impact.
2. Steps to reproduce (proof-of-concept code if available).
3. The affected version(s).
4. Any suggested mitigations.

We aim to acknowledge receipt within **72 hours** and provide an assessment
within **7 days**.

## Kernel-mode specific notes

Because this crate runs in kernel mode (Ring 0):

- Any memory safety violation can result in a system bug check (BSOD).
- Buffer overflows or use-after-free in kernel code can lead to kernel
  privilege escalation.
- `unsafe` blocks in this crate have explicit `// SAFETY:` justifications.
  If you find an `unsafe` block whose justification is incorrect or
  insufficient, that is a potential vulnerability.

See [`SAFETY.md`](SAFETY.md) for the full safety contract.
