# Security Policy

SkyEngine is a native runtime project. Security issues may involve unsafe Rust,
FFI integrations, asset parsing, file watching, archive/package handling,
audio/video decoding, or GPU/resource lifecycle bugs.

## Supported Versions

Security fixes target the active development line until the project starts
maintaining multiple release branches.

## Reporting a Vulnerability

Please report suspected vulnerabilities privately through GitHub Security
Advisories for `jz315/SkyEngine`, or contact the maintainer through the
repository if advisories are unavailable.

Include:

- affected commit or release,
- platform and feature flags,
- reproduction steps or proof of concept,
- expected and observed behavior,
- whether the issue is exploitable from untrusted assets or project files.

Do not publish exploit details until a fix or mitigation is available.
