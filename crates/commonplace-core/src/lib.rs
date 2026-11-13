//! The deterministic half of an LLM-maintained research wiki (second brain).
//!
//! The LLM writes and judges; this crate registers, hashes, links, verifies,
//! indexes and searches.

#![warn(missing_docs, missing_debug_implementations)]

pub mod analysis;
pub mod cite;
pub mod config;
mod error;
pub mod frontmatter;
pub mod fsutil;
pub mod graph;
pub mod index;
pub mod lint;
pub mod manifest;
mod markdown;
pub mod ops;
pub mod pack;
pub mod query;
pub mod scaffold;
pub mod search;
pub mod skill;
pub mod sources;
pub mod vault;

pub use error::{Error, Result};

/// The crate version, reported by `commonplace --version` and `doctor`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Compile a regex from a pattern literal written in this crate.
///
/// Every caller passes a string literal, so a failure is a bug in the literal
/// and is caught the first time the pattern is used by the test suite.
#[expect(
    clippy::expect_used,
    reason = "patterns are compile-time literals; an invalid one is a programming error"
)]
pub(crate) fn re(pattern: &str) -> regex::Regex {
    regex::Regex::new(pattern).expect("built-in regex literal is valid")
}

/// A count (pages, links, terms, tags) as `f64` for scoring.
///
/// Counts in this crate are far below 2^52, where every integer is exact in `f64`.
#[expect(
    clippy::cast_precision_loss,
    reason = "counts are far below 2^52, where f64 represents every integer exactly"
)]
pub(crate) fn count_f64(n: usize) -> f64 {
    n as f64
}
