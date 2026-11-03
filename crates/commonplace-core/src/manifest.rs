//! `.commonplace/manifest.json`: tool-owned registry of sources and their hashes.

use crate::fsutil;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    #[serde(default)]
    pub sources: BTreeMap<String, SourceRecord>,
    #[serde(default)]
    pub last_lint: Option<String>,
    #[serde(default)]
    pub last_index: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceRecord {
    pub raw: String,
    pub raw_sha256: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub text_sha256: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    pub ingested: String,
    #[serde(default)]
    pub converter: Option<String>,
    #[serde(default)]
    pub origin: Option<String>,
}

impl Manifest {
    pub fn path(root: &Path) -> std::path::PathBuf {
        crate::vault::tool_dir(root).join("manifest.json")
    }
    pub fn load(root: &Path) -> Result<Self> {
        let p = Self::path(root);
        if !p.exists() {
            return Ok(Self::default());
        }
        let s = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
        Ok(serde_json::from_str(&s).with_context(|| format!("parsing {}", p.display()))?)
    }
    pub fn save(&self, root: &Path) -> Result<()> {
        let s = serde_json::to_string_pretty(self)?;
        fsutil::write_atomic(&Self::path(root), s.as_bytes())
    }
    pub fn find_by_hash(&self, sha: &str) -> Option<(&String, &SourceRecord)> {
        self.sources.iter().find(|(_, r)| r.raw_sha256 == sha)
    }
}
