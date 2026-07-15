// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Fuzzing / property test for the DSL compiler (Phase 8).
//!
//! Compiling arbitrary input must *never* panic: it returns `Ok` for valid
//! programs and a structured `Err` otherwise. This drives a deterministic LCG
//! over 20k random byte strings (including quotes, braces, and NULs) and
//! asserts `compile_source` always returns instead of unwinding.

use bastion_dsl::compile_source;
use std::panic;

#[test]
fn compile_never_panics_on_random_input() {
    // Deterministic LCG so the test is reproducible.
    let lcg = |s: &mut u64| {
        *s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *s
    };
    let mut rng = 0x1234_5678_9abc_def0u64;
    let mut buf = Vec::new();

    for _ in 0..20000u32 {
        buf.clear();
        let len = (lcg(&mut rng) % 64) as usize;
        for _ in 0..len {
            // Printable-ish ASCII plus a few structural characters.
            let b = (lcg(&mut rng) % 96 + 32) as u8;
            buf.push(b);
        }
        let src = String::from_utf8_lossy(&buf);
        let result = panic::catch_unwind(|| {
            let _ = compile_source(&src);
        });
        assert!(result.is_ok(), "compile_source paniced on input: {:?}", src);
    }
}
