//! `commonplace.toml` and root discovery.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub(crate) const CONFIG_FILE: &str = "commonplace.toml";
/// Name used before 0.5.0; still read and, if present, still written.
pub(crate) const LEGACY_CONFIG_FILE: &str = "brain.toml";

/// The config file of a root: the current name, else the legacy one if it exists.
pub fn config_path(root: &Path) -> PathBuf {
    let cur = root.join(CONFIG_FILE);
    if cur.is_file() {
        return cur;
    }
    let old = root.join(LEGACY_CONFIG_FILE);
    if old.is_file() {
        return old;
    }
    cur
}
pub(crate) const SCHEMA_VERSION: u32 = 1;

/// Settings from `commonplace.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub(crate) schema_version: u32,
    pub(crate) name: String,
    pub(crate) purpose: String,
    /// Enabled packs.
    pub packs: Vec<String>,
    /// Jurisdiction profile for the legal pack: za, au, en, or a code you add
    /// under .commonplace/packs/legal/jurisdictions/. Empty = generic patterns.
    pub jurisdiction: String,
    pub(crate) index: IndexConfig,
    pub(crate) lint: LintConfig,
    pub(crate) converters: ConvertersConfig,
    pub(crate) privacy: PrivacyConfig,
    /// Persona review settings.
    pub review: ReviewConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct IndexConfig {
    /// A folder whose page count passes this gets its own hub instead of inline lines in index.md.
    pub(crate) hub_threshold: usize,
    /// Hard ceiling for index.md lines before the tool refuses to inline more.
    pub(crate) max_index_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LintConfig {
    /// Pages whose `updated` is older than this many days get a stale warning (0 disables).
    pub(crate) stale_days: u32,
    /// Words that must no longer appear in non-superseded pages (old product names, retired terms).
    pub(crate) retired_terms: Vec<String>,
    /// Rule ids to silence.
    pub(crate) disabled_rules: Vec<String>,
    /// Pages above this many words get an oversize warning.
    pub(crate) max_words: usize,
    /// Minimum and maximum words in a citation quote.
    pub(crate) quote_min_words: usize,
    pub(crate) quote_max_words: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ConvertersConfig {
    /// Preference order of external converters for PDF/DOCX: docling, marker, markitdown, pandoc, pdftotext.
    pub(crate) prefer: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PrivacyConfig {
    /// confidentiality levels excluded from `commonplace export` unless --include is given.
    pub(crate) exclude_from_export: Vec<String>,
}

/// Persona review settings (`[review]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReviewConfig {
    /// Context lines shown around each cited span in a review packet.
    pub context_lines: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            name: String::new(),
            purpose: String::new(),
            packs: vec![],
            jurisdiction: String::new(),
            index: IndexConfig::default(),
            lint: LintConfig::default(),
            converters: ConvertersConfig::default(),
            privacy: PrivacyConfig::default(),
            review: ReviewConfig::default(),
        }
    }
}
impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            hub_threshold: 60,
            max_index_lines: 200,
        }
    }
}
impl Default for LintConfig {
    fn default() -> Self {
        Self {
            stale_days: 180,
            retired_terms: vec![],
            disabled_rules: vec![],
            max_words: 2500,
            quote_min_words: 3,
            quote_max_words: 80,
        }
    }
}
impl Default for ConvertersConfig {
    fn default() -> Self {
        Self {
            prefer: vec![
                "docling".into(),
                "marker".into(),
                "markitdown".into(),
                "pandoc".into(),
                "pdftotext".into(),
            ],
        }
    }
}
impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            exclude_from_export: vec!["privileged".into(), "confidential".into()],
        }
    }
}
impl Default for ReviewConfig {
    fn default() -> Self {
        Self { context_lines: 3 }
    }
}

impl Config {
    /// Load the config of the commonplace at `root` (defaults when absent).
    ///
    /// # Errors
    ///
    /// Returns an `io` or `parse` error when a file cannot be read, written or parsed.
    pub fn load(root: &Path) -> Result<Self> {
        let p = config_path(root);
        let s = std::fs::read_to_string(&p).map_err(|e| Error::io("reading", &p, e))?;
        let mut c: Config = toml::from_str(&s).map_err(|e| Error::parse(&p, e))?;
        // compatibility: the pre-0.3 `sa-legal` pack is `legal` with jurisdiction za
        let mut packs = vec![];
        for p in &c.packs {
            let (name, jur) = crate::scaffold::normalise_pack(p);
            if let Some(j) = jur
                && c.jurisdiction.is_empty()
            {
                c.jurisdiction = j.to_string();
            }
            if !packs.contains(&name) {
                packs.push(name);
            }
        }
        c.packs = packs;
        Ok(c)
    }

    /// Write the config back to `root`.
    ///
    /// # Errors
    ///
    /// Returns an `io` or `parse` error when a file cannot be read, written or parsed.
    pub fn save(&self, root: &Path) -> Result<()> {
        let s = toml::to_string_pretty(self)?;
        crate::fsutil::write_atomic(&config_path(root), s.as_bytes())
    }
}

/// Walk up from `start` to find the directory containing commonplace.toml.
///
/// # Errors
///
/// `no_root` when no `commonplace.toml` is found.
pub fn find_root(start: &Path) -> Result<PathBuf> {
    let mut cur = Some(start.to_path_buf());
    while let Some(dir) = cur {
        if dir.join(CONFIG_FILE).is_file() || dir.join(LEGACY_CONFIG_FILE).is_file() {
            return Ok(dir);
        }
        cur = dir.parent().map(std::path::Path::to_path_buf);
    }
    Err(Error::no_root(start))
}
