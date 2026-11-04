//! commonplace-core: the deterministic half of an LLM-maintained research wiki /
//! second brain. The LLM writes and judges; this crate registers, hashes,
//! links, verifies, indexes and searches.

pub mod analysis;
pub mod cite;
pub mod config;
pub mod frontmatter;
pub mod fsutil;
pub mod graph;
pub mod index;
pub mod lint;
pub mod manifest;
pub mod markdown;
pub mod ops;
pub mod pack;
pub mod query;
pub mod scaffold;
pub mod search;
pub mod sources;
pub mod vault;

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
