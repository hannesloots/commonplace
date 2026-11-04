//! Smaller operations: log, changed-set, rename, dedupe, trace, archive,
//! export, status, review packets.

use crate::cite;
use crate::frontmatter;
use crate::fsutil;
use crate::markdown;
use crate::pack::Pack;
use crate::sources;
use crate::vault::{Page, Vault};
use anyhow::{Result, anyhow, bail};
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

// ---------- log ----------

pub fn log_append(root: &Path, op: &str, title: &str, bullets: &[String]) -> Result<()> {
    let p = root.join("log.md");
    let mut text = std::fs::read_to_string(&p)
        .unwrap_or_else(|_| "---\ntype: log\n---\n\n# Log\n".to_string());
    if !text.ends_with('\n') {
        text.push('\n');
    }
    let _ = write!(text, "\n## [{}] {} | {}\n", fsutil::today(), op, title);
    for b in bullets {
        let _ = writeln!(text, "- {b}");
    }
    fsutil::write_atomic(&p, text.as_bytes())
}

/// Keep the newest `keep` entries in log.md; move the rest to archive/log-<year>.md.
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

/// Markdown files changed since the last commit (git) or since the last lint (mtime).
pub fn changed_files(root: &Path) -> Result<HashSet<String>> {
    let mut set = HashSet::new();
    if root.join(".git").exists() {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["status", "--porcelain", "--untracked-files=all"])
            .output();
        if let Ok(o) = out
            && o.status.success()
        {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                if line.len() < 4 {
                    continue;
                }
                let path = line[3..].trim();
                let path = path.rsplit(" -> ").next().unwrap_or(path).trim_matches('"');
                if fsutil::is_markdown(Path::new(path)) {
                    set.insert(path.to_string());
                }
            }
            // also files changed in the last commit if the tree is clean
            if set.is_empty() {
                let o2 = std::process::Command::new("git")
                    .arg("-C")
                    .arg(root)
                    .args(["diff", "--name-only", "HEAD~1", "HEAD"])
                    .output();
                if let Ok(o2) = o2 {
                    for line in String::from_utf8_lossy(&o2.stdout).lines() {
                        if fsutil::is_markdown(Path::new(line.trim())) {
                            set.insert(line.trim().to_string());
                        }
                    }
                }
            }
            return Ok(set);
        }
    }
    let manifest = crate::manifest::Manifest::load(root)?;
    let since = manifest
        .last_lint
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

#[derive(Debug, Serialize)]
pub struct RenameReport {
    pub from: String,
    pub to: String,
    pub files_updated: Vec<String>,
    pub links_rewritten: usize,
}

pub fn rename(vault: &Vault, old: &str, new_title: &str) -> Result<RenameReport> {
    let page = vault
        .resolve_page(old)
        .ok_or_else(|| anyhow!("no page named '{old}'"))?;
    let new_title = fsutil::safe_title(new_title);
    if vault.resolve_page(&new_title).is_some() {
        bail!("a page named '{new_title}' already exists");
    }
    let new_path = page.path.with_file_name(format!("{new_title}.md"));
    let old_stem = page.stem.clone();
    // rewrite links everywhere
    let re = Regex::new(&format!(
        r"(?i)\[\[(?:[^\]\|#]*/)?{}(#[^\]\|]*)?(\|[^\]]*)?\]\]",
        regex::escape(&old_stem)
    ))?;
    let mut files_updated = vec![];
    let mut links = 0;
    for p in &vault.pages {
        let text = std::fs::read_to_string(&p.path)?;
        let n = re.find_iter(&text).count();
        if n == 0 {
            continue;
        }
        let out = re.replace_all(&text, |c: &regex::Captures<'_>| {
            let h = c.get(1).map_or("", |m| m.as_str());
            let a = c.get(2).map_or("", |m| m.as_str());
            format!("[[{new_title}{h}{a}]]")
        });
        fsutil::write_atomic(&p.path, out.as_bytes())?;
        links += n;
        files_updated.push(p.rel.clone());
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
    let h1 = Regex::new(&format!(r"(?m)^# {}\s*$", regex::escape(&old_stem)))?;
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

#[derive(Debug, Serialize)]
pub struct DupeCandidate {
    pub a: String,
    pub b: String,
    pub similarity: f64,
    pub reason: String,
}

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
                    let inter = tx.intersection(&ty).count() as f64;
                    let uni = tx.union(&ty).count() as f64;
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

#[derive(Debug, Serialize)]
pub struct TraceRow {
    pub cells: Vec<String>,
}

/// Follow a chain `type, field, type, field, type` and produce rows. A page
/// with no link in `field` yields a row padded with "—" (a gap).
pub fn trace(vault: &Vault, chain: &[String]) -> Result<(Vec<String>, Vec<TraceRow>)> {
    if chain.len() < 3 || chain.len().is_multiple_of(2) {
        bail!("chain must be type, field, type[, field, type ...]");
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

pub fn pack_trace<'a>(packs: &'a [Pack], name: &str) -> Option<&'a crate::pack::Trace> {
    packs
        .iter()
        .flat_map(|p| p.traces.iter())
        .find(|t| t.name == name)
}

// ---------- archive ----------

pub fn archive(vault: &Vault, target: &str) -> Result<Vec<String>> {
    let page = vault
        .resolve_page(target)
        .ok_or_else(|| anyhow!("no page named '{target}'"))?;
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
        bail!("{} already exists", dest.display());
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

pub fn export(vault: &Vault, targets: &[String], include_confidential: bool) -> Result<String> {
    let excluded: Vec<String> = if include_confidential {
        vec![]
    } else {
        vault.config.privacy.exclude_from_export.clone()
    };
    let mut pages: Vec<&Page> = vec![];
    for t in targets {
        if let Some(p) = vault.resolve_page(t) {
            pages.push(p);
        } else {
            for p in &vault.pages {
                if p.rel.starts_with(t.trim_end_matches('/')) {
                    pages.push(p);
                }
            }
        }
    }
    if pages.is_empty() {
        bail!("nothing matched {targets:?}");
    }
    let mut out = String::new();
    let mut bib: BTreeMap<String, &Page> = BTreeMap::new();
    let mut counter = 0usize;
    for p in pages {
        let conf = p
            .get("confidentiality")
            .unwrap_or_else(|| "personal".into());
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
            if let Some(sp) = vault.resolve_page(&c.source) {
                let sc = sp
                    .get("confidentiality")
                    .unwrap_or_else(|| "personal".into());
                if excluded.contains(&sc) {
                    if let Some(q) = &c.quote {
                        body = body.replace(q, "[quote withheld: confidential source]");
                    }
                } else {
                    bib.insert(c.source.clone(), sp);
                }
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
        for (id, sp) in bib {
            let origin = sp.get("origin").unwrap_or_default();
            let pubd = sp.get("published").unwrap_or_default();
            let auth = sp.get("author").unwrap_or_default();
            let _ = writeln!(
                out,
                "- **{}**{}{}{} — retrieved {}",
                sp.title,
                if auth.is_empty() {
                    String::new()
                } else {
                    format!(", {auth}")
                },
                if pubd.is_empty() {
                    String::new()
                } else {
                    format!(" ({pubd})")
                },
                if origin.is_empty() {
                    String::new()
                } else {
                    format!(", {origin}")
                },
                sp.get("retrieved").unwrap_or_default()
            );
            let _ = id;
        }
    }
    Ok(out)
}

// ---------- status ----------

#[derive(Debug, Serialize, Default)]
pub struct Status {
    pub name: String,
    pub pages: usize,
    pub by_type: BTreeMap<String, usize>,
    pub active_projects: Vec<String>,
    pub projects_without_next_action: Vec<String>,
    pub inbox_items: usize,
    pub sources_total: usize,
    pub sources_unconverted: usize,
    pub sources_unsummarised: usize,
    pub citations_total: usize,
    pub open_questions: usize,
    pub open_contradictions: usize,
    pub journal_last: Option<String>,
    pub packs: Vec<String>,
    pub last_lint: Option<String>,
}

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

pub fn review_packet(
    vault: &Vault,
    persona_text: &str,
    persona: &str,
    targets: &[String],
    context_lines: usize,
) -> Result<String> {
    let mut pages: Vec<&Page> = vec![];
    for t in targets {
        if let Some(p) = vault.resolve_page(t) {
            pages.push(p);
        } else {
            for p in &vault.pages {
                if p.rel.starts_with(t.trim_end_matches('/')) && !p.rel.starts_with("indexes/") {
                    pages.push(p);
                }
            }
        }
    }
    if pages.is_empty() {
        bail!("nothing matched {targets:?}");
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

#[derive(Debug, Serialize)]
pub struct ReviewRecord {
    pub pages_updated: Vec<String>,
    pub findings: usize,
    pub blockers: usize,
}

/// Record persona findings (JSON array) into the reviewed pages' frontmatter and
/// write a review page under reviews/.
pub fn review_record(
    vault: &Vault,
    persona: &str,
    findings_json: &str,
    scope: &str,
    model: Option<&str>,
) -> Result<ReviewRecord> {
    let findings: Vec<serde_json::Value> = serde_json::from_str(findings_json.trim())
        .map_err(|e| anyhow!("findings must be a JSON array: {e}"))?;
    let today = fsutil::today();
    let mut per_page: BTreeMap<String, Vec<&serde_json::Value>> = BTreeMap::new();
    for f in &findings {
        let page = f
            .get("page")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        per_page.entry(page).or_default().push(f);
    }
    let blockers = findings
        .iter()
        .filter(|f| f.get("severity").and_then(|v| v.as_str()) == Some("blocker"))
        .count();
    // review page
    let scope_title = fsutil::safe_title(scope);
    let rel = format!("reviews/{today} {persona} {scope_title}.md");
    let mut fm = serde_yaml::Mapping::new();
    frontmatter::set_str(&mut fm, "type", "review");
    frontmatter::set_str(&mut fm, "persona", persona);
    frontmatter::set_str(&mut fm, "date", &today);
    frontmatter::set_str(&mut fm, "scope", scope);
    if let Some(m) = model {
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
        "# Review: {persona} on {scope}\n\n{} finding(s), {} blocker(s). Resolve each: fix, waive (with reason), or file as an open question.\n\n",
        findings.len(),
        blockers
    );
    for f in &findings {
        let g = |k: &str| f.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
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
    )?;
    // stamp pages
    let mut updated = vec![];
    let all_pages: Vec<&Page> = {
        let mut v = vec![];
        for t in scope.split(',') {
            let t = t.trim();
            if let Some(p) = vault.resolve_page(t) {
                v.push(p);
            } else {
                for p in &vault.pages {
                    if p.rel.starts_with(t.trim_end_matches('/')) && p.ptype != "index" {
                        v.push(p);
                    }
                }
            }
        }
        v
    };
    for p in all_pages {
        let n = per_page.get(&p.rel).map_or(0, std::vec::Vec::len);
        let text = std::fs::read_to_string(&p.path)?;
        let mut doc = frontmatter::parse(&text)?;
        let mut reviews = match doc.fm.get("reviews") {
            Some(serde_yaml::Value::Sequence(s)) => s.clone(),
            _ => vec![],
        };
        let mut entry = serde_yaml::Mapping::new();
        frontmatter::set_str(&mut entry, "persona", persona);
        frontmatter::set_str(&mut entry, "date", &today);
        if let Some(m) = model {
            frontmatter::set_str(&mut entry, "model", m);
        }
        frontmatter::set_str(
            &mut entry,
            "result",
            if n == 0 { "pass" } else { "findings" },
        );
        frontmatter::set_value(&mut entry, "findings", serde_yaml::Value::Number(n.into()));
        frontmatter::set_str(
            &mut entry,
            "review",
            &format!("[[{}]]", fsutil::stem(Path::new(&rel))),
        );
        reviews.push(serde_yaml::Value::Mapping(entry));
        frontmatter::set_value(&mut doc.fm, "reviews", serde_yaml::Value::Sequence(reviews));
        fsutil::write_atomic(&p.path, frontmatter::render(&doc.fm, &doc.body)?.as_bytes())?;
        updated.push(p.rel.clone());
    }
    Ok(ReviewRecord {
        pages_updated: updated,
        findings: findings.len(),
        blockers,
    })
}

// ---------- backlinks / links ----------

#[derive(Debug, Serialize)]
pub struct LinkIssue {
    pub page: String,
    pub line: Option<usize>,
    pub target: String,
    pub suggestions: Vec<String>,
}

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

pub fn backlinks_of(vault: &Vault, target: &str) -> Result<Vec<String>> {
    let crate::vault::Resolution::Found(idx) = vault.resolve(target) else {
        bail!("no page named '{target}'");
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
