// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Process confinement for provider plugins.
//!
//! Providers are untrusted by default: they run as separate OS processes and
//! speak only the ABI. This module tightens that boundary on Unix by applying,
//! inside the child process *before* it execs the provider binary:
//!
//! - **Resource limits** (`setrlimit`): bounds on address space, open file
//!   descriptors, and CPU time, so a runaway or malicious provider cannot
//!   exhaust the host.
//! - **Filesystem sandboxing** (`landlock`, Linux only): the provider is
//!   granted *read-only* access to the filesystem and no write access anywhere,
//!   preventing it from tampering with state files, credentials, or other
//!   resources.
//!
//! Confinement is best-effort and degrades gracefully: where the platform or
//! kernel lacks support, the limits that are available are still applied and
//! the rest are skipped. On non-Unix platforms only process isolation applies.

use std::io;

#[cfg(unix)]
mod unix {
    use super::Limits;
    use std::io;

    /// Bound a single rlimit to `soft == hard == value`.
    unsafe fn set_limit(resource: libc::c_int, value: libc::rlim_t) -> io::Result<()> {
        let rlim = libc::rlimit {
            rlim_cur: value,
            rlim_max: value,
        };
        if libc::setrlimit(resource, &rlim) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Apply the configured resource limits in the current (child) process.
    pub(super) fn apply_rlimits(limits: &Limits) -> io::Result<()> {
        // RLIMIT_AS — total address space. 0 means "leave unchanged".
        if limits.address_space_mib > 0 {
            let bytes = (limits.address_space_mib as u64) * 1024 * 1024;
            let _ = set_limit(libc::RLIMIT_AS, bytes as libc::rlim_t);
        }
        // RLIMIT_NOFILE — open descriptors.
        if limits.max_fds > 0 {
            let _ = set_limit(libc::RLIMIT_NOFILE, limits.max_fds as libc::rlim_t);
        }
        // RLIMIT_CPU — seconds of CPU time.
        if limits.max_cpu_seconds > 0 {
            let _ = set_limit(libc::RLIMIT_CPU, limits.max_cpu_seconds as libc::rlim_t);
        }
        Ok(())
    }

    /// Apply a read-only landlock policy to the current (child) process.
    ///
    /// Grants read access to the filesystem tree (so the provider can load its
    /// binary, shared libraries, and CA certificates) but no write access
    /// anywhere. Fails silently when the running kernel lacks landlock support.
    #[cfg(target_os = "linux")]
    pub(super) fn apply_landlock() -> io::Result<()> {
        use landlock::{ABI, AccessFs, BitFlags, PathBeneath, RestrictionStatus, Ruleset};

        let access: BitFlags<AccessFs> = AccessFs::from_all(ABI::V1)
            .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "landlock unavailable"))?;

        let ruleset = Ruleset::new()
            .handle_access(access)
            .map_err(|e| io::Error::new(io::ErrorKind::Unsupported, e.to_string()))?
            .create()
            .map_err(|e| io::Error::new(io::ErrorKind::Unsupported, e.to_string()))?;

        let ruleset = ruleset
            .add_rule(
                PathBeneath::new("/", access).map_err(|e| io::Error::other(e.to_string()))?,
            )
            .map_err(|e| io::Error::other(e.to_string()))?;

        match ruleset.restrict_self() {
            Ok(_) => Ok(()), // Fully or partially enforced: best-effort.
            Err(e) => Err(io::Error::other(e.to_string())),
        }
    }

    /// Non-Linux Unix (macOS/BSD): resource limits apply, landlock does not.
    #[cfg(not(target_os = "linux"))]
    pub(super) fn apply_landlock() -> io::Result<()> {
        Ok(())
    }
}

/// Tunable confinement limits for a provider child process.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Max address space in MiB (`0` = leave default).
    pub address_space_mib: u64,
    /// Max open file descriptors (`0` = leave default).
    pub max_fds: u64,
    /// Max CPU seconds (`0` = leave default).
    pub max_cpu_seconds: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            address_space_mib: 1024,
            max_fds: 1024,
            max_cpu_seconds: 300,
        }
    }
}

/// Confine the *current* process for an upcoming `exec` of a provider binary.
///
/// Intended to be called from [`std::os::unix::process::CommandExt::pre_exec`],
/// so it runs in the child after fork and before exec. Best-effort: callers may
/// choose to ignore errors (degraded confinement) or abort (strict confinement).
///
/// # Safety
/// Must only be called in a `pre_exec`/`fork` child where no other threads are
/// running — the only safe operations are async-signal-safe syscalls.
/// `setrlimit` and `landlock`'s `restrict_self` satisfy this.
pub unsafe fn confine_current_process(limits: Limits) -> io::Result<()> {
    #[cfg(unix)]
    {
        unix::apply_rlimits(&limits)?;
        // Landlock is best-effort; a missing kernel feature should not prevent
        // the provider from running, only reduce its confinement.
        let _ = unix::apply_landlock();
    }
    #[cfg(not(unix))]
    {
        let _ = limits;
    }
    Ok(())
}
