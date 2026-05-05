# Security Policy

## Supported Versions

wali is pre-1.0. Security fixes are provided for the latest released version only.

| Version | Supported |
| ------- | --------- |
| 0.1.x   | Yes       |
| < 0.1   | No        |

## Reporting a Vulnerability

Please do not report security vulnerabilities through public GitHub issues,
discussions, or pull requests.

Report vulnerabilities privately by using GitHub private vulnerability reporting,
if it is enabled for this repository.

If private vulnerability reporting is not enabled, contact the maintainer directly.

When reporting a vulnerability, please include:

- affected wali version, commit, or tag;
- operating system and architecture;
- manifest or module code needed to reproduce the issue;
- exact command line used;
- expected behavior;
- observed behavior;
- security impact;
- whether the issue affects local execution, SSH execution, file transfer,
  secret handling, module loading, cleanup, or state handling.

## Scope

Security issues include, but are not limited to:

- command execution escaping the intended wali behavior;
- unsafe handling of secrets;
- unsafe file writes, deletes, permissions, or ownership changes;
- path traversal in module loading or transfer operations;
- unexpected execution of untrusted Lua code;
- state-file behavior that can cause unsafe cleanup;
- SSH behavior that exposes credentials or runs commands on an unintended host.

## Non-Security Bugs

General correctness bugs, usability issues, documentation problems, and feature
requests should be reported through normal GitHub issues.
