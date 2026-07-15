// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Integration tests for the Bastion native state layer (Phase 4).
//!
//! Exercises the public API end-to-end: backends, encryption, locking, history,
//! drift detection, and reconciliation through the engine executor.

use bastion_engine::provider::AppliedResource;
use bastion_engine::{Executor, MockProvider, NoopHandler};
use bastion_ir::{Document, Resource, Value};
use bastion_state::backend::StateBackend;
use bastion_state::crypto::DerivedKey;
use bastion_state::drift::detect_drift;
use bastion_state::file::FileStateBackend;
use bastion_state::history::StateHistory;
use bastion_state::reconcile::detect_and_reconcile;
use bastion_state::state::State;
use std::collections::BTreeMap;
use std::time::Duration;

fn applied(id: &str, type_name: &str, attrs: BTreeMap<String, Value>) -> AppliedResource {
    AppliedResource {
        id: id.to_string(),
        type_name: type_name.to_string(),
        attributes: attrs,
    }
}

fn doc_with(name: &str, bucket: &str) -> Document {
    let mut attrs = BTreeMap::new();
    attrs.insert("bucket".to_string(), Value::String(bucket.to_string()));
    Document {
        ir_version: bastion_ir::IR_VERSION,
        config: None,
        variables: vec![],
        resources: vec![Resource {
            id: format!("aws_s3_bucket.{name}"),
            type_name: "aws_s3_bucket".to_string(),
            name: name.to_string(),
            attributes: attrs,
        }],
        modules: vec![],
        outputs: vec![],
    }
}

fn state_with(id: &str, bucket: &str) -> State {
    let mut attrs = BTreeMap::new();
    attrs.insert("bucket".to_string(), Value::String(bucket.to_string()));
    State::from_applied(
        "ln_test".to_string(),
        vec![applied(id, "aws_s3_bucket", attrs)],
    )
}

#[test]
fn memory_backend_round_trips_and_versions() {
    let backend = bastion_state::MemoryBackend::new();
    assert!(backend.load_state().unwrap().resources.is_empty());

    let mut s = State::new();
    s.resources.push(applied(
        "aws_s3_bucket.b",
        "aws_s3_bucket",
        std::iter::once(("bucket".to_string(), Value::String("x".into()))).collect(),
    ));
    backend.save_state(&s).unwrap();

    let loaded = backend.load_state().unwrap();
    assert_eq!(loaded.resources.len(), 1);
    assert_eq!(loaded.resources[0].id, "aws_s3_bucket.b");

    // History records the revision.
    let history = backend.read_history().unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history.latest().unwrap().summary.created, 1);

    // A no-op re-save is idempotent (same fingerprint, no new revision).
    backend.save_state(&loaded).unwrap();
    assert_eq!(backend.read_history().unwrap().len(), 1);

    // A changed save appends a revision.
    let mut s2 = loaded;
    s2.resources[0]
        .attributes
        .insert("acl".into(), Value::String("private".into()));
    backend.save_state(&s2).unwrap();
    assert_eq!(backend.read_history().unwrap().len(), 2);
}

#[test]
fn memory_lock_is_mutually_exclusive() {
    let backend = bastion_state::MemoryBackend::new();
    let ttl = Duration::from_secs(60);
    let g1 = backend.lock("alice", ttl).unwrap();
    // A second holder is rejected while the lock is live.
    assert!(matches!(
        backend.lock("bob", ttl),
        Err(bastion_state::StateError::LockHeld { .. })
    ));
    backend.unlock(&g1).unwrap();
    // After release, a new lock can be taken.
    let _g2 = backend.lock("bob", ttl).unwrap();
}

#[test]
fn file_backend_plain_round_trip() {
    let dir = std::env::temp_dir().join(format!("bastion-state-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("plain.state");
    let _ = std::fs::remove_file(&path);

    let backend = FileStateBackend::plain(&path);
    let mut s = State::new();
    s.resources.push(applied(
        "aws_s3_bucket.b",
        "aws_s3_bucket",
        std::iter::once(("bucket".to_string(), Value::String("y".into()))).collect(),
    ));
    backend.save_state(&s).unwrap();
    assert!(path.exists());
    assert!(dir.join("plain.state.history").exists());

    let loaded = backend.load_state().unwrap();
    assert_eq!(loaded.resources[0].id, "aws_s3_bucket.b");

    // Envelope is plaintext JSON.
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("bastion-state-plain-v1"));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(dir.join("plain.state.history"));
}

#[test]
fn file_backend_encryption_round_trip_and_wrong_key_fails() {
    let dir = std::env::temp_dir().join(format!("bastion-state-enc-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("enc.state");
    let _ = std::fs::remove_file(&path);

    let mut s = State::new();
    s.resources.push(applied(
        "aws_s3_bucket.b",
        "aws_s3_bucket",
        std::iter::once(("bucket".to_string(), Value::String("secret".into()))).collect(),
    ));

    // Encrypt with a passphrase.
    let enc = FileStateBackend::encrypted(&path, "correct horse");
    enc.save_state(&s).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("bastion-state-enc-v1"));
    assert!(!text.contains("secret")); // ciphertext, not plaintext

    // Load with the right passphrase.
    let loaded = enc.load_state().unwrap();
    assert_eq!(
        loaded.resources[0].attributes.get("bucket"),
        Some(&Value::String("secret".into()))
    );

    // Load with the wrong passphrase fails decryption.
    let wrong = FileStateBackend::encrypted(&path, "wrong key");
    assert!(matches!(
        wrong.load_state(),
        Err(bastion_state::StateError::Decrypt)
    ));

    // Load without a passphrase is rejected.
    let nokey = FileStateBackend::plain(&path);
    assert!(matches!(
        nokey.load_state(),
        Err(bastion_state::StateError::MissingPassphrase)
    ));

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(dir.join("enc.state.history"));
}

#[test]
fn crypto_key_derivation_is_deterministic_per_salt() {
    let k1 = DerivedKey::from_passphrase("pw").unwrap();
    let salt = k1.salt().to_vec();
    let k2 = DerivedKey::from_passphrase_and_salt("pw", &salt).unwrap();
    let (nonce, ct1) = k1.encrypt(b"hello").unwrap();
    let pt = k2.decrypt(&nonce, &ct1).unwrap();
    assert_eq!(pt, b"hello");
}

#[test]
fn drift_detects_missing_changed_and_extra() {
    let doc = doc_with("b", "want");

    // Missing: state has no resource.
    let empty = State::new();
    let report = detect_drift(&doc, &empty);
    assert!(!report.is_clean());
    assert!(report
        .items
        .iter()
        .any(|i| matches!(i.kind, bastion_state::drift::DriftKind::MissingResource)));

    // Changed: state has the resource but wrong bucket value.
    let changed = state_with("aws_s3_bucket.b", "got");
    let report = detect_drift(&doc, &changed);
    assert!(report.items.iter().any(|i| matches!(
        i.kind,
        bastion_state::drift::DriftKind::AttributeDrift { .. }
    )));

    // Extra: state has a resource the document does not declare.
    let extra = state_with("aws_s3_bucket.other", "z");
    let report = detect_drift(&doc, &extra);
    assert!(report
        .items
        .iter()
        .any(|i| matches!(i.kind, bastion_state::drift::DriftKind::ExtraResource)));

    // Clean: state matches the document.
    let clean = state_with("aws_s3_bucket.b", "want");
    assert!(detect_drift(&doc, &clean).is_clean());
}

#[test]
fn reconcile_produces_applyable_plan() {
    let doc = doc_with("b", "want");

    // Missing resource -> create plan -> apply yields matching state.
    let empty = State::new();
    let (report, plan) = detect_and_reconcile(&doc, &empty);
    assert_eq!(plan.to_create, 1);
    assert!(report
        .items
        .iter()
        .any(|i| matches!(i.kind, bastion_state::drift::DriftKind::MissingResource)));

    let exec = Executor::new(MockProvider, Box::new(NoopHandler));
    let empty_eng: bastion_engine::State = (&empty).into();
    let new_state: State = exec.apply(&doc, &plan, &empty_eng).unwrap().into();
    assert!(detect_drift(&doc, &new_state).is_clean());

    // Attribute drift -> update plan -> apply resolves the drift.
    let drifted = state_with("aws_s3_bucket.b", "got");
    let (_, plan) = detect_and_reconcile(&doc, &drifted);
    assert_eq!(plan.to_update, 1);
    let drifted_eng: bastion_engine::State = (&drifted).into();
    let fixed: State = exec.apply(&doc, &plan, &drifted_eng).unwrap().into();
    assert!(detect_drift(&doc, &fixed).is_clean());
}

#[test]
fn reconcile_delete_orphaned_resource() {
    let doc = doc_with("b", "want");
    let orphan = state_with("aws_s3_bucket.ghost", "z");
    let (report, plan) = detect_and_reconcile(&doc, &orphan);
    assert_eq!(plan.to_delete, 1);
    assert!(report
        .items
        .iter()
        .any(|i| matches!(i.kind, bastion_state::drift::DriftKind::ExtraResource)));

    let exec = Executor::new(MockProvider, Box::new(NoopHandler));
    let orphan_eng: bastion_engine::State = (&orphan).into();
    let fixed: State = exec.apply(&doc, &plan, &orphan_eng).unwrap().into();
    assert!(detect_drift(&doc, &fixed).is_clean());
}

#[test]
fn history_summary_counts_changes() {
    let mut h = StateHistory::new();
    let mut s1 = State::new();
    s1.resources.push(applied(
        "a",
        "t",
        std::iter::once(("k".to_string(), Value::String("v".into()))).collect(),
    ));
    h.record(&s1, None).unwrap();
    let mut s2 = s1.clone();
    s2.resources.push(applied(
        "b",
        "t",
        std::iter::once(("k".to_string(), Value::String("v".into()))).collect(),
    ));
    h.record(&s2, Some(&s1)).unwrap();
    let latest = h.latest().unwrap();
    // First rev: 1 created. Second rev: +1 created (now 2 total, 1 updated vs prev).
    assert_eq!(latest.summary.total, 2);
    assert_eq!(latest.summary.created, 1);
}
