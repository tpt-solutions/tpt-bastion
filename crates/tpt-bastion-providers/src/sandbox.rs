// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Process confinement for provider plugins.
//!
//! Providers are untrusted by default: they run as separate OS processes and
//! speak only the ABI. This module tightens that boundary by applying, inside
//! the child process *before* it execs the provider binary:
//!
//! - **Resource limits** (`setrlimit`, Unix): bounds on address space, open
//!   file descriptors, and CPU time, so a runaway or malicious provider cannot
//!   exhaust the host. A failed `setrlimit` is an error and the spawn fails.
//! - **Filesystem sandboxing** (`landlock`, Linux only): every filesystem access
//!   right is handled, but only *read* rights are granted (under `/`). The
//!   provider can load its binary, libraries, and CA certificates, and cannot
//!   write, create, or delete anything, which protects state files, credentials,
//!   and other resources.
//!
//! # Platform limits
//!
//! - **Linux**: resource limits and read-only landlock. Landlock needs kernel
//!   5.13+. On older kernels the limits still apply and landlock is reported as
//!   not enforced; set [`Limits::require_landlock`] to make that an error.
//! - **macOS / BSD**: resource limits only. There is no filesystem confinement.
//! - **Windows**: no confinement from this module. Providers are still isolated
//!   as separate processes, but they have full user privileges.

use std::io;

#[cfg(unix)]
mod unix {
    use super::Limits;
    use std::io;

    /// Set one rlimit to `soft == hard == value`. Expands in place so the
    /// resource argument takes whatever integer type this libc expects.
    macro_rules! set_rlimit {
        ($resource:expr, $value:expr) => {{
            let value: libc::rlim_t = $value;
            let rlim = libc::rlimit {
                rlim_cur: value,
                rlim_max: value,
            };
            // SAFETY: `rlim` is a valid, initialized rlimit for the call.
            if unsafe { libc::setrlimit($resource, &rlim) } != 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }};
    }

    /// Apply the configured resource limits in the current (child) process.
    /// A value of `0` leaves that limit unchanged.
    pub(super) fn apply_rlimits(limits: &Limits) -> io::Result<()> {
        if limits.address_space_mib > 0 {
            let bytes = limits.address_space_mib.saturating_mul(1024 * 1024);
            set_rlimit!(libc::RLIMIT_AS, bytes as libc::rlim_t)?;
        }
        if limits.max_fds > 0 {
            set_rlimit!(libc::RLIMIT_NOFILE, limits.max_fds as libc::rlim_t)?;
        }
        if limits.max_cpu_seconds > 0 {
            set_rlimit!(libc::RLIMIT_CPU, limits.max_cpu_seconds as libc::rlim_t)?;
        }
        Ok(())
    }

    /// Apply a read-only landlock policy to the current (child) process.
    ///
    /// All filesystem access rights are handled, and only read rights are
    /// allowed beneath `/`, so any write is denied. Returns an error if the
    /// ruleset cannot be built or enforced. If the kernel does not support
    /// landlock, the ruleset reports `NotEnforced`; that is an error only when
    /// `require` is set.
    #[cfg(target_os = "linux")]
    pub(super) fn apply_landlock(require: bool) -> io::Result<()> {
        use landlock::{
            AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetStatus,
            ABI,
        };

        let handled = AccessFs::from_read(ABI::V1) | AccessFs::from_write(ABI::V1);
        let read_only = AccessFs::from_read(ABI::V1);

        let root = PathFd::new("/").map_err(io::Error::other)?;
        let status = Ruleset::default()
            .handle_access(handled)
            .map_err(io::Error::other)?
            .create()
            .map_err(io::Error::other)?
            .add_rule(PathBeneath::new(root, read_only))
            .map_err(io::Error::other)?
            .restrict_self()
            .map_err(io::Error::other)?;

        match status.ruleset {
            RulesetStatus::FullyEnforced | RulesetStatus::PartiallyEnforced => Ok(()),
            RulesetStatus::NotEnforced if require => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "landlock is not supported by this kernel",
            )),
            RulesetStatus::NotEnforced => Ok(()),
        }
    }

    /// Non-Linux Unix (macOS/BSD): resource limits apply, landlock does not.
    #[cfg(not(target_os = "linux"))]
    pub(super) fn apply_landlock(require: bool) -> io::Result<()> {
        if require {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "landlock is only available on Linux",
            ));
        }
        Ok(())
    }
}

/// Confinement limits for a provider child process.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Max address space in MiB (`0` = leave unchanged).
    pub address_space_mib: u64,
    /// Max open file descriptors (`0` = leave unchanged).
    pub max_fds: u64,
    /// Max CPU seconds (`0` = leave unchanged).
    pub max_cpu_seconds: u64,
    /// Fail the spawn when landlock cannot be enforced, instead of running
    /// with resource limits alone.
    pub require_landlock: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            address_space_mib: 1024,
            max_fds: 1024,
            max_cpu_seconds: 300,
            require_landlock: false,
        }
    }
}

/// Confine the *current* process for an upcoming `exec` of a provider binary.
///
/// Intended to be called from [`std::os::unix::process::CommandExt::pre_exec`],
/// so it runs in the child after fork and before exec. Any failure is returned
/// so the caller can abort the spawn. Confinement is never silently skipped,
/// except for a landlock-less kernel when [`Limits::require_landlock`] is false.
///
/// # Safety
/// Must only be called in a `pre_exec`/`fork` child where no other threads are
/// running. `setrlimit` and landlock's `restrict_self` are the only syscalls
/// here; error construction may allocate, which is acceptable in practice for
/// a single-threaded child.
pub unsafe fn confine_current_process(limits: Limits) -> io::Result<()> {
    #[cfg(unix)]
    {
        unix::apply_rlimits(&limits)?;
        unix::apply_landlock(limits.require_landlock)?;
    }
    #[cfg(not(unix))]
    {
        let _ = limits;
    }
    Ok(())
}
