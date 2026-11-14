//! Smaller operations on a commonplace.
//!
//! Log, changed-set, rename, dedupe, trace, archive, export, status and
//! review packets.

use crate::cite;
use crate::frontmatter;
use crate::fsutil;
use crate::markdown;
use crate::pack::Pack;
use crate::sources;
use crate::vault::{Page, Vault};
use crate::{Error, Result};
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

// ---------- log ----------

/// Append an entry `## [date] op | title` with `bullets` to `log.md`.
///
/// # Errors
///
/// Returns an `io` or `parse` error when a file cannot be read, written or parsed.
pub fn log_append(root: &Path, op: &str, title: &str, bullets: &[String]) -> Result<()> {
    let entry = Entry {
        op,
        title,
        bullets,
        trailer: None,
    };
    append_entry(
        &root.join("log.md"),
        "---\ntype: log\n---\n\n# Log\n",
        &entry,
    )
}

/// One `## [date] op | title` entry of an append-only record.
pub(crate) struct Entry<'a> {
    pub(crate) op: &'a str,
    pub(crate) title: &'a str,
    /// Lines written as `- bullet`.
    pub(crate) bullets: &'a [String],
    /// A block after the bullets (a fenced diff, say).
    pub(crate) trailer: Option<&'a str>,
}

/// Append `entry` to the record at `path`, starting it from `fallback` when
/// the file does not exist yet. `log.md` and `skill-impact.md` share this
/// format; `skill::usage` parses it back.
///
/// # Errors
///
/// `io` when the file exists but cannot be read, or cannot be written.
pub(crate) fn append_entry(path: &Path, fallback: &str, entry: &Entry<'_>) -> Result<()> {
    let mut text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => fallback.to_string(),
        Err(e) => return Err(Error::io("reading", path, e)),
    };
    if !text.ends_with('\n') {
        text.push('\n');
    }
    let _ = write!(
        text,
        "\n## [{}] {} | {}\n",
        fsutil::today(),
        entry.op,
        entry.title
    );
    for b in entry.bullets {
        let _ = writeln!(text, "- {b}");
    }
    if let Some(t) = entry.trailer {
        let _ = write!(text, "\n{t}");
        if !t.ends_with('\n') {
            text.push('\n');
        }
    }
    fsutil::write_atomic(path, text.as_bytes())
}

/// Keep the newest `keep` entries in log.md; move the rest to `archive/log-<year>.md`.
///
/// # Errors
///
/// `io` or `frontmatter` when `log.md` or an archive file cannot be read or written.
pub fn log_rotate(root: &Path, keep: usize) -> Result<usize> {
    let p = root.join("log.md");
    let text = std::fs::read_to_string(&p)?;
    let doc = frontmatter::parse(&text)?;
    let re = crate::re(r"(?m)^## \[(\d{4})-\d{2}-\d{2}\] ");
    let starts: Vec<(usize, String)> = re
        .captures_iter(&doc.body)
        .map(|c| (c.get_match().start(), c[1].to_string()))
        .collect();
    if starts.len() <= keep {
        return Ok(0);
    }
    let cut = starts.len() - keep;
    let head = &doc.body[..starts[0].0];
    let mut moved = 0;
    let mut by_year: BTreeMap<String, String> = BTreeMap::new();
    for i in 0..cut {
        let end = starts.get(i + 1).map_or(doc.body.len(), |s| s.0);
        by_year
            .entry(starts[i].1.clone())
            .or_default()
            .push_str(&doc.body[starts[i].0..end]);
        moved += 1;
    }
    for (y, entries) in by_year {
        let ap = root.join("archive").join(format!("log-{y}.md"));
        let mut t = std::fs::read_to_string(&ap)
            .unwrap_or_else(|_| format!("---\ntype: log\n---\n\n# Log {y}\n\n"));
        t.push_str(&entries);
        fsutil::write_atomic(&ap, t.as_bytes())?;
    }
    let new_body = format!("{}{}", head, &doc.body[starts[cut].0..]);
    fsutil::write_atomic(&p, frontmatter::render(&doc.fm, &new_body)?.as_bytes())?;
    Ok(moved)
}

// ---------- changed set ----------

/// Markdown files git reports as changed (or, on a clean tree, changed by the
/// last commit); `None` when git is unavailable or fails.
fn git_changed_markdown(root: &Path) -> Option<HashSet<String>> {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
    };
    let markdown_only = |paths: Vec<String>| -> HashSet<String> {
        paths
            .into_iter()
            .filter(|p| fsutil::is_markdown(Path::new(p)))
            .collect()
    };
    let status = git(&["status", "--porcelain", "-z", "--untracked-files=all"])?;
    let mut paths = markdown_only(porcelain_z_paths(&status.stdout));
    // Only non-markdown files dirty (the manifest is, after almost every run):
    // treat the tree as clean and report the pages of the last commit.
    if paths.is_empty()
        && let Some(diff) = git(&["diff", "--name-only", "-z", "HEAD~1", "HEAD"])
    {
        paths = markdown_only(nul_separated(&diff.stdout).map(String::from).collect());
    }
    Some(paths)
}

/// NUL-separated, non-empty fields of git `-z` output (never quoted or escaped).
fn nul_separated(out: &[u8]) -> impl Iterator<Item = &str> {
    out.split(|&b| b == 0)
        .filter(|f| !f.is_empty())
        .filter_map(|f| std::str::from_utf8(f).ok())
}

/// Paths from `git status --porcelain -z`: each entry is `XY path`; a rename
/// or copy entry is followed by its original path, which is skipped.
fn porcelain_z_paths(out: &[u8]) -> Vec<String> {
    let mut paths = vec![];
    let mut fields = nul_separated(out);
    while let Some(entry) = fields.next() {
        let (Some(status), Some(path)) = (entry.get(..2), entry.get(3..)) else {
            continue;
        };
        paths.push(path.to_string());
        if status.contains(['R', 'C']) {
            fields.next();
        }
    }
    paths
}

/// Markdown files changed since the last commit (git) or since the last lint (mtime).
///
/// # Errors
///
/// `io` when the folder cannot be walked.
pub fn changed_files(root: &Path) -> Result<HashSet<String>> {
    if root.join(".git").exists()
        && let Some(set) = git_changed_markdown(root)
    {
        return Ok(set);
    }
    let mut set = HashSet::new();
    // An unreadable manifest means "no last lint": everything counts as changed,
    // so the run goes on and lint reports the manifest itself (SRC-005).
    let since = crate::manifest::Manifest::load(root)
        .ok()
        .and_then(|m| m.last_lint)
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&chrono::Local));
    for path in crate::vault::walk_markdown(root) {
        let m = std::fs::metadata(&path)?.modified()?;
        let mt: chrono::DateTime<chrono::Local> = m.into();
        if since.is_none_or(|s| mt > s) {
            set.insert(fsutil::rel(root, &path));
        }
    }
    Ok(set)
}

// ---------- rename ----------

/// What `rename` changed.
#[derive(Debug, Serialize)]
pub struct RenameReport {
    /// Old vault-relative path.
    pub from: String,
    /// New vault-relative path.
    pub to: String,
    /// Pages whose links were rewritten.
    pub files_updated: Vec<String>,
    /// Number of links rewritten.
    pub links_rewritten: usize,
}

/// An escaped title only fails to compile when it exceeds the regex size limit.
fn title_regex_error(e: &regex::Error) -> Error {
    Error::invalid_input(format!("title is too long to match: {e}"))
}

/// Rename a page to `new_title` and rewrite every link to it.
///
/// # Errors
///
/// `page_not_found` for an unknown page, `invalid_input` for a skill's page,
/// `already_exists` when the new title is taken, `io` on write failure.
pub fn rename(vault: &Vault, old: &str, new_title: &str) -> Result<RenameReport> {
    let page = vault
        .resolve_page(old)
        .ok_or_else(|| vault.page_not_found(old))?;
    if page.ptype == "skill" {
        return Err(Error::invalid_input(format!(
            "'{}' is a skill, named by its folder: create the new one with `commonplace skill new`, then retire this one",
            page.stem
        )));
    }
    let new_title = fsutil::safe_title(new_title);
    if vault.resolve_page(&new_title).is_some() {
        return Err(Error::already_exists(format!("a page named '{new_title}'")));
    }
    let new_path = page.path.with_file_name(format!("{new_title}.md"));
    let old_stem = page.stem.clone();
    // rewrite links everywhere
    let re = Regex::new(&format!(
        r"(?i)\[\[(?:[^\]\|#]*/)?{}(#[^\]\|]*)?(\|[^\]]*)?\]\]",
        regex::escape(&old_stem)
    ))
    .map_err(|e| title_regex_error(&e))?;
    let rewrite = |text: &str| {
        re.replace_all(text, |c: &regex::Captures<'_>| {
            let h = c.get(1).map_or("", |m| m.as_str());
            let a = c.get(2).map_or("", |m| m.as_str());
            format!("[[{new_title}{h}{a}]]")
        })
        .into_owned()
    };
    let mut files_updated = vec![];
    let mut links = 0;
    for p in &vault.pages {
        let text = std::fs::read_to_string(&p.path)?;
        let n = re.find_iter(&text).count();
        if n == 0 {
            continue;
        }
        fsutil::write_atomic(&p.path, rewrite(&text).as_bytes())?;
        links += n;
        files_updated.push(p.rel.clone());
        // a skill's accepted PURPOSE.md gets the same rewrite, so a rename is
        // not mistaken for a proposal waiting at the gate
        if let Some((skill, true)) = crate::skill::skill_file(&vault.root, &p.rel) {
            let accepted = crate::skill::accepted_purpose(&vault.root, skill);
            if let Ok(old_text) = std::fs::read_to_string(&accepted) {
                fsutil::write_atomic(&accepted, rewrite(&old_text).as_bytes())?;
            }
        }
    }
    // update the page itself: title, alias, h1
    let text = std::fs::read_to_string(&page.path)?;
    let mut doc = frontmatter::parse(&text)?;
    if frontmatter::get_str(&doc.fm, "title").is_some() {
        frontmatter::set_str(&mut doc.fm, "title", &new_title);
    }
    let mut aliases = frontmatter::get_list(&doc.fm, "aliases");
    if !aliases.iter().any(|a| a.eq_ignore_ascii_case(&old_stem)) {
        aliases.push(old_stem.clone());
    }
    frontmatter::set_value(
        &mut doc.fm,
        "aliases",
        serde_yaml::Value::Sequence(aliases.into_iter().map(serde_yaml::Value::String).collect()),
    );
    frontmatter::set_str(&mut doc.fm, "updated", &fsutil::today());
    let h1 = Regex::new(&format!(r"(?m)^# {}\s*$", regex::escape(&old_stem)))
        .map_err(|e| title_regex_error(&e))?;
    let body = h1.replace(&doc.body, format!("# {new_title}")).to_string();
    fsutil::write_atomic(&new_path, frontmatter::render(&doc.fm, &body)?.as_bytes())?;
    std::fs::remove_file(&page.path)?;
    Ok(RenameReport {
        from: page.rel.clone(),
        to: fsutil::rel(&vault.root, &new_path),
        files_updated,
        links_rewritten: links,
    })
}

// ---------- dedupe ----------

/// Two pages that may be duplicates.
#[derive(Debug, Serialize)]
pub struct DupeCandidate {
    /// First page (vault-relative path).
    pub a: String,
    /// Second page (vault-relative path).
    pub b: String,
    /// Best name similarity, 0–1 (Jaro-Winkler or word overlap).
    pub similarity: f64,
    /// Why they were paired.
    pub reason: String,
}

/// Pairs of concept, entity, claim, person, topic and playbook pages whose names or aliases are at least `threshold` similar.
pub fn dedupe(vault: &Vault, threshold: f64) -> Vec<DupeCandidate> {
    let mut out = vec![];
    let cand: Vec<&Page> = vault
        .pages
        .iter()
        .filter(|p| {
            ["concept", "entity", "claim", "person", "topic", "playbook"]
                .contains(&p.ptype.as_str())
                || p.ptype.is_empty()
        })
        .collect();
    for (i, a) in cand.iter().enumerate() {
        for b in cand.iter().skip(i + 1) {
            let names_a: Vec<String> = std::iter::once(a.stem.to_lowercase())
                .chain(a.aliases.iter().map(|s| s.to_lowercase()))
                .collect();
            let names_b: Vec<String> = std::iter::once(b.stem.to_lowercase())
                .chain(b.aliases.iter().map(|s| s.to_lowercase()))
                .collect();
            let mut best = 0.0f64;
            for x in &names_a {
                for y in &names_b {
                    best = best.max(strsim::jaro_winkler(x, y));
                    // token overlap
                    let tx: HashSet<&str> = x.split_whitespace().collect();
                    let ty: HashSet<&str> = y.split_whitespace().collect();
                    let inter = crate::count_f64(tx.intersection(&ty).count());
                    let uni = crate::count_f64(tx.union(&ty).count());
                    if uni > 0.0 {
                        best = best.max(inter / uni);
                    }
                }
            }
            let shared: Vec<String> = a
                .list("sources")
                .into_iter()
                .filter(|s| b.list("sources").contains(s))
                .collect();
            let sim = if shared.len() >= 2 {
                best.max(0.75)
            } else {
                best
            };
            if sim >= threshold && a.ptype == b.ptype {
                out.push(DupeCandidate {
                    a: a.rel.clone(),
                    b: b.rel.clone(),
                    similarity: (sim * 1000.0).round() / 1000.0,
                    reason: if shared.is_empty() {
                        "similar names".into()
                    } else {
                        format!("similar names and {} shared sources", shared.len())
                    },
                });
            }
        }
    }
    out.sort_by(|x, y| y.similarity.total_cmp(&x.similarity));
    out
}

// ---------- trace ----------

/// One row of a traceability table.
#[derive(Debug, Serialize)]
pub struct TraceRow {
    /// Page titles along the chain; `—` marks a gap.
    pub cells: Vec<String>,
}

/// Follow a chain `type, field, type, field, type` and produce rows. A page
/// with no link in `field` yields a row padded with "—" (a gap).
///
/// # Errors
///
/// `invalid_input` when `chain` is not `type, field, type[, field, type ...]`.
pub fn trace(vault: &Vault, chain: &[String]) -> Result<(Vec<String>, Vec<TraceRow>)> {
    if chain.len() < 3 || chain.len().is_multiple_of(2) {
        return Err(Error::invalid_input(
            "chain must be type, field, type[, field, type ...]",
        ));
    }
    let header: Vec<String> = chain.iter().step_by(2).cloned().collect();
    let mut rows: Vec<Vec<String>> = vault
        .pages_of_type(&chain[0])
        .map(|p| vec![p.stem.clone()])
        .collect();
    let mut k = 1;
    while k + 1 < chain.len() {
        let field = &chain[k];
        let want_type = &chain[k + 1];
        let mut next: Vec<Vec<String>> = vec![];
        for row in rows {
            let Some(last) = row.last().cloned() else {
                continue;
            };
            if last == "—" {
                let mut r = row.clone();
                r.push("—".into());
                next.push(r);
                continue;
            }
            let Some(p) = vault.resolve_page(&last) else {
                continue;
            };
            // links in this field, or reverse links (pages of want_type whose field points at p)
            let mut targets: Vec<String> = p
                .list(field)
                .iter()
                .flat_map(|s| markdown::wikilink_targets_in(s))
                .collect();
            if targets.is_empty() {
                for q in vault.pages_of_type(want_type) {
                    if q.list(field)
                        .iter()
                        .flat_map(|s| markdown::wikilink_targets_in(s))
                        .any(|t| t.eq_ignore_ascii_case(&p.stem))
                    {
                        targets.push(q.stem.clone());
                    }
                }
            }
            targets.retain(|t| vault.resolve_page(t).is_some_and(|q| q.ptype == *want_type));
            if targets.is_empty() {
                let mut r = row.clone();
                r.push("—".into());
                next.push(r);
            } else {
                for t in targets {
                    let mut r = row.clone();
                    r.push(t);
                    next.push(r);
                }
            }
        }
        rows = next;
        k += 2;
    }
    Ok((
        header,
        rows.into_iter().map(|cells| TraceRow { cells }).collect(),
    ))
}

/// The trace called `name` declared by one of `packs`.
pub fn pack_trace<'a>(packs: &'a [Pack], name: &str) -> Option<&'a crate::pack::Trace> {
    packs
        .iter()
        .flat_map(|p| p.traces.iter())
        .find(|t| t.name == name)
}

// ---------- archive ----------

/// Move a project or area (its whole folder) into `archive/`; returns the moved files.
///
/// # Errors
///
/// `page_not_found`, `already_exists` when the archive folder exists, `io` on move failure.
pub fn archive(vault: &Vault, target: &str) -> Result<Vec<String>> {
    let page = vault
        .resolve_page(target)
        .ok_or_else(|| vault.page_not_found(target))?;
    let folder = page.folder();
    let src_dir = if folder.is_empty() {
        page.path.clone()
    } else {
        vault.root.join(&folder)
    };
    let dest = vault.root.join("archive").join(if folder.is_empty() {
        page.rel.clone()
    } else {
        folder.clone()
    });
    if dest.exists() {
        return Err(Error::already_exists(dest.display().to_string()));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&src_dir, &dest)?;
    let mut moved = vec![];
    for p in crate::vault::walk_markdown(&dest) {
        let text = std::fs::read_to_string(&p)?;
        if let Ok(mut doc) = frontmatter::parse(&text)
            && doc.has_frontmatter
            && ["project", "area"].contains(
                &frontmatter::get_str(&doc.fm, "type")
                    .unwrap_or_default()
                    .as_str(),
            )
        {
            frontmatter::set_str(&mut doc.fm, "status", "archived");
            frontmatter::set_str(&mut doc.fm, "archived", &fsutil::today());
            fsutil::write_atomic(&p, frontmatter::render(&doc.fm, &doc.body)?.as_bytes())?;
        }
        moved.push(fsutil::rel(&vault.root, &p));
    }
    Ok(moved)
}

// ---------- export ----------

/// Pages named by `targets`: each target is a page name, or else a folder prefix
/// (pages under it that pass `keep`).
fn select_pages<'v, 't>(
    vault: &'v Vault,
    targets: impl IntoIterator<Item = &'t str>,
    keep: impl Fn(&Page) -> bool,
) -> Vec<&'v Page> {
    let mut pages = vec![];
    for t in targets {
        if let Some(p) = vault.resolve_page(t) {
            pages.push(p);
        } else {
            let prefix = t.trim_end_matches('/');
            pages.extend(
                vault
                    .pages
                    .iter()
                    .filter(|p| p.rel.starts_with(prefix) && keep(p)),
            );
        }
    }
    pages
}

/// One bibliography entry: `- **Title**, author (published), origin — retrieved date`.
fn bibliography_line(sp: &Page) -> String {
    let field = |key: &str, prefix: &str, suffix: &str| {
        sp.get(key)
            .filter(|v| !v.is_empty())
            .map(|v| format!("{prefix}{v}{suffix}"))
            .unwrap_or_default()
    };
    format!(
        "- **{}**{}{}{} — retrieved {}",
        sp.title,
        field("author", ", ", ""),
        field("published", " (", ")"),
        field("origin", ", ", ""),
        sp.get("retrieved").unwrap_or_default()
    )
}

/// Concatenate the `targets` pages into one markdown document with a bibliography, withholding confidential content unless `include_confidential`.
///
/// # Errors
///
/// `invalid_input` when nothing matches `targets`.
pub fn export(vault: &Vault, targets: &[String], include_confidential: bool) -> Result<String> {
    let excluded: &[String] = if include_confidential {
        &[]
    } else {
        &vault.config.privacy.exclude_from_export
    };
    let confidentiality = |p: &Page| {
        p.get("confidentiality")
            .unwrap_or_else(|| "personal".into())
    };
    let pages = select_pages(vault, targets.iter().map(String::as_str), |_| true);
    if pages.is_empty() {
        return Err(Error::invalid_input(format!("nothing matched {targets:?}")));
    }
    let mut out = String::new();
    let mut bib: BTreeMap<String, &Page> = BTreeMap::new();
    let mut counter = 0usize;
    for p in pages {
        let conf = confidentiality(p);
        if excluded.contains(&conf) {
            let _ = write!(
                out,
                "\n<!-- {} omitted: confidentiality {} -->\n",
                p.rel, conf
            );
            continue;
        }
        let (cites, _) = cite::parse_citations(&p.doc.body);
        // renumber footnotes to keep labels unique across pages
        let mut body = renumber_footnotes(&p.doc.body, &mut counter);
        for c in &cites {
            let Some(sp) = vault.resolve_page(&c.source) else {
                continue;
            };
            if !excluded.contains(&confidentiality(sp)) {
                bib.insert(c.source.clone(), sp);
            } else if let Some(q) = &c.quote {
                body = body.replace(q, "[quote withheld: confidential source]");
            }
        }
        if !body.trim_start().starts_with("# ") {
            let _ = write!(out, "\n# {}\n", p.title);
        }
        out.push_str(&body);
        out.push('\n');
    }
    if !bib.is_empty() {
        out.push_str("\n## Sources\n\n");
        for sp in bib.values() {
            out.push_str(&bibliography_line(sp));
            out.push('\n');
        }
    }
    Ok(out)
}

// ---------- status ----------

/// Counts and pending work, for `status`.
#[derive(Debug, Serialize, Default)]
pub struct Status {
    /// The commonplace's name.
    pub name: String,
    /// Number of pages.
    pub pages: usize,
    /// Pages per type.
    pub by_type: BTreeMap<String, usize>,
    /// Active project pages.
    pub active_projects: Vec<String>,
    /// Active projects with no next action recorded.
    pub projects_without_next_action: Vec<String>,
    /// Files waiting in `inbox/`.
    pub inbox_items: usize,
    /// Registered sources.
    pub sources_total: usize,
    /// Sources without a text layer.
    pub sources_unconverted: usize,
    /// Sources whose note is not yet summarised.
    pub sources_unsummarised: usize,
    /// Footnote citations across all pages.
    pub citations_total: usize,
    /// Question pages with status `open`.
    pub open_questions: usize,
    /// Unresolved entries in pages' `contradictions` lists.
    pub open_contradictions: usize,
    /// Date of the newest journal entry.
    pub journal_last: Option<String>,
    /// Enabled packs.
    pub packs: Vec<String>,
    /// When `lint` last ran.
    pub last_lint: Option<String>,
}

/// Gather the status counts.
///
/// # Errors
///
/// Returns an `io` or `parse` error when a file cannot be read, written or parsed.
pub fn status(vault: &Vault) -> Result<Status> {
    let mut s = Status {
        name: vault.config.name.clone(),
        pages: vault.pages.len(),
        packs: vault.config.packs.clone(),
        ..Default::default()
    };
    let manifest = crate::manifest::Manifest::load(&vault.root)?;
    s.last_lint.clone_from(&manifest.last_lint);
    for p in &vault.pages {
        *s.by_type
            .entry(if p.ptype.is_empty() {
                "(none)".into()
            } else {
                p.ptype.clone()
            })
            .or_default() += 1;
        if p.ptype == "project" && p.get("status").as_deref() == Some("active") {
            s.active_projects.push(p.stem.clone());
            if p.list("next_actions").is_empty()
                && markdown::section(&p.doc.body, "Next actions")
                    .is_none_or(|x| !markdown::has_prose(x))
            {
                s.projects_without_next_action.push(p.stem.clone());
            }
        }
        if p.ptype == "source" {
            s.sources_total += 1;
            match p.get("status").as_deref() {
                Some("registered") => s.sources_unconverted += 1,
                Some("converted") => s.sources_unsummarised += 1,
                _ => {}
            }
        }
        if p.ptype == "question" && p.get("status").as_deref() == Some("open") {
            s.open_questions += 1;
        }
        if let Some(serde_yaml::Value::Sequence(cs)) = p.fm().get("contradictions") {
            s.open_contradictions += cs.iter().filter(|c| matches!(c, serde_yaml::Value::Mapping(m) if frontmatter::get_str(m, "status").as_deref() != Some("resolved"))).count();
        }
        if p.ptype == "journal" {
            s.journal_last = Some(
                s.journal_last
                    .take()
                    .map(|j| if p.stem > j { p.stem.clone() } else { j })
                    .unwrap_or(p.stem.clone()),
            );
        }
        let (c, _) = cite::parse_citations(&p.doc.body);
        s.citations_total += c.len();
    }
    if let Ok(rd) = std::fs::read_dir(vault.root.join("inbox")) {
        s.inbox_items = rd
            .flatten()
            .filter(|e| e.path().is_file() && !e.file_name().to_string_lossy().starts_with('.'))
            .count();
    }
    Ok(s)
}

// ---------- review packets ----------

/// A reviewer persona: its name and its brief (the persona file's text).
#[derive(Debug, Clone, Copy)]
pub struct Persona<'a> {
    /// Persona name, e.g. `skeptic`.
    pub name: &'a str,
    /// The brief the reviewer works from.
    pub brief: &'a str,
}

/// Build a review packet: the persona brief, the target pages and every cited span with `context_lines` of context.
///
/// # Errors
///
/// `invalid_input` when nothing matches `targets`.
pub fn review_packet(
    vault: &Vault,
    persona: &Persona<'_>,
    targets: &[String],
    context_lines: usize,
) -> Result<String> {
    let Persona {
        name: persona,
        brief: persona_text,
    } = *persona;
    let pages = select_pages(vault, targets.iter().map(String::as_str), |p| {
        !p.rel.starts_with("indexes/")
    });
    if pages.is_empty() {
        return Err(Error::invalid_input(format!("nothing matched {targets:?}")));
    }
    let mut out = String::new();
    let _ = write!(
        out,
        "# Review packet — persona: {persona}\n\nGenerated {} by `commonplace review prepare`. Pages: {}\n\n",
        fsutil::now_iso(),
        pages
            .iter()
            .map(|p| p.rel.clone())
            .collect::<Vec<_>>()
            .join(", ")
    );
    out.push_str("## Persona brief\n\n");
    out.push_str(persona_text.trim());
    out.push_str("\n\n## Output format\n\nReturn ONLY a JSON array of findings, each:\n```json\n{\"severity\": \"blocker|major|minor|note\", \"page\": \"<rel path>\", \"location\": \"<heading or line>\", \"claim\": \"<the sentence at issue>\", \"problem\": \"<what is wrong>\", \"evidence\": \"<verbatim quote from the page or source span that shows it>\", \"recommendation\": \"<concrete fix>\"}\n```\nReturn `[]` if nothing is wrong. Do not edit any file. Every finding must carry an `evidence` quote.\n\n");
    for p in pages {
        let _ = write!(out, "---\n\n## Page: {}\n\n", p.rel);
        out.push_str("```markdown\n");
        out.push_str(&frontmatter::render(&p.doc.fm, &p.doc.body)?);
        out.push_str("\n```\n\n");
        let (cites, _) = cite::parse_citations(&p.doc.body);
        if cites.is_empty() {
            continue;
        }
        out.push_str("### Cited source spans\n\n");
        for c in &cites {
            let _ = write!(
                out,
                "#### [^{}] → [[{}]] {}\n\n",
                c.label, c.source, c.locator_raw
            );
            match sources::load_text_layer(&vault.root, &c.source) {
                Ok(Some(tl)) => {
                    let (v, found, _near, msg) = cite::verify_quote(
                        &tl.text,
                        c,
                        vault.config.lint.quote_min_words,
                        vault.config.lint.quote_max_words,
                    );
                    let _ = write!(out, "Deterministic check: **{v:?}** — {msg}\n\n");
                    let lines: Vec<&str> = tl.text.lines().collect();
                    let centre = found.or_else(|| {
                        cite::span(&tl.text, &c.locator).and_then(|s| s.first().map(|x| x.0))
                    });
                    if let Some(l) = centre {
                        let from = l.saturating_sub(1 + context_lines * 2);
                        let to = (l + context_lines * 4).min(lines.len());
                        out.push_str("```text\n");
                        for (i, line) in lines.iter().enumerate().take(to).skip(from) {
                            let _ = writeln!(out, "{:>5} | {}", i + 1, line);
                        }
                        out.push_str("```\n\n");
                    }
                }
                _ => out.push_str("(no text layer available for this source)\n\n"),
            }
        }
    }
    Ok(out)
}

/// What `review record` wrote.
#[derive(Debug, Serialize)]
pub struct ReviewRecord {
    /// Pages stamped with the review.
    pub pages_updated: Vec<String>,
    /// Findings recorded.
    pub findings: usize,
    /// Findings at blocker severity.
    pub blockers: usize,
}

/// Record persona findings (JSON array) into the reviewed pages' frontmatter and
/// write a review page under reviews/.
///
/// # Errors
///
/// `invalid_input` for malformed findings JSON or no matching pages; `frontmatter` or `io` when a page cannot be updated.
pub fn review_record(
    vault: &Vault,
    persona: &str,
    scope: &str,
    findings_json: &str,
    model: Option<&str>,
) -> Result<ReviewRecord> {
    let findings: Vec<serde_json::Value> = serde_json::from_str(findings_json.trim())
        .map_err(|e| Error::invalid_input(format!("findings must be a JSON array: {e}")))?;
    let today = fsutil::today();
    let text_of = |f: &serde_json::Value, k: &str| {
        f.get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let mut per_page: BTreeMap<String, usize> = BTreeMap::new();
    for f in &findings {
        *per_page.entry(text_of(f, "page")).or_default() += 1;
    }
    let blockers = findings
        .iter()
        .filter(|f| text_of(f, "severity") == "blocker")
        .count();
    let review = ReviewPage {
        persona,
        scope,
        model,
        date: &today,
    };
    let review_link = format!("[[{}]]", fsutil::stem(Path::new(&review.rel())));
    let pages = select_pages(vault, scope.split(',').map(str::trim), |p| {
        p.ptype != "index"
    });
    // Stamp every page in memory first: a page that cannot be read or parsed
    // aborts the record before anything is written.
    let stamped = pages
        .into_iter()
        .map(|p| {
            let n = per_page.get(&p.rel).copied().unwrap_or(0);
            Ok((p, review.stamped(p, n, &review_link)?))
        })
        .collect::<Result<Vec<_>>>()?;
    review.write(vault, &findings, blockers, text_of)?;
    for (p, text) in &stamped {
        fsutil::write_atomic(&p.path, text.as_bytes())?;
    }
    Ok(ReviewRecord {
        pages_updated: stamped.iter().map(|(p, _)| p.rel.clone()).collect(),
        findings: findings.len(),
        blockers,
    })
}

/// One persona review being recorded.
struct ReviewPage<'a> {
    persona: &'a str,
    scope: &'a str,
    model: Option<&'a str>,
    date: &'a str,
}

impl ReviewPage<'_> {
    /// Vault-relative path of the review page: `reviews/<date> <persona> <scope>.md`.
    fn rel(&self) -> String {
        format!(
            "reviews/{} {} {}.md",
            self.date,
            self.persona,
            fsutil::safe_title(self.scope)
        )
    }

    /// Write the review page listing every finding.
    fn write(
        &self,
        vault: &Vault,
        findings: &[serde_json::Value],
        blockers: usize,
        text_of: impl Fn(&serde_json::Value, &str) -> String,
    ) -> Result<()> {
        let (persona, scope) = (self.persona, self.scope);
        let rel = self.rel();
        let mut fm = serde_yaml::Mapping::new();
        frontmatter::set_str(&mut fm, "type", "review");
        frontmatter::set_str(&mut fm, "persona", persona);
        frontmatter::set_str(&mut fm, "date", self.date);
        frontmatter::set_str(&mut fm, "scope", scope);
        if let Some(m) = self.model {
            frontmatter::set_str(&mut fm, "model", m);
        }
        frontmatter::set_value(
            &mut fm,
            "findings",
            serde_yaml::Value::Number(findings.len().into()),
        );
        frontmatter::set_value(
            &mut fm,
            "blockers",
            serde_yaml::Value::Number(blockers.into()),
        );
        frontmatter::set_str(&mut fm, "resolution", "pending");
        let mut body = format!(
            "# Review: {persona} on {scope}\n\n{} finding(s), {blockers} blocker(s). Resolve each: fix, waive (with reason), or file as an open question.\n\n",
            findings.len(),
        );
        for f in findings {
            let g = |k: &str| text_of(f, k);
            let _ = write!(
                body,
                "## [{}] {} — {}\n\n- **Claim:** {}\n- **Problem:** {}\n- **Evidence:** {}\n- **Recommendation:** {}\n- **Resolution:** _pending_\n\n",
                g("severity"),
                g("page"),
                g("location"),
                g("claim"),
                g("problem"),
                g("evidence"),
                g("recommendation")
            );
        }
        fsutil::write_atomic(
            &vault.root.join(&rel),
            frontmatter::render(&fm, &body)?.as_bytes(),
        )
    }

    /// The page's text with this review appended to its `reviews` frontmatter list.
    fn stamped(&self, p: &Page, findings: usize, review_link: &str) -> Result<String> {
        let text =
            std::fs::read_to_string(&p.path).map_err(|e| Error::io("reading", &p.path, e))?;
        let mut doc = frontmatter::parse(&text).map_err(|e| e.in_file(Path::new(&p.rel)))?;
        let mut reviews = match doc.fm.get("reviews") {
            Some(serde_yaml::Value::Sequence(s)) => s.clone(),
            _ => vec![],
        };
        let mut entry = serde_yaml::Mapping::new();
        frontmatter::set_str(&mut entry, "persona", self.persona);
        frontmatter::set_str(&mut entry, "date", self.date);
        if let Some(m) = self.model {
            frontmatter::set_str(&mut entry, "model", m);
        }
        frontmatter::set_str(
            &mut entry,
            "result",
            if findings == 0 { "pass" } else { "findings" },
        );
        frontmatter::set_value(
            &mut entry,
            "findings",
            serde_yaml::Value::Number(findings.into()),
        );
        frontmatter::set_str(&mut entry, "review", review_link);
        reviews.push(serde_yaml::Value::Mapping(entry));
        frontmatter::set_value(&mut doc.fm, "reviews", serde_yaml::Value::Sequence(reviews));
        frontmatter::render(&doc.fm, &doc.body)
    }
}

// ---------- backlinks / links ----------

/// A link whose target has no page.
#[derive(Debug, Serialize)]
pub struct LinkIssue {
    /// Vault-relative path of the linking page.
    pub page: String,
    /// 1-based line of a body link; `None` for frontmatter links.
    pub line: Option<usize>,
    /// The target as written.
    pub target: String,
    /// Existing pages it probably meant.
    pub suggestions: Vec<String>,
}

/// Every link in the vault whose target has no page.
pub fn broken_links(vault: &Vault) -> Vec<LinkIssue> {
    let known: HashSet<String> = sources::known_source_ids(&vault.root).into_iter().collect();
    let mut out = vec![];
    for p in &vault.pages {
        for l in &p.body_links {
            if vault.resolve_page(&l.target).is_none() && !known.contains(&l.target) {
                out.push(LinkIssue {
                    page: p.rel.clone(),
                    line: Some(l.line + p.doc.body_line_offset),
                    target: l.target.clone(),
                    suggestions: vault
                        .suggest(&l.target, 3)
                        .into_iter()
                        .map(|(s, _)| s)
                        .collect(),
                });
            }
        }
        for (_, t) in &p.fm_links {
            if vault.resolve_page(t).is_none() && !known.contains(t) {
                out.push(LinkIssue {
                    page: p.rel.clone(),
                    line: Some(1),
                    target: t.clone(),
                    suggestions: vault.suggest(t, 3).into_iter().map(|(s, _)| s).collect(),
                });
            }
        }
    }
    out
}

/// Give every footnote defined in `body` the next number from `counter`,
/// rewriting all references and definitions in one pass so that a new label
/// can never collide with an old one that is still to be rewritten.
fn renumber_footnotes(body: &str, counter: &mut usize) -> String {
    let map: HashMap<String, String> = markdown::footnote_defs(body)
        .into_iter()
        .map(|d| {
            *counter += 1;
            (d.label, counter.to_string())
        })
        .collect();
    markdown::FOOTNOTE_REF_RE
        .replace_all(body, |c: &regex::Captures<'_>| match map.get(&c[1]) {
            Some(new) => format!("[^{new}]"),
            None => c[0].to_string(),
        })
        .into_owned()
}

/// Vault-relative paths of the pages that link to `target`.
///
/// # Errors
///
/// `page_not_found` for an unknown page.
pub fn backlinks_of(vault: &Vault, target: &str) -> Result<Vec<String>> {
    let crate::vault::Resolution::Found(idx) = vault.resolve(target) else {
        return Err(vault.page_not_found(target));
    };
    let bl = vault.backlinks();
    let mut v: Vec<String> = bl
        .get(&idx)
        .map(|xs| xs.iter().map(|&i| vault.pages[i].rel.clone()).collect())
        .unwrap_or_default();
    v.sort();
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::renumber_footnotes;

    #[test]
    fn porcelain_z_keeps_unicode_names_and_new_rename_targets() {
        let out = " M resources/concepts/Café notes.md\0\
                   ?? \"quoted\" name.md\0\
                   R  new name.md\0old name.md\0";
        assert_eq!(
            super::porcelain_z_paths(out.as_bytes()),
            [
                "resources/concepts/Café notes.md",
                "\"quoted\" name.md",
                "new name.md"
            ]
        );
    }

    #[test]
    fn renumbering_out_of_order_labels_keeps_references_distinct() {
        let body = "A[^2] then B[^1].\n\n[^2]: second\n[^1]: first\n";
        let mut counter = 0;
        let out = renumber_footnotes(body, &mut counter);
        assert_eq!(out, "A[^1] then B[^2].\n\n[^1]: second\n[^2]: first\n");
        assert_eq!(counter, 2);
    }

    #[test]
    fn renumbering_continues_from_counter_and_leaves_undefined_refs() {
        let body = "x[^a] y[^missing]\n\n[^a]: note\n";
        let mut counter = 4;
        let out = renumber_footnotes(body, &mut counter);
        assert_eq!(out, "x[^5] y[^missing]\n\n[^5]: note\n");
    }
}
