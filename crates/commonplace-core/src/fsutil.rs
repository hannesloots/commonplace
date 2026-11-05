//! Small filesystem helpers: atomic writes, hashing, slugs, path display.

use crate::{Error, Result};
use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveTime, TimeZone};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::LazyLock;

/// Write `data` to `path` via a temporary file and rename, creating parent folders.
///
/// # Errors
///
/// Returns an `io` error when a folder or file cannot be created, written or renamed.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io("creating", parent, e))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp~",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    std::fs::write(&tmp, data).map_err(|e| Error::io("writing", &tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| Error::io("renaming to", path, e))?;
    Ok(())
}

pub(crate) fn write_if_absent(path: &Path, data: &str) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    write_atomic(path, data.as_bytes())?;
    Ok(true)
}

pub(crate) fn sha256_file(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path).map_err(|e| Error::io("opening", path, e))?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(hex::encode(h.finalize()))
}

pub(crate) fn sha256_str(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

pub(crate) fn rel(root: &Path, path: &Path) -> String {
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
pub(crate) fn safe_title(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '#' | '^' | '[' | ']' | '|' | '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' => ' ',
            c => c,
        })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Environment variable that pins the clock for repeatable runs (tests, evals):
/// an RFC 3339 timestamp, or a `YYYY-MM-DD` date meaning local midnight.
/// An unparseable value is ignored and the real clock is used, because the
/// clock is read deep inside operations that have no good way to report it.
const NOW_ENV: &str = "COMMONPLACE_NOW";

static PINNED_NOW: LazyLock<Option<DateTime<FixedOffset>>> = LazyLock::new(|| {
    std::env::var(NOW_ENV)
        .ok()
        .and_then(|v| parse_pinned_now(&v))
});

fn parse_pinned_now(value: &str) -> Option<DateTime<FixedOffset>> {
    let value = value.trim();
    if let Ok(t) = DateTime::parse_from_rfc3339(value) {
        return Some(t);
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    Local
        .from_local_datetime(&date.and_time(NaiveTime::MIN))
        .earliest()
        .map(|t| t.fixed_offset())
}

fn now() -> DateTime<FixedOffset> {
    PINNED_NOW.unwrap_or_else(|| Local::now().fixed_offset())
}

/// Today's date as `YYYY-MM-DD` (honours `COMMONPLACE_NOW`).
pub fn today() -> String {
    now().format("%Y-%m-%d").to_string()
}

/// The current time as RFC 3339 with seconds (honours `COMMONPLACE_NOW`).
pub fn now_iso() -> String {
    now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub(crate) fn is_markdown(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

pub(crate) fn stem(p: &Path) -> String {
    p.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::parse_pinned_now;

    #[test]
    fn pinned_now_accepts_rfc3339_and_plain_dates() {
        let t = parse_pinned_now("2026-01-02T03:04:05+02:00").unwrap();
        assert_eq!(t.to_rfc3339(), "2026-01-02T03:04:05+02:00");
        let d = parse_pinned_now(" 2026-01-02 ").unwrap();
        assert_eq!(d.format("%Y-%m-%d %H:%M").to_string(), "2026-01-02 00:00");
        assert!(parse_pinned_now("next tuesday").is_none());
    }
}
