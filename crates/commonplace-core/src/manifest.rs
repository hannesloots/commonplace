//! `.commonplace/manifest.json`: tool-owned registry of sources and their hashes.

use crate::fsutil;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The registry of ingested sources (`.commonplace/manifest.json`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    /// Source records by source id.
    #[serde(default)]
    pub sources: BTreeMap<String, SourceRecord>,
    /// When `lint` last ran (RFC 3339).
    #[serde(default)]
    pub last_lint: Option<String>,
    /// When `index` last ran (RFC 3339).
    #[serde(default)]
    pub last_index: Option<String>,
}

/// One registered source: where its files are and their hashes at ingest.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceRecord {
    pub(crate) raw: String,
    pub(crate) raw_sha256: String,
    #[serde(default)]
    pub(crate) text: Option<String>,
    #[serde(default)]
    pub(crate) text_sha256: Option<String>,
    #[serde(default)]
    pub(crate) note: Option<String>,
    pub(crate) ingested: String,
    #[serde(default)]
    pub(crate) converter: Option<String>,
    #[serde(default)]
    pub(crate) origin: Option<String>,
}

/// Where the manifest of the commonplace at `root` lives.
pub(crate) fn manifest_path(root: &Path) -> PathBuf {
    crate::vault::tool_dir(root).join("manifest.json")
}

impl Manifest {
    /// Load the manifest of the commonplace at `root` (empty when absent).
    ///
    /// # Errors
    ///
    /// Returns an `io` or `parse` error when a file cannot be read, written or parsed.
    pub fn load(root: &Path) -> Result<Self> {
        let p = manifest_path(root);
        if !p.exists() {
            return Ok(Self::default());
        }
        let s = std::fs::read_to_string(&p).map_err(|e| Error::io("reading", &p, e))?;
        serde_json::from_str(&s).map_err(|e| Error::parse(&p, e))
    }
    /// Write the manifest back to `root`.
    ///
    /// # Errors
    ///
    /// Returns an `io` or `parse` error when a file cannot be read, written or parsed.
    pub fn save(&self, root: &Path) -> Result<()> {
        let s = serde_json::to_string_pretty(self)?;
        fsutil::write_atomic(&manifest_path(root), s.as_bytes())
    }
    pub(crate) fn find_by_hash(&self, sha: &str) -> Option<(&String, &SourceRecord)> {
        self.sources.iter().find(|(_, r)| r.raw_sha256 == sha)
    }
}
