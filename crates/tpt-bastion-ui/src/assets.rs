// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The embedded single-page dashboard. Inlined so the binary ships with no
//! external assets and no third-party CDN (avoiding supply-chain exposure).

pub const INDEX_HTML: &str = include_str!("assets/index.html");
