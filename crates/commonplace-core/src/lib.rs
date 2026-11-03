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
