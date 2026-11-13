<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# tpt-bastion-state

Native state management for [TPT Bastion](https://bastion.tpt.dev). Implements
Phase 4 of the Bastion roadmap: encrypted state files, append-only version
history, cooperative locking, plan-time drift detection against recorded state, and
reconcile plans. Drift is not checked against live infrastructure yet.

See the [crate docs](../../crates/tpt-bastion-state/src/lib.rs) for the module
layout and usage. The `StateBackend` trait is the pluggable interface that
external stores (e.g. TPT Keystone) implement.
