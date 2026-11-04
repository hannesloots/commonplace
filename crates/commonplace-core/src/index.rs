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

pub fn generate(vault: &Vault) -> Result<IndexReport> {
    let mut report = IndexReport::default();
    let threshold = vault.config.index.hub_threshold;
    let mut groups: Vec<Group> = vec![];
    let pages = &vault.pages;
    let idx = |pred: &dyn Fn(&Page) -> bool| -> Vec<usize> {
        let mut v: Vec<usize> = pages
            .iter()
            .enumerate()
            .filter(|(_, p)| pred(p) && p.parse_error.is_none())
            .map(|(i, _)| i)
            .collect();
        v.sort_by_key(|&i| pages[i].stem.to_lowercase());
        v
    };
    let is_hub = |p: &Page| {
        p.rel == "index.md"
            || p.rel.starts_with("indexes/")
            || p.ptype == "index"
            || p.ptype == "log"
    };

    groups.push(Group {
        title: "Projects".into(),
        pages: idx(&|p| p.ptype == "project" && !p.is_archived()),
    });
    groups.push(Group {
        title: "Areas".into(),
        pages: idx(&|p| p.ptype == "area" && !p.is_archived()),
    });
    groups.push(Group {
        title: "Memory".into(),
        pages: idx(&|p| p.top_folder() == "memory" && !["person"].contains(&p.ptype.as_str())),
    });
    groups.push(Group {
        title: "People".into(),
        pages: idx(&|p| p.ptype == "person" && !p.is_archived()),
    });
    groups.push(Group {
        title: "Topics".into(),
        pages: idx(&|p| p.ptype == "topic" && !p.is_archived()),
    });
    groups.push(Group {
        title: "Concepts".into(),
        pages: idx(&|p| p.ptype == "concept" || p.ptype == "claim"),
    });
    groups.push(Group {
        title: "Entities".into(),
        pages: idx(&|p| p.ptype == "entity" && !p.is_archived()),
    });
    groups.push(Group {
        title: "Playbooks".into(),
        pages: idx(&|p| p.ptype == "playbook" && !p.is_archived()),
    });
    groups.push(Group {
        title: "Syntheses".into(),
        pages: idx(&|p| p.ptype == "synthesis" && !p.is_archived()),
    });
    groups.push(Group {
        title: "Decisions".into(),
        pages: idx(&|p| p.ptype == "decision" && !p.is_archived()),
    });
    groups.push(Group {
        title: "Open questions".into(),
        pages: idx(&|p| p.ptype == "question" && p.get("status").as_deref() == Some("open")),
    });
    // pack types not covered
    let covered = [
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
    let mut other: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, p) in pages.iter().enumerate() {
        if !covered.contains(&p.ptype.as_str())
            && !p.ptype.is_empty()
            && !is_hub(p)
            && !p.is_archived()
            && p.top_folder() != "memory"
        {
            other.entry(p.ptype.clone()).or_default().push(i);
        }
    }
    for (t, v) in other {
        groups.push(Group {
            title: format!("{} pages", capitalise(&t)),
            pages: v,
        });
    }
    groups.push(Group {
        title: "Sources".into(),
        pages: idx(&|p| p.ptype == "source"),
    });
    groups.push(Group {
        title: "Journal".into(),
        pages: {
            let mut v = idx(&|p| p.ptype == "journal");
            v.reverse();
            v
        },
    });
    groups.push(Group {
        title: "Archive".into(),
        pages: idx(&|p| p.is_archived() && !is_hub(p)),
    });

    let indexes_dir = vault.root.join("indexes");
    let mut body = String::new();
    let _ = write!(
        body,
        "# Index — {}\n\n",
        if vault.config.name.is_empty() {
            "commonplace"
        } else {
            &vault.config.name
        }
    );
    let existing = std::fs::read_to_string(vault.root.join("index.md")).unwrap_or_default();
    let keep = markdown::generated_block(&existing, "keep").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| {
        format!("{}\n\nStart here: [[overview]] for the big picture, [[error-book]] for standing constraints, [[open-questions]] for what is unresolved.", vault.config.purpose)
    });
    body.push_str("<!-- commonplace:keep:start -->\n");
    body.push_str(&keep);
    body.push_str("\n<!-- commonplace:keep:end -->\n\n");
    body.push_str("<!-- Everything below is generated by `commonplace index`. Edit the orientation above, never the lists. -->\n\n");
    let mut wanted_hubs: Vec<String> = vec![];
    for g in &groups {
        if g.pages.is_empty() {
            continue;
        }
        let _ = writeln!(body, "## {} ({})", g.title, g.pages.len());
        if g.pages.len() > threshold
            || g.title == "Archive"
            || (g.title == "Sources" && g.pages.len() > threshold / 2)
        {
            let hub_name = format!("Index - {}", g.title);
            wanted_hubs.push(hub_name.clone());
            let _ = write!(body, "- [[{hub_name}]] — {} pages\n\n", g.pages.len());
            let mut h = String::new();
            let mut fm = serde_yaml::Mapping::new();
            frontmatter::set_str(&mut fm, "type", "index");
            frontmatter::set_str(&mut fm, "group", &g.title);
            frontmatter::set_value(
                &mut fm,
                "page_count",
                serde_yaml::Value::Number(g.pages.len().into()),
            );
            frontmatter::set_str(&mut fm, "updated", &fsutil::today());
            let _ = write!(
                h,
                "# {hub_name}\n\nGenerated by `commonplace index`; do not edit. Up: [[index]]\n\n"
            );
            // group sources by year, others alphabetically with letter headings when large
            if g.title == "Sources" || g.title == "Journal" {
                let mut by_year: BTreeMap<String, Vec<usize>> = BTreeMap::new();
                for &i in &g.pages {
                    let y = pages[i].stem.chars().take(4).collect::<String>();
                    by_year
                        .entry(if y.chars().all(|c| c.is_ascii_digit()) {
                            y
                        } else {
                            "undated".into()
                        })
                        .or_default()
                        .push(i);
                }
                for (y, v) in by_year.iter().rev() {
                    let _ = writeln!(h, "## {y}");
                    for &i in v {
                        h.push_str(&line_for(&pages[i]));
                        h.push('\n');
                    }
                    h.push('\n');
                }
            } else {
                for &i in &g.pages {
                    h.push_str(&line_for(&pages[i]));
                    h.push('\n');
                }
            }
            std::fs::create_dir_all(&indexes_dir)?;
            fsutil::write_atomic(
                &indexes_dir.join(format!("{hub_name}.md")),
                frontmatter::render(&fm, &h)?.as_bytes(),
            )?;
            report.hubs.push(hub_name);
        } else {
            for &i in &g.pages {
                body.push_str(&line_for(&pages[i]));
                body.push('\n');
            }
            body.push('\n');
        }
    }
    // remove stale hubs
    if let Ok(rd) = std::fs::read_dir(&indexes_dir) {
        for e in rd.flatten() {
            let s = fsutil::stem(&e.path());
            if s.starts_with("Index - ") && !wanted_hubs.contains(&s) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
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

    // Maps of content inside project / area / topic pages
    for p in pages {
        if !["project", "area", "topic"].contains(&p.ptype.as_str()) || p.parse_error.is_some() {
            continue;
        }
        let folder = p.folder();
        let mut members: Vec<&Page> = vec![];
        if p.ptype == "topic" {
            let scope = p.get("scope").unwrap_or_default();
            if scope.is_empty() {
                continue;
            }
            for q in pages {
                if std::ptr::eq(p, q) || is_hub(q) {
                    continue;
                }
                let in_scope = if scope.starts_with("topic/") || scope.starts_with('#') {
                    q.tags.iter().any(|t| t == scope.trim_start_matches('#'))
                } else {
                    q.rel.starts_with(scope.trim_end_matches('/'))
                };
                if in_scope {
                    members.push(q);
                }
            }
        } else {
            for q in pages {
                if std::ptr::eq(p, q) || is_hub(q) {
                    continue;
                }
                let in_folder = !folder.is_empty() && q.rel.starts_with(&format!("{folder}/"));
                let points_here = q.fm_links.iter().any(|(k, t)| {
                    ["project", "projects", "area", "up"].contains(&k.as_str())
                        && t.eq_ignore_ascii_case(&p.stem)
                });
                if in_folder || points_here {
                    members.push(q);
                }
            }
        }
        if members.is_empty() && markdown::generated_block(&p.doc.body, "pages").is_none() {
            continue;
        }
        members.sort_by_key(|q| (q.ptype.clone(), q.stem.to_lowercase()));
        let mut block = String::new();
        let mut cur = String::new();
        for q in &members {
            if q.ptype != cur {
                cur = q.ptype.clone();
                let _ = writeln!(block, "**{}**", capitalise(&cur));
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
    Ok(report)
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
