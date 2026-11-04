//! `commonplace init`, `commonplace new`, embedded templates and packs. Templates are
//! compiled into the binary (from skills/commonplace/templates and packs) and copied
//! into `.commonplace/` at init so every commonplace is self-contained and can be
//! customised without touching the skill.

use crate::config::Config;
use crate::frontmatter;
use crate::fsutil;
use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use std::path::{Path, PathBuf};

macro_rules! tpl {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("../../../skills/commonplace/templates/", $name)),
        )
    };
}
macro_rules! pack_file {
    ($pack:literal, $name:literal) => {
        (
            concat!($pack, "/", $name),
            include_str!(concat!(
                "../../../skills/commonplace/packs/",
                $pack,
                "/",
                $name
            )),
        )
    };
}

/// Root-level files written once at init (name, content). `{{...}}` placeholders are filled.
pub const ROOT_FILES: &[(&str, &str)] = &[
    tpl!("AGENTS.md"),
    tpl!("CLAUDE.md"),
    tpl!("TAGS.md"),
    tpl!("overview.md"),
    tpl!("log.md"),
    tpl!("error-book.md"),
    tpl!("open-questions.md"),
    tpl!("gitignore"),
    tpl!("obsidian-app.json"),
];

/// Page templates copied to `.commonplace/templates/<type>.md` and used by `commonplace new`.
pub const PAGE_TEMPLATES: &[(&str, &str)] = &[
    tpl!("concept.md"),
    tpl!("entity.md"),
    tpl!("synthesis.md"),
    tpl!("topic.md"),
    tpl!("playbook.md"),
    tpl!("project.md"),
    tpl!("area.md"),
    tpl!("decision.md"),
    tpl!("person.md"),
    tpl!("journal.md"),
    tpl!("capture.md"),
    tpl!("question.md"),
    tpl!("profile.md"),
    tpl!("preferences.md"),
    tpl!("glossary.md"),
    tpl!("source.md"),
];

/// Memory pages created at init (relative path, template name).
pub const MEMORY_PAGES: &[(&str, &str)] = &[
    ("memory/Profile.md", "profile.md"),
    ("memory/Preferences.md", "preferences.md"),
    ("memory/Glossary.md", "glossary.md"),
];

/// Pack files (pack-relative path, content).
pub const PACK_FILES: &[(&str, &str)] = &[
    pack_file!("ea", "pack.yaml"),
    pack_file!("ea", "templates/capability.md"),
    pack_file!("ea", "templates/use-case.md"),
    pack_file!("ea", "templates/value-stream.md"),
    pack_file!("ea", "templates/abb.md"),
    pack_file!("ea", "templates/sbb.md"),
    pack_file!("ea", "templates/gap.md"),
    pack_file!("ea", "templates/work-package.md"),
    pack_file!("ea", "templates/requirement.md"),
    pack_file!("ea", "templates/principle.md"),
    pack_file!("ea", "templates/stakeholder.md"),
    pack_file!("ea", "templates/driver.md"),
    pack_file!("ea", "templates/architecture-vision.md"),
    pack_file!("ea", "templates/target-architecture.md"),
    pack_file!("ea", "templates/roadmap.md"),
    pack_file!("legal", "pack.yaml"),
    pack_file!("legal", "templates/matter.md"),
    pack_file!("legal", "templates/authority.md"),
    pack_file!("legal", "templates/party.md"),
    pack_file!("legal", "templates/issue.md"),
    pack_file!("legal", "templates/chronology-event.md"),
    pack_file!("legal", "templates/evidence.md"),
    pack_file!("legal", "templates/argument.md"),
    pack_file!("legal", "templates/draft.md"),
    pack_file!("legal", "jurisdictions/za.yaml"),
    pack_file!("legal", "jurisdictions/au.yaml"),
    pack_file!("legal", "jurisdictions/en.yaml"),
    pack_file!("legal", "jurisdictions/generic.yaml"),
    pack_file!("compare", "pack.yaml"),
    pack_file!("compare", "templates/option.md"),
    pack_file!("compare", "templates/needs-profile.md"),
    pack_file!("compare", "templates/scenario.md"),
    pack_file!("compare", "templates/comparison.md"),
    pack_file!("study", "pack.yaml"),
    pack_file!("study", "templates/course.md"),
    pack_file!("study", "templates/module.md"),
    pack_file!("study", "templates/certificate.md"),
    pack_file!("study", "templates/flashcard.md"),
    pack_file!("study", "templates/exam.md"),
];

pub const DIRS: &[&str] = &[
    "inbox",
    "journal",
    "sources/raw",
    "sources/text",
    "sources/notes",
    "projects",
    "areas",
    "resources/concepts",
    "resources/entities",
    "resources/topics",
    "resources/syntheses",
    "resources/playbooks",
    "memory/people",
    "memory/decisions",
    "questions",
    "reviews",
    "archive",
    "indexes",
    ".obsidian",
];

pub fn known_packs() -> Vec<String> {
    let mut v: Vec<String> = PACK_FILES
        .iter()
        .map(|(p, _)| p.split_once('/').map_or(*p, |(pack, _)| pack).to_string())
        .collect();
    v.dedup();
    v
}

/// Map legacy pack names to current ones: `sa-legal` is `legal` with jurisdiction `za`; `togaf` is `ea`.
pub fn normalise_pack(name: &str) -> (String, Option<&'static str>) {
    match name.trim() {
        "sa-legal" => ("legal".to_string(), Some("za")),
        "togaf" => ("ea".to_string(), None),
        n => (n.to_string(), None),
    }
}

/// Jurisdiction profile codes shipped with a pack (excluding `generic`).
pub fn known_jurisdictions(pack: &str) -> Vec<String> {
    let prefix = format!("{pack}/jurisdictions/");
    let mut v: Vec<String> = PACK_FILES
        .iter()
        .filter_map(|(p, _)| p.strip_prefix(&prefix))
        .map(|f| f.trim_end_matches(".yaml").to_string())
        .filter(|c| c != "generic")
        .collect();
    v.sort();
    v
}

#[derive(Debug, Serialize, Default)]
pub struct InitReport {
    pub root: String,
    pub created: Vec<String>,
    pub skipped: Vec<String>,
    pub packs: Vec<String>,
}

fn fill(s: &str, name: &str, purpose: &str, title: &str) -> String {
    s.replace("{{name}}", name)
        .replace("{{purpose}}", purpose)
        .replace("{{date}}", &fsutil::today())
        .replace("{{title}}", title)
        .replace("{{year}}", &fsutil::today()[..4])
}

pub fn init(
    root: &Path,
    name: &str,
    purpose: &str,
    packs: &[String],
    jurisdiction: &str,
    force: bool,
) -> Result<InitReport> {
    let mut rep = InitReport {
        root: root.display().to_string(),
        ..Default::default()
    };
    std::fs::create_dir_all(root)?;
    if crate::config::config_path(root).exists() && !force {
        bail!(
            "{} already has a commonplace.toml (use --force to re-apply templates; existing pages are never overwritten)",
            root.display()
        );
    }
    for d in DIRS {
        std::fs::create_dir_all(root.join(d))?;
    }
    let tool = crate::vault::tool_dir(root);
    std::fs::create_dir_all(tool.join("templates"))?;
    std::fs::create_dir_all(tool.join("packs"))?;
    let mut cfg = if crate::config::config_path(root).exists() {
        Config::load(root)?
    } else {
        Config::default()
    };
    if !name.is_empty() {
        cfg.name = name.to_string();
    }
    if !purpose.is_empty() {
        cfg.purpose = purpose.to_string();
    }
    for raw in packs {
        let (p, jur) = normalise_pack(raw);
        if !known_packs().contains(&p) {
            bail!(
                "unknown pack '{p}'; known packs: {}",
                known_packs().join(", ")
            );
        }
        if !cfg.packs.contains(&p) {
            cfg.packs.push(p.clone());
        }
        if let (Some(j), true) = (jur, cfg.jurisdiction.is_empty()) {
            cfg.jurisdiction = j.to_string();
        }
    }
    if !jurisdiction.is_empty() {
        cfg.jurisdiction = jurisdiction.to_string();
    }
    cfg.save(root)?;
    rep.created.push("commonplace.toml".into());
    for (fname, content) in ROOT_FILES {
        let target = match *fname {
            "gitignore" => ".gitignore".to_string(),
            "obsidian-app.json" => ".obsidian/app.json".to_string(),
            other => other.to_string(),
        };
        let text = fill(content, &cfg.name, &cfg.purpose, "");
        let path = root.join(&target);
        if *fname == "AGENTS.md" && path.exists() {
            // upgrade: replace standard part, keep local rules
            let old = std::fs::read_to_string(&path)?;
            let merged = merge_local_rules(&text, &old);
            fsutil::write_atomic(&path, merged.as_bytes())?;
            rep.created.push(format!(
                "{target} (standard sections refreshed, local rules kept)"
            ));
            continue;
        }
        if fsutil::write_if_absent(&path, &text)? {
            rep.created.push(target);
        } else {
            rep.skipped.push(target);
        }
    }
    for (fname, content) in PAGE_TEMPLATES {
        fsutil::write_atomic(
            &crate::vault::tool_dir(root).join("templates").join(fname),
            content.as_bytes(),
        )?;
    }
    rep.created.push(".commonplace/templates/*".into());
    for (rel, tname) in MEMORY_PAGES {
        let content = PAGE_TEMPLATES
            .iter()
            .find(|(n, _)| n == tname)
            .map_or("", |(_, c)| *c);
        let title = fsutil::stem(Path::new(rel));
        if fsutil::write_if_absent(
            &root.join(rel),
            &fill(content, &cfg.name, &cfg.purpose, &title),
        )? {
            rep.created.push(rel.to_string());
        }
    }
    for p in &cfg.packs {
        install_pack(root, p)?;
        rep.packs.push(p.clone());
    }
    // prove every pack loads with this configuration (unknown jurisdiction fails here, not at lint time)
    crate::pack::load_packs(
        &crate::vault::tool_dir(root).join("packs"),
        &cfg.packs,
        &cfg.jurisdiction,
    )?;
    crate::manifest::Manifest::load(root)?.save(root)?;
    Ok(rep)
}

pub fn install_pack(root: &Path, pack: &str) -> Result<()> {
    let (pack, _) = normalise_pack(pack);
    for (rel, content) in PACK_FILES {
        if rel.starts_with(&format!("{pack}/")) {
            let path = crate::vault::tool_dir(root).join("packs").join(rel);
            fsutil::write_atomic(&path, content.as_bytes())?;
        }
    }
    Ok(())
}

/// Replace everything outside the LOCAL-RULES markers with the new template.
fn merge_local_rules(new: &str, old: &str) -> String {
    let (s, e) = ("<!-- LOCAL-RULES-START -->", "<!-- LOCAL-RULES-END -->");
    let old_local = match (old.find(s), old.find(e)) {
        (Some(a), Some(b)) if b > a => Some(&old[a..b + e.len()]),
        _ => None,
    };
    match (old_local, new.find(s), new.find(e)) {
        (Some(local), Some(a), Some(b)) if b > a => {
            format!("{}{}{}", &new[..a], local, &new[b + e.len()..])
        }
        _ => new.to_string(),
    }
}

#[derive(Debug, Serialize)]
pub struct NewReport {
    pub path: String,
    #[serde(rename = "type")]
    pub ptype: String,
    pub title: String,
}

/// Default folder for a core type. Pack types use their `folder_hint` (or --folder).
pub fn default_folder(ptype: &str, title: &str) -> Option<String> {
    let slug = fsutil::slug(title, 60);
    Some(match ptype {
        "concept" | "claim" => "resources/concepts".into(),
        "entity" => "resources/entities".into(),
        "synthesis" => "resources/syntheses".into(),
        "topic" => "resources/topics".into(),
        "playbook" => "resources/playbooks".into(),
        "project" => format!("projects/{slug}"),
        "area" => format!("areas/{slug}"),
        "decision" => "memory/decisions".into(),
        "person" => "memory/people".into(),
        "journal" => "journal".into(),
        "capture" => "inbox".into(),
        "question" => "questions".into(),
        "source" => "sources/notes".into(),
        _ => return None,
    })
}

/// The template for `ptype`: the commonplace's own templates first, then each
/// enabled pack's. Also returns the pack type's `folder_hint`, if any.
fn find_template(root: &Path, cfg: &Config, ptype: &str) -> Result<(PathBuf, Option<String>)> {
    let tool = crate::vault::tool_dir(root);
    let own = tool.join("templates").join(format!("{ptype}.md"));
    if own.exists() {
        return Ok((own, None));
    }
    for p in &cfg.packs {
        let cand = tool
            .join("packs")
            .join(p)
            .join("templates")
            .join(format!("{ptype}.md"));
        if cand.exists() {
            let packs = crate::pack::load_packs(
                &tool.join("packs"),
                std::slice::from_ref(p),
                &cfg.jurisdiction,
            )?;
            let hint = packs
                .first()
                .and_then(|pk| pk.types.get(ptype))
                .map(|t| t.folder_hint.clone())
                .filter(|h| !h.is_empty());
            return Ok((cand, hint));
        }
    }
    bail!(
        "no template for type '{ptype}' (core types: {}; pack types come from enabled packs)",
        PAGE_TEMPLATES
            .iter()
            .map(|(n, _)| n.trim_end_matches(".md"))
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// A `--set key=value` value: `[a, "b"]` becomes a list of strings, anything else a string.
fn extra_field_value(v: &str) -> serde_yaml::Value {
    match v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        Some(inner) => serde_yaml::Value::Sequence(
            inner
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
                .map(serde_yaml::Value::String)
                .collect(),
        ),
        None => serde_yaml::Value::String(v.to_string()),
    }
}

pub fn new_page(
    root: &Path,
    cfg: &Config,
    ptype: &str,
    title: &str,
    folder: Option<&str>,
    extra: &[(String, String)],
) -> Result<NewReport> {
    let title = if ptype == "journal" && title.is_empty() {
        fsutil::today()
    } else {
        fsutil::safe_title(title)
    };
    if title.is_empty() {
        bail!("a title is required");
    }
    let (tpl_path, pack_hint) = find_template(root, cfg, ptype)?;
    let folder = match folder {
        Some(f) => f.trim_matches('/').to_string(),
        None => match default_folder(ptype, &title).or(pack_hint) {
            Some(f) if !f.contains('<') => f,
            Some(f) => bail!("type '{ptype}' needs --folder (hint: {f})"),
            None => bail!("type '{ptype}' needs --folder"),
        },
    };
    let dir = root.join(&folder);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{title}.md"));
    if path.exists() {
        bail!("{} already exists", fsutil::rel(root, &path));
    }
    // uniqueness of stem across vault
    for existing in crate::vault::walk_markdown(root) {
        if fsutil::stem(&existing).eq_ignore_ascii_case(&title) {
            bail!(
                "a page named '{title}' already exists at {} (extend it, or pick a distinct title)",
                fsutil::rel(root, &existing)
            );
        }
    }
    let raw = std::fs::read_to_string(&tpl_path)?;
    let text = fill(&raw, &cfg.name, &cfg.purpose, &title);
    let mut doc = frontmatter::parse(&text)?;
    for (k, v) in extra {
        frontmatter::set_value(&mut doc.fm, k, extra_field_value(v));
    }
    frontmatter::set_str(&mut doc.fm, "created", &fsutil::today());
    frontmatter::set_str(&mut doc.fm, "updated", &fsutil::today());
    fsutil::write_atomic(&path, frontmatter::render(&doc.fm, &doc.body)?.as_bytes())?;
    Ok(NewReport {
        path: fsutil::rel(root, &path),
        ptype: ptype.to_string(),
        title,
    })
}

pub fn template_dir(root: &Path) -> PathBuf {
    crate::vault::tool_dir(root).join("templates")
}

pub fn persona_text(
    root: &Path,
    skill_dir: Option<&Path>,
    cfg: &Config,
    name: &str,
) -> Result<String> {
    let mut candidates: Vec<PathBuf> = vec![];
    for p in &cfg.packs {
        candidates.push(
            crate::vault::tool_dir(root)
                .join("packs")
                .join(p)
                .join("personas")
                .join(format!("{name}.md")),
        );
    }
    if let Some(sd) = skill_dir {
        candidates.push(sd.join("references/personas").join(format!("{name}.md")));
        for p in &cfg.packs {
            candidates.push(
                sd.join("packs")
                    .join(p)
                    .join("personas")
                    .join(format!("{name}.md")),
            );
        }
    }
    candidates.push(root.join("reviews/personas").join(format!("{name}.md")));
    for c in &candidates {
        if c.is_file() {
            return Ok(std::fs::read_to_string(c)?);
        }
    }
    Err(anyhow!(
        "persona '{name}' not found (looked in {})",
        candidates
            .iter()
            .map(|c| c.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracketed_extra_field_becomes_a_list() {
        let v = extra_field_value(r#"[ a, "b c" , ]"#);
        let items: Vec<&str> = v
            .as_sequence()
            .expect("a list")
            .iter()
            .filter_map(serde_yaml::Value::as_str)
            .collect();
        assert_eq!(items, ["a", "b c"]);
        assert_eq!(extra_field_value("[").as_str(), Some("["));
        assert_eq!(extra_field_value("plain").as_str(), Some("plain"));
    }

    #[test]
    fn embedded_packs_parse() {
        for (rel, content) in PACK_FILES {
            if rel.ends_with("pack.yaml") {
                let pack: Result<crate::pack::Pack, _> = serde_yaml::from_str(content);
                assert!(pack.is_ok(), "{rel}: {:?}", pack.err());
                let pack = pack.unwrap();
                for r in &pack.rules {
                    assert!(
                        !r.id.is_empty() && !r.check.is_empty(),
                        "{rel}: rule without id/check"
                    );
                }
            }
        }
    }

    #[test]
    fn embedded_templates_have_frontmatter() {
        for (name, content) in PAGE_TEMPLATES
            .iter()
            .chain(PACK_FILES.iter().filter(|(n, _)| n.contains("/templates/")))
        {
            let doc = crate::frontmatter::parse(content).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(doc.has_frontmatter, "{name}: no frontmatter");
            assert!(
                crate::frontmatter::get_str(&doc.fm, "type").is_some(),
                "{name}: no type"
            );
        }
    }
}
