//! Generated navigation: root index.md (L1), hubs for large groups, and the
//! generated page lists inside project / area / topic pages (maps of content).

use crate::frontmatter;
use crate::fsutil;
use crate::markdown;
use crate::vault::{Page, Vault};
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Debug, Serialize, Default)]
pub struct IndexReport {
    pub index_lines: usize,
    pub hubs: Vec<String>,
    pub mocs_updated: Vec<String>,
}

fn line_for(p: &Page) -> String {
    let desc = p
        .get("description")
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| first_sentence(&p.doc.body));
    let mut extra = vec![];
    if let Some(s) = p.get("status")
        && !["mature", "growing", "seed"].contains(&s.as_str())
        && !s.is_empty()
    {
        extra.push(s);
    }
    if let Some(c) = p.get("confidence")
        && (c == "contested" || c == "low")
    {
        extra.push(format!("confidence {c}"));
    }
    let tail = if extra.is_empty() {
        String::new()
    } else {
        format!(" ({})", extra.join(", "))
    };
    let desc: String = desc.chars().take(160).collect();
    let desc = if desc.trim().is_empty() {
        "(no description yet)".to_string()
    } else {
        desc
    };
    format!("- [[{}]] — {}{}", link_target(p), desc.trim(), tail)
}

/// Link by stem unless another page shares the stem, then by path.
fn link_target(p: &Page) -> String {
    p.stem.clone()
}

fn first_sentence(body: &str) -> String {
    let masked = markdown::mask_code(body);
    let masked = markdown::mask_comments(&masked);
    // prefer an explicit TL;DR / Answer section
    let preferred = markdown::section(&masked, "TL;DR")
        .or_else(|| markdown::section(&masked, "Answer"))
        .or_else(|| markdown::section(&masked, "Summary"));
    let text: String = preferred
        .map(std::string::ToString::to_string)
        .filter(|s| markdown::has_prose(s))
        .unwrap_or(masked.clone());
    for l in text.lines() {
        let t = l.trim();
        if t.is_empty()
            || t.starts_with('#')
            || t.starts_with("<!--")
            || t.starts_with("- ")
            || t.starts_with("* ")
            || t.starts_with("[^")
            || t.starts_with('|')
            || t.starts_with("**")
        {
            continue;
        }
        let s: String = t.chars().take(200).collect();
        return s
            .split_inclusive(['.', '!', '?'])
            .next()
            .unwrap_or(&s)
            .to_string();
    }
    String::new()
}

struct Group {
    title: String,
    pages: Vec<usize>,
}

/// Navigation pages, which are never listed as content.
fn is_hub(p: &Page) -> bool {
    p.rel == "index.md" || p.rel.starts_with("indexes/") || p.ptype == "index" || p.ptype == "log"
}

/// Types with their own group (or deliberately unlisted); any other type gets a "<Type> pages" group.
const GROUPED_TYPES: [&str; 24] = [
    "project",
    "area",
    "person",
    "topic",
    "concept",
    "claim",
    "entity",
    "playbook",
    "synthesis",
    "decision",
    "question",
    "source",
    "journal",
    "capture",
    "review",
    "index",
    "log",
    "registry",
    "overview",
    "questions",
    "error-book",
    "profile",
    "preferences",
    "glossary",
];

/// A group of the parseable pages matching `pred`, sorted by stem (case-insensitive).
fn group(title: &str, pages: &[Page], pred: impl Fn(&Page) -> bool) -> Group {
    let mut v: Vec<usize> = pages
        .iter()
        .enumerate()
        .filter(|(_, p)| pred(p) && p.parse_error.is_none())
        .map(|(i, _)| i)
        .collect();
    v.sort_by_key(|&i| pages[i].stem.to_lowercase());
    Group {
        title: title.into(),
        pages: v,
    }
}

/// The sections of index.md, in display order.
fn index_groups(pages: &[Page]) -> Vec<Group> {
    let live = |t: &'static str| move |p: &Page| p.ptype == t && !p.is_archived();
    let mut groups = vec![
        group("Projects", pages, live("project")),
        group("Areas", pages, live("area")),
        group("Memory", pages, |p| {
            p.top_folder() == "memory" && p.ptype != "person"
        }),
        group("People", pages, live("person")),
        group("Topics", pages, live("topic")),
        group("Concepts", pages, |p| {
            p.ptype == "concept" || p.ptype == "claim"
        }),
        group("Entities", pages, live("entity")),
        group("Playbooks", pages, live("playbook")),
        group("Syntheses", pages, live("synthesis")),
        group("Decisions", pages, live("decision")),
        group("Open questions", pages, |p| {
            p.ptype == "question" && p.get("status").as_deref() == Some("open")
        }),
    ];
    // pack types not covered above, one group per type
    let mut other: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, p) in pages.iter().enumerate() {
        if !GROUPED_TYPES.contains(&p.ptype.as_str())
            && !p.ptype.is_empty()
            && !is_hub(p)
            && !p.is_archived()
            && p.top_folder() != "memory"
        {
            other.entry(p.ptype.as_str()).or_default().push(i);
        }
    }
    groups.extend(other.into_iter().map(|(t, v)| Group {
        title: format!("{} pages", capitalise(t)),
        pages: v,
    }));
    groups.push(group("Sources", pages, |p| p.ptype == "source"));
    let mut journal = group("Journal", pages, |p| p.ptype == "journal");
    journal.pages.reverse();
    groups.push(journal);
    groups.push(group("Archive", pages, |p| p.is_archived() && !is_hub(p)));
    groups
}

/// Append one index line per page.
fn push_lines(out: &mut String, pages: &[Page], members: &[usize]) {
    for &i in members {
        out.push_str(&line_for(&pages[i]));
        out.push('\n');
    }
}

/// The title and hand-kept orientation block that open index.md.
fn index_header(vault: &Vault) -> String {
    let name = if vault.config.name.is_empty() {
        "commonplace"
    } else {
        &vault.config.name
    };
    let existing = std::fs::read_to_string(vault.root.join("index.md")).unwrap_or_default();
    let keep = markdown::generated_block(&existing, "keep").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| {
        format!("{}\n\nStart here: [[overview]] for the big picture, [[error-book]] for standing constraints, [[open-questions]] for what is unresolved.", vault.config.purpose)
    });
    let mut body = format!("# Index — {name}\n\n");
    body.push_str("<!-- commonplace:keep:start -->\n");
    body.push_str(&keep);
    body.push_str("\n<!-- commonplace:keep:end -->\n\n");
    body.push_str("<!-- Everything below is generated by `commonplace index`. Edit the orientation above, never the lists. -->\n\n");
    body
}

/// Whether a group is too big (or too archival) to list inline, so it gets its own hub page.
fn needs_hub(g: &Group, threshold: usize) -> bool {
    g.pages.len() > threshold
        || g.title == "Archive"
        || (g.title == "Sources" && g.pages.len() > threshold / 2)
}

/// Write `indexes/<hub_name>.md` listing a group's pages; sources and journals are grouped by year.
fn write_hub(vault: &Vault, hub_name: &str, g: &Group) -> Result<()> {
    let pages = &vault.pages;
    let mut fm = serde_yaml::Mapping::new();
    frontmatter::set_str(&mut fm, "type", "index");
    frontmatter::set_str(&mut fm, "group", &g.title);
    frontmatter::set_value(
        &mut fm,
        "page_count",
        serde_yaml::Value::Number(g.pages.len().into()),
    );
    frontmatter::set_str(&mut fm, "updated", &fsutil::today());
    let mut h =
        format!("# {hub_name}\n\nGenerated by `commonplace index`; do not edit. Up: [[index]]\n\n");
    if g.title == "Sources" || g.title == "Journal" {
        let mut by_year: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for &i in &g.pages {
            let y: String = pages[i].stem.chars().take(4).collect();
            let key = if y.chars().all(|c| c.is_ascii_digit()) {
                y
            } else {
                "undated".into()
            };
            by_year.entry(key).or_default().push(i);
        }
        for (y, v) in by_year.iter().rev() {
            let _ = writeln!(h, "## {y}");
            push_lines(&mut h, pages, v);
            h.push('\n');
        }
    } else {
        push_lines(&mut h, pages, &g.pages);
    }
    let dir = vault.root.join("indexes");
    std::fs::create_dir_all(&dir)?;
    fsutil::write_atomic(
        &dir.join(format!("{hub_name}.md")),
        frontmatter::render(&fm, &h)?.as_bytes(),
    )
}

/// Delete `indexes/Index - *.md` hubs that this run did not write.
fn remove_stale_hubs(vault: &Vault, wanted: &[String]) {
    let Ok(rd) = std::fs::read_dir(vault.root.join("indexes")) else {
        return;
    };
    for e in rd.flatten() {
        let s = fsutil::stem(&e.path());
        if s.starts_with("Index - ") && !wanted.contains(&s) {
            // best effort: a hub that cannot be removed is only stale navigation
            let _ = std::fs::remove_file(e.path());
        }
    }
}

pub fn generate(vault: &Vault) -> Result<IndexReport> {
    let mut report = IndexReport::default();
    let threshold = vault.config.index.hub_threshold;
    let pages = &vault.pages;
    let mut body = index_header(vault);
    for g in index_groups(pages) {
        if g.pages.is_empty() {
            continue;
        }
        let _ = writeln!(body, "## {} ({})", g.title, g.pages.len());
        if needs_hub(&g, threshold) {
            let hub_name = format!("Index - {}", g.title);
            let _ = write!(body, "- [[{hub_name}]] — {} pages\n\n", g.pages.len());
            write_hub(vault, &hub_name, &g)?;
            report.hubs.push(hub_name);
        } else {
            push_lines(&mut body, pages, &g.pages);
            body.push('\n');
        }
    }
    remove_stale_hubs(vault, &report.hubs);
    let mut fm = serde_yaml::Mapping::new();
    frontmatter::set_str(&mut fm, "type", "index");
    frontmatter::set_str(&mut fm, "updated", &fsutil::today());
    frontmatter::set_value(
        &mut fm,
        "page_count",
        serde_yaml::Value::Number(pages.len().into()),
    );
    let text = frontmatter::render(&fm, &body)?;
    report.index_lines = text.lines().count();
    fsutil::write_atomic(&vault.root.join("index.md"), text.as_bytes())?;
    update_maps_of_content(vault, &mut report)?;
    Ok(report)
}

/// The pages a project, area or topic page lists in its generated "pages" block.
///
/// A topic lists pages in its `scope` (a tag such as `topic/x` or `#x`, or a folder prefix);
/// a project or area lists pages in its folder and pages that point to it via
/// `project`/`projects`/`area`/`up`. Returns `None` for a topic without a scope.
fn moc_members<'v>(p: &Page, pages: &'v [Page]) -> Option<Vec<&'v Page>> {
    let candidates = pages.iter().filter(|q| !std::ptr::eq(p, *q) && !is_hub(q));
    if p.ptype == "topic" {
        let scope = p.get("scope").unwrap_or_default();
        if scope.is_empty() {
            return None;
        }
        let by_tag = scope.starts_with("topic/") || scope.starts_with('#');
        return Some(
            candidates
                .filter(|q| {
                    if by_tag {
                        q.tags.iter().any(|t| t == scope.trim_start_matches('#'))
                    } else {
                        q.rel.starts_with(scope.trim_end_matches('/'))
                    }
                })
                .collect(),
        );
    }
    let folder = p.folder();
    Some(
        candidates
            .filter(|q| {
                let in_folder = !folder.is_empty() && q.rel.starts_with(&format!("{folder}/"));
                let points_here = q.fm_links.iter().any(|(k, t)| {
                    ["project", "projects", "area", "up"].contains(&k.as_str())
                        && t.eq_ignore_ascii_case(&p.stem)
                });
                in_folder || points_here
            })
            .collect(),
    )
}

/// Rewrite the generated page list inside project / area / topic pages.
fn update_maps_of_content(vault: &Vault, report: &mut IndexReport) -> Result<()> {
    for p in &vault.pages {
        if !["project", "area", "topic"].contains(&p.ptype.as_str()) || p.parse_error.is_some() {
            continue;
        }
        let Some(mut members) = moc_members(p, &vault.pages) else {
            continue;
        };
        if members.is_empty() && markdown::generated_block(&p.doc.body, "pages").is_none() {
            continue;
        }
        members.sort_by_key(|q| (q.ptype.clone(), q.stem.to_lowercase()));
        let mut block = String::new();
        let mut cur = "";
        for q in &members {
            if q.ptype != cur {
                cur = &q.ptype;
                let _ = writeln!(block, "**{}**", capitalise(cur));
            }
            block.push_str(&line_for(q));
            block.push('\n');
        }
        let new_body = markdown::replace_generated_block(&p.doc.body, "pages", block.trim_end());
        if new_body != p.doc.body {
            fsutil::write_atomic(
                &p.path,
                frontmatter::render(&p.doc.fm, &new_body)?.as_bytes(),
            )?;
            report.mocs_updated.push(p.rel.clone());
        }
    }
    Ok(())
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
