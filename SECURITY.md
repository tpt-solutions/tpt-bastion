# Security Policy

## Supported Versions

TPT Bastion is pre-1.0. Security fixes target the latest `0.x` release line on
the `main` branch.

## Reporting a Vulnerability

Please report security issues **privately**. Do not open a public GitHub issue
for anything that could disclose a vulnerability.

- Use GitHub's private vulnerability reporting on the repository, or
- Email the maintainers at `security@tpt.dev` with a description, affected
  version, and a minimal reproduction.

We aim to acknowledge within 5 business days and will coordinate a fix and
disclosure timeline with you.

## Threat Model

Bastion is a **local, single-user CLI** plus optional local services. The
trust boundaries are:

| Component | Trust assumption |
| --- | --- |
| `.bcl` documents | Authoritative — the user controls them. |
| State file | Sensitive. May contain resource ids/secrets-adjacent metadata. Encrypted at rest with AES-256-GCM + Argon2id when a passphrase is supplied. |
| Provider plugins | Untrusted by default. Run as isolated OS processes (subprocess ABI); see "Provider isolation" below. |
| `bastion ui` dashboard | Local-only. Binds `127.0.0.1`; supports an optional bearer token. Never expose to a non-loopback interface without a reverse proxy + TLS. |
| `events webhook` | Unauthenticated by default and **plan-only**. When run with `--apply`, a shared secret is **required** (fail-closed) and every request must present it. |

## Safe Defaults

- State is written with `0600` permissions on Unix.
- Encrypted state derives its key from a passphrase read from an **environment
  variable** — never from the command line or logs.
- `apply` requires an explicit `--auto-approve` (or `--state` to persist).
- The webhook listener rejects oversized bodies (1 MiB cap) and enforces a
  per-read timeout to resist slowloris.
- Provider subprocesses are confined with `landlock` (Linux) and bounded by
  resource limits where the platform supports it.

## Provider Isolation

Providers are intentionally run as separate OS processes and communicate only
over the line-delimited JSON ABI. On Linux, `SubprocessRuntime` additionally
applies a `landlock` policy that restricts the provider to read-only access of
system paths and the provider binary, plus `setrlimit` bounds on address space,
file descriptors, and CPU time. Other platforms currently rely on process
isolation alone; this is documented as a platform limitation and tracked for
future sandboxing support.

## Known Gaps

- The webhook/UI servers bind loopback only; **loopback is not authentication** —
  always use the token option (or a reverse proxy) when the listener can trigger
  mutating operations.
- Cooperative state locking (like Terraform's) is not OS-enforced; it relies on
  every writer checking the lock file first.
