//! Small filesystem helpers: atomic writes, hashing, slugs, path display.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp~",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    std::fs::write(&tmp, data).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("renaming {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

pub fn write_if_absent(path: &Path, data: &str) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    write_atomic(path, data.as_bytes())?;
    Ok(true)
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(hex::encode(h.finalize()))
}

pub fn sha256_str(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

pub fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Lowercase kebab-case slug, ASCII only, trimmed to `max` chars.
pub fn slug(s: &str, max: usize) -> String {
    use unicode_normalization::UnicodeNormalization;
    let folded: String = s.nfkd().filter(char::is_ascii).collect();
    let mut out = String::new();
    let mut last_dash = true;
    for c in folded.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let mut out = out.trim_matches('-').to_string();
    if out.len() > max {
        out.truncate(max);
        out = out.trim_end_matches('-').to_string();
    }
    out
}

/// Title-case a free-text title for file names: remove characters that break
/// wikilinks and paths, collapse whitespace.
pub fn safe_title(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '#' | '^' | '[' | ']' | '|' | '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' => ' ',
            c => c,
        })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn now_iso() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn is_markdown(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

pub fn stem(p: &Path) -> String {
    p.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}
