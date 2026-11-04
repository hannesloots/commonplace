//! Load every page in a commonplace and resolve wikilinks.

use crate::config::Config;
use crate::frontmatter::{self, Document};
use crate::fsutil;
use crate::markdown::{self, WikiLink};
use crate::{Error, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Minimum Jaro-Winkler similarity for a title to be offered as a "did you
/// mean" suggestion; below this, matches are mostly noise.
const SUGGESTION_CUTOFF: f64 = 0.8;

/// Folders (relative to root) that are not wiki pages.
/// Name of the tool-owned folder (manifest, templates, packs, reviews).
pub(crate) const TOOL_DIR: &str = ".commonplace";
/// Name used before 0.5.0; a root that has it keeps using it.
pub(crate) const LEGACY_TOOL_DIR: &str = ".brain";

/// The tool-owned folder of a root: `.commonplace`, or `.brain` if only that exists.
pub fn tool_dir(root: &Path) -> PathBuf {
    let cur = root.join(TOOL_DIR);
    if cur.is_dir() {
        return cur;
    }
    let old = root.join(LEGACY_TOOL_DIR);
    if old.is_dir() {
        return old;
    }
    cur
}

pub(crate) const SKIP_DIRS: &[&str] = &[
    ".commonplace",
    ".brain",
    ".git",
    ".obsidian",
    ".trash",
    "sources/raw",
    "sources/text",
    "node_modules",
];

/// Root files that are instructions, not wiki pages.
pub(crate) const SKIP_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md", "CLAUDE.local.md", "README.md"];

/// A markdown page of the commonplace.
#[derive(Debug, Clone)]
pub struct Page {
    pub(crate) path: PathBuf,
    /// Vault-relative path with forward slashes, the same on every OS.
    pub rel: String,
    /// File name without `.md`; unique within the commonplace.
    pub stem: String,
    pub(crate) doc: Document,
    pub(crate) ptype: String,
    /// `title` from frontmatter, else the first heading, else the stem.
    pub title: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) body_links: Vec<WikiLink>,
    /// (field name, target) for every `[[wikilink]]` in any frontmatter field; packs
    /// define their own relation fields, so no field list is assumed.
    pub(crate) fm_links: Vec<(String, String)>,
    pub(crate) parse_error: Option<String>,
    pub(crate) mtime: Option<std::time::SystemTime>,
}

impl Page {
    pub(crate) fn fm(&self) -> &serde_yaml::Mapping {
        &self.doc.fm
    }
    /// A frontmatter field as plain text.
    pub fn get(&self, key: &str) -> Option<String> {
        frontmatter::get_str(&self.doc.fm, key)
    }
    pub(crate) fn list(&self, key: &str) -> Vec<String> {
        frontmatter::get_list(&self.doc.fm, key)
    }
    pub(crate) fn folder(&self) -> String {
        match self.rel.rfind('/') {
            Some(i) => self.rel[..i].to_string(),
            None => String::new(),
        }
    }
    pub(crate) fn top_folder(&self) -> String {
        self.rel.split('/').next().unwrap_or("").to_string()
    }
    pub(crate) fn all_link_targets(&self) -> Vec<String> {
        let mut v: Vec<String> = self.body_links.iter().map(|l| l.target.clone()).collect();
        v.extend(self.fm_links.iter().map(|(_, t)| t.clone()));
        v
    }
    /// Whether the page lives under `archive/`.
    pub fn is_archived(&self) -> bool {
        self.top_folder() == "archive"
    }
}

/// A loaded commonplace: its config and every page.
pub struct Vault {
    /// The commonplace root.
    pub root: PathBuf,
    /// Settings from `commonplace.toml`.
    pub config: Config,
    /// Every page, in walk order.
    pub pages: Vec<Page>,
    by_stem: HashMap<String, Vec<usize>>,
    by_alias: HashMap<String, Vec<usize>>,
    by_rel: HashMap<String, usize>,
}

impl Vault {
    /// Load the commonplace at `root`.
    ///
    /// # Errors
    ///
    /// Returns an `io` or `parse` error when a file cannot be read, written or parsed.
    pub fn open(root: &Path) -> Result<Self> {
        let config = Config::load(root)?;
        let mut v = Vault {
            root: root.to_path_buf(),
            config,
            pages: vec![],
            by_stem: HashMap::new(),
            by_alias: HashMap::new(),
            by_rel: HashMap::new(),
        };
        v.load_pages();
        Ok(v)
    }

    /// Load every markdown page; a page that fails to parse is kept with a
    /// `parse_error` so lint can report it instead of aborting the load.
    fn load_pages(&mut self) {
        let mut pages = vec![];
        for entry in walk_markdown(&self.root) {
            let path = entry;
            match load_page(&self.root, &path) {
                Ok(p) => pages.push(p),
                Err(e) => pages.push(Page {
                    rel: fsutil::rel(&self.root, &path),
                    stem: fsutil::stem(&path),
                    path,
                    doc: Document::default(),
                    ptype: String::new(),
                    title: String::new(),
                    aliases: vec![],
                    tags: vec![],
                    body_links: vec![],
                    fm_links: vec![],
                    parse_error: Some(format!("{e:#}")),
                    mtime: None,
                }),
            }
        }
        pages.sort_by(|a, b| a.rel.cmp(&b.rel));
        self.pages = pages;
        self.reindex();
    }

    pub(crate) fn reindex(&mut self) {
        self.by_stem.clear();
        self.by_alias.clear();
        self.by_rel.clear();
        for (i, p) in self.pages.iter().enumerate() {
            self.by_stem
                .entry(p.stem.to_lowercase())
                .or_default()
                .push(i);
            self.by_rel.insert(p.rel.to_lowercase(), i);
            for a in &p.aliases {
                self.by_alias.entry(a.to_lowercase()).or_default().push(i);
            }
        }
    }

    /// Resolve a wikilink target to candidate page indexes.
    /// Order of preference: exact relative path, exact stem, alias.
    pub(crate) fn resolve(&self, target: &str) -> Resolution {
        let mut t = target.trim().replace('\\', "/");
        if let Some(s) = t.strip_suffix(".md") {
            t = s.to_string();
        }
        let lower = t.to_lowercase();
        if t.contains('/') {
            if let Some(&i) = self.by_rel.get(&format!("{lower}.md")) {
                return Resolution::Found(i);
            }
            let last = lower.rsplit('/').next().unwrap_or(&lower).to_string();
            if let Some(v) = self.by_stem.get(&last) {
                // prefer a page whose rel ends with the given path
                let matching: Vec<usize> = v
                    .iter()
                    .copied()
                    .filter(|&i| {
                        self.pages[i]
                            .rel
                            .to_lowercase()
                            .ends_with(&format!("{lower}.md"))
                    })
                    .collect();
                if matching.len() == 1 {
                    return Resolution::Found(matching[0]);
                }
                return match v.len() {
                    1 => Resolution::Found(v[0]),
                    _ => Resolution::Ambiguous(v.clone()),
                };
            }
            return Resolution::Missing;
        }
        if let Some(v) = self.by_stem.get(&lower) {
            return match v.len() {
                1 => Resolution::Found(v[0]),
                _ => Resolution::Ambiguous(v.clone()),
            };
        }
        if let Some(v) = self.by_alias.get(&lower) {
            return match v.len() {
                1 => Resolution::Found(v[0]),
                _ => Resolution::Ambiguous(v.clone()),
            };
        }
        Resolution::Missing
    }

    pub(crate) fn resolve_page(&self, target: &str) -> Option<&Page> {
        match self.resolve(target) {
            Resolution::Found(i) => Some(&self.pages[i]),
            _ => None,
        }
    }

    /// Closest existing stems/aliases for a missing target (for `links fix` suggestions).
    pub(crate) fn suggest(&self, target: &str, n: usize) -> Vec<(String, f64)> {
        let t = target.to_lowercase();
        let mut cands: Vec<(String, f64)> = self
            .pages
            .iter()
            .flat_map(|p| std::iter::once(p.stem.clone()).chain(p.aliases.iter().cloned()))
            .map(|name| {
                let s = strsim::jaro_winkler(&t, &name.to_lowercase());
                (name, s)
            })
            .filter(|(_, s)| *s > SUGGESTION_CUTOFF)
            .collect();
        cands.sort_by(|a, b| b.1.total_cmp(&a.1));
        cands.dedup_by(|a, b| a.0 == b.0);
        cands.truncate(n);
        cands
    }

    /// A `page_not_found` error for `name`, with the closest titles as suggestions.
    pub fn page_not_found(&self, name: &str) -> Error {
        let suggestions = self.suggest(name, 3).into_iter().map(|(s, _)| s).collect();
        Error::page_not_found(name, suggestions)
    }

    /// Inbound link map: page index -> set of page indexes linking to it.
    pub(crate) fn backlinks(&self) -> HashMap<usize, Vec<usize>> {
        let mut map: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, p) in self.pages.iter().enumerate() {
            for t in p.all_link_targets() {
                if let Resolution::Found(j) = self.resolve(&t)
                    && j != i
                {
                    let e = map.entry(j).or_default();
                    if !e.contains(&i) {
                        e.push(i);
                    }
                }
            }
        }
        map
    }

    /// Pages whose type is `t`.
    pub fn pages_of_type<'a>(&'a self, t: &'a str) -> impl Iterator<Item = &'a Page> + 'a {
        self.pages.iter().filter(move |p| p.ptype == t)
    }

    /// Folder of installed packs.
    pub fn packs_dir(&self) -> PathBuf {
        tool_dir(&self.root).join("packs")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Resolution {
    Found(usize),
    Ambiguous(Vec<usize>),
    Missing,
}

pub(crate) fn walk_markdown(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .follow_links(false)
        .filter_entry({
            let root = root.to_path_buf();
            move |e| {
                let rel = fsutil::rel(&root, e.path());
                if rel.is_empty() {
                    return true;
                }
                !SKIP_DIRS
                    .iter()
                    .any(|s| rel == *s || rel.starts_with(&format!("{s}/")))
            }
        })
        .build();
    for e in walker.flatten() {
        if e.file_type().is_some_and(|t| t.is_file()) && fsutil::is_markdown(e.path()) {
            let rel = fsutil::rel(root, e.path());
            if SKIP_FILES.contains(&rel.as_str()) {
                continue;
            }
            out.push(e.path().to_path_buf());
        }
    }
    out
}

pub(crate) fn load_page(root: &Path, path: &Path) -> Result<Page> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::io("reading", path, e))?;
    let doc =
        frontmatter::parse(&text).map_err(|e| e.in_file(Path::new(&fsutil::rel(root, path))))?;
    let ptype = frontmatter::get_str(&doc.fm, "type").unwrap_or_default();
    let stem = fsutil::stem(path);
    let title = frontmatter::get_str(&doc.fm, "title")
        .or_else(|| markdown::first_h1(&doc.body))
        .unwrap_or_else(|| stem.clone());
    let aliases = frontmatter::get_list(&doc.fm, "aliases");
    let tags = frontmatter::get_list(&doc.fm, "tags")
        .into_iter()
        .map(|t| t.trim_start_matches('#').to_string())
        .collect();
    let body_links = markdown::wikilinks(&doc.body);
    let mut fm_links = vec![];
    for (k, v) in &doc.fm {
        let key = frontmatter::scalar_to_string(k);
        let text = frontmatter::scalar_to_string(v);
        for t in markdown::wikilink_targets_in(&text) {
            fm_links.push((key.clone(), t));
        }
    }
    let mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
    Ok(Page {
        rel: fsutil::rel(root, path),
        path: path.to_path_buf(),
        stem,
        doc,
        ptype,
        title,
        aliases,
        tags,
        body_links,
        fm_links,
        parse_error: None,
        mtime,
    })
}
