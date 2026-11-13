// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Linux-only checks that the provider sandbox really denies writes.
//!
//! The test skips (passes with a note) when the running kernel cannot enforce
//! landlock, because the policy then cannot be observed.

#![cfg(target_os = "linux")]

use std::os::unix::process::CommandExt;
use std::process::Command;

use tpt_bastion_providers::sandbox::{confine_current_process, Limits};

/// Spawn `/bin/sh -c script` with the sandbox applied before exec.
fn run_confined(script: &str, require_landlock: bool) -> std::process::ExitStatus {
    let limits = Limits {
        require_landlock,
        ..Limits::default()
    };
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(script);
    // SAFETY: the closure only calls `confine_current_process` in the child.
    unsafe {
        cmd.pre_exec(move || confine_current_process(limits));
    }
    cmd.status().expect("spawning confined shell")
}

/// Whether this kernel can enforce the landlock policy at all.
fn landlock_enforceable() -> bool {
    run_confined("exit 0", true).success()
}

#[test]
fn confined_provider_can_read_but_not_write() {
    if !landlock_enforceable() {
        eprintln!("skipping: landlock is not enforced on this kernel");
        return;
    }

    let dir = std::env::temp_dir().join(format!("bastion-sandbox-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("creating scratch dir");
    let target = dir.join("written.txt");

    // Reading succeeds; the redirect into the scratch directory must be denied.
    let script = format!(
        "cat /etc/hostname >/dev/null && echo x > '{}'",
        target.display()
    );
    let status = run_confined(&script, true);

    assert!(!status.success(), "confined write unexpectedly succeeded");
    assert!(!target.exists(), "confined process created a file");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn confined_provider_can_still_read_files() {
    if !landlock_enforceable() {
        eprintln!("skipping: landlock is not enforced on this kernel");
        return;
    }
    let status = run_confined("cat /etc/hostname >/dev/null", true);
    assert!(status.success(), "confined read was denied");
}
