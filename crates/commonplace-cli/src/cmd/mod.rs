//! One function per subcommand. Each returns the process exit code.

pub(crate) mod check;
pub(crate) mod find;
pub(crate) mod graph;
pub(crate) mod pages;
pub(crate) mod review;
pub(crate) mod setup;
pub(crate) mod source;

use anyhow::Result;
use commonplace_core::{config, ops, pack, vault::Vault};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The global options every command sees.
pub(crate) struct Ctx {
    /// `--root`, if given.
    pub(crate) root: Option<PathBuf>,
    /// `--json`: print machine-readable output.
    pub(crate) json: bool,
}

impl Ctx {
    /// The commonplace root: `--root` if given, else found by walking up from the current directory.
    pub(crate) fn find_root(&self) -> Result<PathBuf> {
        match &self.root {
            Some(r) => Ok(r.canonicalize().unwrap_or_else(|_| r.clone())),
            None => config::find_root(&std::env::current_dir()?),
        }
    }

    /// Open the vault at [`Ctx::find_root`].
    pub(crate) fn open(&self) -> Result<Vault> {
        Vault::open(&self.find_root()?)
    }
}

pub(crate) fn load_packs(v: &Vault) -> Result<Vec<pack::Pack>> {
    pack::load_packs(&v.packs_dir(), &v.config.packs, &v.config.jurisdiction)
}

pub(crate) fn skill_dir() -> Option<PathBuf> {
    std::env::var_os("COMMONPLACE_SKILL_DIR")
        .or_else(|| std::env::var_os("BRAIN_SKILL_DIR"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("CLAUDE_SKILL_DIR").map(PathBuf::from))
        .filter(|p| p.is_dir())
}

pub(crate) fn print_json<T: serde::Serialize>(v: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

/// Format names as a comma-separated list of wikilinks.
pub(crate) fn wikilinks<'a>(names: impl IntoIterator<Item = &'a String>) -> String {
    names
        .into_iter()
        .map(|n| format!("[[{n}]]"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The set of paths a command is restricted to: the given paths plus, with
/// `changed`, the files changed since the last commit. `None` means "all".
pub(crate) fn paths_set(
    paths: &[String],
    changed: bool,
    root: &Path,
) -> Result<Option<HashSet<String>>> {
    let mut set: HashSet<String> = paths
        .iter()
        .map(|p| p.trim_start_matches("./").to_string())
        .collect();
    if changed {
        set.extend(ops::changed_files(root)?);
        if set.is_empty() {
            // nothing changed: report nothing rather than everything
            return Ok(Some(HashSet::new()));
        }
    }
    Ok(if set.is_empty() { None } else { Some(set) })
}
