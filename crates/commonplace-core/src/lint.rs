//! Deterministic hygiene checks. Semantic judgement (does the quote support the
//! claim, are two pages really the same concept) is left to persona reviews.

use crate::cite::{self, Verdict};
use crate::frontmatter;
use crate::markdown;
use crate::pack::{self, Pack};
use crate::sources;
use crate::vault::{Resolution, Vault};
use regex::Regex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn parse(s: &str) -> Severity {
        match s.to_ascii_lowercase().as_str() {
            "error" | "err" => Severity::Error,
            "warning" | "warn" => Severity::Warning,
            _ => Severity::Info,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub rule: String,
    pub path: String,
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LintReport {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub pages_checked: usize,
    pub findings: Vec<Finding>,
}

pub struct LintOptions {
    /// Only report findings on these relative paths (None = all).
    pub only: Option<HashSet<String>>,
    pub verify_citations: bool,
    pub rules: Option<Vec<String>>,
}

pub const SYNTHESIS_TYPES: &[&str] = &[
    "concept",
    "entity",
    "synthesis",
    "claim",
    "playbook",
    "decision",
];
pub const STATUS_DEFAULT: &[&str] = &["seed", "growing", "mature", "superseded"];
pub const CONFIDENCE: &[&str] = &["high", "medium", "low", "contested"];
pub const CONFIDENTIALITY: &[&str] = &["public", "personal", "confidential", "privileged"];
pub const AUTHORITY: &[&str] = &["primary", "secondary", "tertiary"];
pub const FIDELITY: &[&str] = &["high", "medium", "low", "none"];

pub struct TypeRules {
    pub required: Vec<&'static str>,
    pub status: Vec<&'static str>,
    pub cited: bool,
}

pub fn core_type(t: &str) -> Option<TypeRules> {
    let base = |req: &[&'static str], status: &[&'static str], cited: bool| {
        Some(TypeRules {
            required: req.to_vec(),
            status: status.to_vec(),
            cited,
        })
    };
    match t {
        "source" => base(
            &[
                "title",
                "kind",
                "raw",
                "status",
                "confidentiality",
                "authority",
                "created",
                "updated",
            ],
            &[
                "registered",
                "converted",
                "summarised",
                "integrated",
                "superseded",
            ],
            false,
        ),
        "concept" | "entity" | "synthesis" | "claim" | "playbook" => base(
            &[
                "tags",
                "status",
                "confidence",
                "description",
                "created",
                "updated",
            ],
            STATUS_DEFAULT,
            true,
        ),
        "decision" => base(
            &["status", "date", "description", "created", "updated"],
            &["proposed", "accepted", "rejected", "superseded"],
            true,
        ),
        "topic" => base(
            &["description", "created", "updated"],
            &["seed", "growing", "mature", "superseded"],
            false,
        ),
        "project" => base(
            &["status", "description", "outcome", "created", "updated"],
            &["active", "paused", "done", "archived"],
            false,
        ),
        "area" => base(
            &["status", "description", "created", "updated"],
            &["active", "dormant", "archived"],
            false,
        ),
        "person" => base(
            &["description", "created", "updated"],
            &["active", "inactive"],
            false,
        ),
        "journal" => base(&["date"], &[], false),
        "capture" => base(&["created"], &["new", "processed"], false),
        "question" => base(
            &["status", "created", "updated"],
            &["open", "answered", "parked"],
            false,
        ),
        "review" => base(&["persona", "date", "scope"], &[], false),
        "index" | "log" | "registry" | "overview" | "questions" | "error-book" | "profile"
        | "preferences" | "glossary" | "moc" => base(&[], &[], false),
        _ => None,
    }
}

static TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*(?:/[a-z0-9]+(?:-[a-z0-9]+)*)?$").unwrap()
});
static DATE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}").unwrap());
static LOG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^## \[\d{4}-\d{2}-\d{2}\] [a-z\-]+ \| .+").unwrap());
static SECRET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:sk-[a-z0-9]{20,}|AKIA[0-9A-Z]{16}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY|ghp_[A-Za-z0-9]{30,}|xox[baprs]-[A-Za-z0-9\-]{10,}|AIza[0-9A-Za-z\-_]{30,})").unwrap()
});
static CURRENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:currently|at present|as of now|the latest|state[- ]of[- ]the[- ]art|right now)\b").unwrap()
});

fn registered_tags(vault: &Vault) -> Option<HashSet<String>> {
    let p = vault.root.join("TAGS.md");
    let text = std::fs::read_to_string(p).ok()?;
    let re = Regex::new(r"(?s)^## Active\s*$(.*?)(?:^## |\z)").unwrap();
    let section = regex::RegexBuilder::new(r"^## Active\s*$(.*?)(?:^## |\z)")
        .multi_line(true)
        .dot_matches_new_line(true)
        .build()
        .unwrap()
        .captures(&text)
        .map(|c| c[1].to_string())
        .or_else(|| re.captures(&text).map(|c| c[1].to_string()))?;
    let item = Regex::new(r"(?m)^\s*-\s*`([^`]+)`").unwrap();
    Some(
        item.captures_iter(&section)
            .map(|c| c[1].to_string())
            .collect(),
    )
}

pub fn lint(vault: &Vault, packs: &[Pack], opts: &LintOptions) -> LintReport {
    let mut f: Vec<Finding> = vec![];
    // rule ids renamed in 0.4.0 (TOG-00n → EA-00n): accept either spelling in disabled_rules
    let disabled: HashSet<String> = vault
        .config
        .lint
        .disabled_rules
        .iter()
        .flat_map(|r| [r.clone(), r.replacen("TOG-", "EA-", 1)])
        .collect();
    let today = crate::fsutil::today();
    let tags = registered_tags(vault);
    let backlinks = vault.backlinks();
    let known_sources: HashSet<String> =
        sources::known_source_ids(&vault.root).into_iter().collect();
    let hubs: HashSet<usize> = vault
        .pages
        .iter()
        .enumerate()
        .filter(|(_, p)| p.ptype == "index" || p.ptype == "log" || p.rel == "index.md")
        .map(|(i, _)| i)
        .collect();
    let pack_types: HashMap<&str, &pack::TypeSpec> = packs
        .iter()
        .flat_map(|p| p.types.iter().map(|(k, v)| (k.as_str(), v)))
        .collect();
    let conf_rank = |c: &str| CONFIDENTIALITY.iter().position(|x| *x == c).unwrap_or(1);

    // duplicate names/aliases
    let mut names: HashMap<String, Vec<String>> = HashMap::new();
    for p in &vault.pages {
        names
            .entry(p.stem.to_lowercase())
            .or_default()
            .push(p.rel.clone());
        for a in &p.aliases {
            names
                .entry(a.to_lowercase())
                .or_default()
                .push(p.rel.clone());
        }
    }
    for (name, rels) in &names {
        let uniq: HashSet<&String> = rels.iter().collect();
        if uniq.len() > 1 {
            let mut v: Vec<&String> = uniq.into_iter().collect();
            v.sort();
            f.push(Finding { severity: Severity::Warning, rule: "DUP-001".into(), path: v[0].clone(), line: None, message: format!("name or alias '{name}' is used by more than one page ({}); merge or disambiguate", v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")) });
        }
    }

    for (i, p) in vault.pages.iter().enumerate() {
        let rel = p.rel.clone();
        let off = p.doc.body_line_offset;
        let bl = |l: usize| Some(l + off);
        let mut push = |sev: Severity, rule: &str, line: Option<usize>, msg: String| {
            f.push(Finding {
                severity: sev,
                rule: rule.into(),
                path: rel.clone(),
                line,
                message: msg,
            });
        };
        if let Some(e) = &p.parse_error {
            push(
                Severity::Error,
                "FM-001",
                Some(1),
                format!("cannot parse frontmatter: {e}"),
            );
            continue;
        }
        if !p.doc.has_frontmatter {
            push(
                Severity::Error,
                "FM-001",
                Some(1),
                "no frontmatter (every page starts with --- type: ... ---; use `commonplace new`)"
                    .into(),
            );
            continue;
        }
        if p.ptype.is_empty() {
            push(Severity::Error, "FM-002", Some(1), "missing `type`".into());
            continue;
        }
        let core = core_type(&p.ptype);
        let pt = pack_types.get(p.ptype.as_str());
        if core.is_none() && pt.is_none() {
            push(
                Severity::Warning,
                "FM-003",
                Some(1),
                format!(
                    "unknown type '{}' (not a core type or declared by an enabled pack)",
                    p.ptype
                ),
            );
        }
        let mut required: Vec<String> = core
            .as_ref()
            .map(|c| c.required.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default();
        let mut status_vals: Vec<String> = core
            .as_ref()
            .map(|c| c.status.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default();
        let mut cited = core.as_ref().map(|c| c.cited).unwrap_or(false);
        if let Some(t) = pt {
            required.extend(t.required.iter().cloned());
            if !t.status.is_empty() {
                status_vals = t.status.clone();
            }
            if t.cited {
                cited = true;
                for r in [
                    "tags",
                    "status",
                    "confidence",
                    "description",
                    "created",
                    "updated",
                ] {
                    if !required.iter().any(|x| x == r) {
                        required.push(r.into());
                    }
                }
            }
        }
        for r in &required {
            match p.fm().get(r.as_str()) {
                None => push(
                    Severity::Error,
                    "FM-004",
                    Some(1),
                    format!("missing required property '{r}' for type '{}'", p.ptype),
                ),
                Some(v)
                    if matches!(v, serde_yaml::Value::Null)
                        && !["text", "origin", "author", "publisher", "description"]
                            .contains(&r.as_str()) =>
                {
                    push(
                        Severity::Error,
                        "FM-004",
                        Some(1),
                        format!("property '{r}' is empty"),
                    )
                }
                _ => {}
            }
        }
        // enums
        if let Some(s) = p.get("status") {
            if !status_vals.is_empty() && !status_vals.iter().any(|v| v == &s) {
                push(
                    Severity::Error,
                    "FM-005",
                    Some(1),
                    format!(
                        "status '{s}' is not one of {} for type '{}'",
                        status_vals.join("|"),
                        p.ptype
                    ),
                );
            }
        }
        for (field, vals) in [
            ("confidence", CONFIDENCE),
            ("confidentiality", CONFIDENTIALITY),
            ("authority", AUTHORITY),
            ("fidelity", FIDELITY),
        ] {
            if let Some(v) = p.get(field) {
                if !v.is_empty() && !vals.contains(&v.as_str()) {
                    push(
                        Severity::Error,
                        "FM-005",
                        Some(1),
                        format!("{field} '{v}' is not one of {}", vals.join("|")),
                    );
                }
            }
        }
        if let Some(t) = pt {
            for (field, vals) in &t.enums {
                if let Some(v) = p.get(field) {
                    if !v.is_empty() && !vals.iter().any(|x| x == &v) {
                        push(
                            Severity::Error,
                            "FM-005",
                            Some(1),
                            format!("{field} '{v}' is not one of {}", vals.join("|")),
                        );
                    }
                }
            }
            for (field, spec) in &t.links {
                let targets = p.list(field);
                let sev = spec
                    .severity
                    .as_deref()
                    .map(Severity::parse)
                    .unwrap_or(Severity::Error);
                if targets.len() < spec.min {
                    push(
                        sev.clone(),
                        &format!(
                            "{}-LINK",
                            packs
                                .iter()
                                .find(|pk| pk.types.contains_key(&p.ptype))
                                .map(|pk| pk.name.to_uppercase())
                                .unwrap_or_default()
                        ),
                        Some(1),
                        format!(
                            "'{field}' needs at least {} link(s) to {} ({})",
                            spec.min,
                            spec.to.join("|"),
                            spec.description
                        ),
                    );
                }
                for tgt in targets {
                    for t in markdown::wikilink_targets_in(&tgt) {
                        if let Some(tp) = vault.resolve_page(&t) {
                            if !spec.to.is_empty() && !spec.to.iter().any(|x| x == &tp.ptype) {
                                push(
                                    Severity::Warning,
                                    "LINK-004",
                                    Some(1),
                                    format!(
                                        "'{field}' links to [[{t}]] of type '{}', expected {}",
                                        tp.ptype,
                                        spec.to.join("|")
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }
        // dates
        for d in ["created", "updated", "date", "published", "retrieved"] {
            if let Some(v) = p.get(d) {
                if !v.is_empty() && !DATE_RE.is_match(&v) {
                    push(
                        Severity::Warning,
                        "DATE-001",
                        Some(1),
                        format!("'{d}: {v}' is not an ISO date (YYYY-MM-DD)"),
                    );
                }
            }
        }
        if let (Some(c), Some(u)) = (p.get("created"), p.get("updated")) {
            if u < c {
                push(
                    Severity::Warning,
                    "DATE-002",
                    Some(1),
                    "updated is earlier than created".into(),
                );
            }
        }
        // tags
        let mut topic_count = 0;
        for t in &p.tags {
            if !TAG_RE.is_match(t) {
                push(
                    Severity::Error,
                    "TAG-001",
                    Some(1),
                    format!(
                        "tag '{t}' has the wrong format (lowercase kebab, optional one namespace: topic/name)"
                    ),
                );
            } else if let Some(reg) = &tags {
                if !reg.contains(t) {
                    push(
                        Severity::Error,
                        "TAG-002",
                        Some(1),
                        format!("tag '{t}' is not registered in TAGS.md (propose it there first)"),
                    );
                }
            }
            if t.starts_with("topic/") {
                topic_count += 1;
            }
        }
        if topic_count > 5 {
            push(
                Severity::Warning,
                "TAG-003",
                Some(1),
                format!("{topic_count} topic tags; keep to the 5 most specific"),
            );
        }
        // links
        let all_targets: Vec<(String, Option<usize>)> = p
            .body_links
            .iter()
            .map(|l| (l.target.clone(), bl(l.line)))
            .chain(p.fm_links.iter().map(|(_, t)| (t.clone(), Some(1))))
            .collect();
        for (t, line) in &all_targets {
            match vault.resolve(t) {
                Resolution::Found(_) => {}
                Resolution::Ambiguous(ix) => push(
                    Severity::Warning,
                    "LINK-002",
                    *line,
                    format!(
                        "[[{t}]] is ambiguous: {}; link with the folder path",
                        ix.iter()
                            .map(|&j| vault.pages[j].rel.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ),
                Resolution::Missing => {
                    if known_sources.contains(t) {
                        continue;
                    }
                    let sugg = vault.suggest(t, 3);
                    let hint = if sugg.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "; did you mean {}?",
                            sugg.iter()
                                .map(|(s, _)| format!("[[{s}]]"))
                                .collect::<Vec<_>>()
                                .join(" or ")
                        )
                    };
                    push(
                        Severity::Error,
                        "LINK-001",
                        *line,
                        format!("broken link [[{t}]]{hint}"),
                    );
                }
            }
        }
        // orphans (synthesis + topic + person) — links from index/log/hubs don't count
        if SYNTHESIS_TYPES.contains(&p.ptype.as_str())
            || p.ptype == "topic"
            || p.ptype == "person"
            || pt.map(|t| t.cited).unwrap_or(false)
        {
            let inbound = backlinks
                .get(&i)
                .map(|v| v.iter().filter(|j| !hubs.contains(j)).count())
                .unwrap_or(0);
            if inbound == 0 && !p.is_archived() {
                push(Severity::Warning, "LINK-003", None, "orphan: no inbound links from content pages (link it from a topic, project or related page)".into());
            }
        }
        // supersession and contradictions
        if p.get("status").as_deref() == Some("superseded")
            && p.get("superseded_by").map(|s| s.is_empty()).unwrap_or(true)
        {
            push(
                Severity::Warning,
                "SUP-001",
                Some(1),
                "status is superseded but `superseded_by` is empty".into(),
            );
        }
        if let Some(serde_yaml::Value::Sequence(cs)) = p.fm().get("contradictions") {
            for c in cs {
                if let serde_yaml::Value::Mapping(m) = c {
                    let sev = frontmatter::get_str(m, "severity").unwrap_or_else(|| "soft".into());
                    let st = frontmatter::get_str(m, "status").unwrap_or_else(|| "open".into());
                    let with = frontmatter::get_str(m, "with").unwrap_or_default();
                    if !["soft", "scope", "hard"].contains(&sev.as_str()) {
                        push(
                            Severity::Error,
                            "FM-005",
                            Some(1),
                            format!("contradiction severity '{sev}' is not soft|scope|hard"),
                        );
                    }
                    if st == "open" {
                        let s = if sev == "hard" {
                            Severity::Error
                        } else {
                            Severity::Warning
                        };
                        push(
                            s,
                            "CON-001",
                            Some(1),
                            format!(
                                "open {sev} contradiction with {with}; resolve it or record why it stays open"
                            ),
                        );
                    }
                }
            }
        }
        // body checks
        let body = &p.doc.body;
        let masked = markdown::mask_code(body);
        if let Some(m) = SECRET_RE.find(&masked) {
            push(
                Severity::Error,
                "SEC-001",
                bl(masked[..m.start()].matches('\n').count() + 1),
                "looks like a credential or private key; remove it".into(),
            );
        }
        for term in &vault.config.lint.retired_terms {
            if term.is_empty() || p.get("status").as_deref() == Some("superseded") {
                continue;
            }
            if let Some(pos) = masked.to_lowercase().find(&term.to_lowercase()) {
                push(
                    Severity::Warning,
                    "TERM-001",
                    bl(masked[..pos].matches('\n').count() + 1),
                    format!(
                        "retired term '{term}' still appears; update to the current term or mark the page superseded"
                    ),
                );
            }
        }
        let words = markdown::word_count(body);
        if words > vault.config.lint.max_words
            && ![
                "source",
                "journal",
                "log",
                "index",
                "review",
                "deliverable",
                "draft",
                "comparison",
            ]
            .contains(&p.ptype.as_str())
        {
            push(
                Severity::Warning,
                "SIZE-001",
                None,
                format!(
                    "{words} words; split into atomic pages (limit {})",
                    vault.config.lint.max_words
                ),
            );
        }
        if cited && !markdown::has_prose(body) {
            push(
                Severity::Info,
                "EMPTY-001",
                None,
                "stub: headings only, no content".into(),
            );
        }
        if let Some(u) = p.get("updated") {
            if vault.config.lint.stale_days > 0
                && cited
                && p.get("status").as_deref() == Some("mature")
            {
                if let (Ok(ud), Ok(td)) = (
                    chrono::NaiveDate::parse_from_str(&u[..u.len().min(10)], "%Y-%m-%d"),
                    chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d"),
                ) {
                    let age = (td - ud).num_days();
                    if age > vault.config.lint.stale_days as i64 {
                        push(
                            Severity::Info,
                            "STALE-001",
                            Some(1),
                            format!(
                                "not updated for {age} days; re-check against newer sources or confirm `updated`"
                            ),
                        );
                    }
                    if age > 90 && CURRENT_RE.is_match(&masked) {
                        push(
                            Severity::Warning,
                            "STALE-002",
                            Some(1),
                            format!("says 'currently/latest' but was last updated {age} days ago"),
                        );
                    }
                }
            }
        }
        if p.ptype == "project"
            && p.get("status").as_deref() == Some("active")
            && p.list("next_actions").is_empty()
            && markdown::section(body, "Next actions")
                .map(|s| !markdown::has_prose(s))
                .unwrap_or(true)
        {
            push(Severity::Info, "PROJ-001", Some(1), "active project without next actions (GTD: every live project has a next action, or park it)".into());
        }
        // sources and citations
        if p.ptype == "source" {
            if let Some(raw) = p.get("raw") {
                if !raw.is_empty() && !vault.root.join(&raw).exists() {
                    push(
                        Severity::Error,
                        "SRC-001",
                        Some(1),
                        format!("raw original '{raw}' is missing"),
                    );
                }
            }
            let st = p.get("status").unwrap_or_default();
            if st != "registered" {
                if sources::text_layer_path(&vault.root, &p.stem).is_none() {
                    push(
                        Severity::Error,
                        "SRC-002",
                        Some(1),
                        format!(
                            "no text layer for this source (expected sources/text/{}.text.md or a folder); status '{st}' claims one",
                            p.stem
                        ),
                    );
                }
            }
            if st == "registered" || st == "converted" {
                push(Severity::Info, "SRC-004", Some(1), "source not yet summarised (write TL;DR, summary and key quotes, then set status: summarised)".into());
            }
        }
        if cited || p.ptype == "topic" || p.ptype == "synthesis" || pt.is_some() {
            let (cites, _plain) = cite::parse_citations(body);
            let refs = markdown::footnote_refs(body);
            let defs = markdown::footnote_defs(body);
            let def_labels: HashSet<&str> = defs.iter().map(|d| d.label.as_str()).collect();
            let ref_labels: HashSet<&str> = refs.iter().map(|(l, _)| l.as_str()).collect();
            for (l, line) in &refs {
                if !def_labels.contains(l.as_str()) {
                    push(
                        Severity::Error,
                        "CITE-010",
                        bl(*line),
                        format!("footnote [^{l}] has no definition"),
                    );
                }
            }
            for d in &defs {
                if !ref_labels.contains(d.label.as_str()) {
                    push(
                        Severity::Warning,
                        "CITE-011",
                        bl(d.line),
                        format!("footnote definition [^{}] is never referenced", d.label),
                    );
                }
            }
            if cited
                && markdown::has_prose(body)
                && cites.is_empty()
                && p.list("sources").is_empty()
                && p.get("status").as_deref() != Some("superseded")
            {
                let honest = masked.contains("unsourced:")
                    && matches!(
                        p.get("confidence").as_deref(),
                        Some("low") | Some("contested")
                    );
                if honest {
                    push(Severity::Info, "CITE-012", None, "unsourced page (marked honestly); ingest a source and cite when one exists".into());
                } else {
                    push(Severity::Warning, "CITE-012", None, "no sources and no citations: either cite (footnote with [[source]] locator \"quote\") or mark claims `unsourced:` with confidence: low".into());
                }
            }
            // declared sources must exist
            for s in p.list("sources") {
                for t in markdown::wikilink_targets_in(&s) {
                    if !known_sources.contains(&t) && vault.resolve_page(&t).is_none() {
                        push(
                            Severity::Error,
                            "CITE-013",
                            Some(1),
                            format!("sources lists [[{t}]] but no such source note exists"),
                        );
                    }
                }
            }
            if opts.verify_citations {
                let my_conf = p
                    .get("confidentiality")
                    .unwrap_or_else(|| "personal".into());
                for c in &cites {
                    let line = bl(c.line);
                    if !known_sources.contains(&c.source) {
                        match vault.resolve_page(&c.source) {
                            Some(tp) if tp.ptype != "source" => push(
                                Severity::Warning,
                                "CITE-002",
                                line,
                                format!(
                                    "[^{}] cites [[{}]], which is a {} page, not a source; cite the underlying source",
                                    c.label, c.source, tp.ptype
                                ),
                            ),
                            _ => push(
                                Severity::Error,
                                "CITE-001",
                                line,
                                format!("[^{}] cites unknown source [[{}]]", c.label, c.source),
                            ),
                        }
                        continue;
                    }
                    if let Some(sp) = vault.resolve_page(&c.source) {
                        let sc = sp
                            .get("confidentiality")
                            .unwrap_or_else(|| "personal".into());
                        if conf_rank(&sc) > conf_rank(&my_conf) {
                            push(
                                Severity::Warning,
                                "PRIV-001",
                                line,
                                format!(
                                    "[^{}] quotes a {sc} source into a {my_conf} page; raise this page's confidentiality or remove the quote",
                                    c.label
                                ),
                            );
                        }
                    }
                    match sources::load_text_layer(&vault.root, &c.source) {
                        Ok(Some(tl)) => {
                            let (v, found, _near, msg) = cite::verify_quote(
                                &tl.text,
                                c,
                                vault.config.lint.quote_min_words,
                                vault.config.lint.quote_max_words,
                            );
                            let where_ = found.map(|l| tl.locate(l)).unwrap_or_default();
                            match v {
                                Verdict::Pass => {}
                                Verdict::QuoteNotFound => push(
                                    Severity::Error,
                                    "CITE-003",
                                    line,
                                    format!(
                                        "[^{}] quote not found in [[{}]]: {msg}",
                                        c.label, c.source
                                    ),
                                ),
                                Verdict::QuoteNotInSpan => push(
                                    Severity::Error,
                                    "CITE-004",
                                    line,
                                    format!(
                                        "[^{}] {msg} (found at {where_}); fix the locator",
                                        c.label
                                    ),
                                ),
                                Verdict::SpanNotFound => push(
                                    Severity::Warning,
                                    "CITE-005",
                                    line,
                                    format!("[^{}] {msg}", c.label),
                                ),
                                Verdict::NoQuote => push(
                                    Severity::Warning,
                                    "CITE-006",
                                    line,
                                    format!(
                                        "[^{}] has no verbatim quote; add one so it can be verified",
                                        c.label
                                    ),
                                ),
                                Verdict::QuoteTooShort => push(
                                    Severity::Warning,
                                    "CITE-007",
                                    line,
                                    format!("[^{}] {msg}", c.label),
                                ),
                                Verdict::QuoteTooLong => push(
                                    Severity::Warning,
                                    "CITE-008",
                                    line,
                                    format!("[^{}] {msg}; quote less, cite a span", c.label),
                                ),
                                Verdict::SourceMissing | Verdict::TextLayerMissing => push(
                                    Severity::Warning,
                                    "CITE-009",
                                    line,
                                    format!(
                                        "[^{}] source has no text layer; cannot verify",
                                        c.label
                                    ),
                                ),
                            }
                        }
                        _ => push(
                            Severity::Warning,
                            "CITE-009",
                            line,
                            format!(
                                "[^{}] source [[{}]] has no text layer; cannot verify the quote",
                                c.label, c.source
                            ),
                        ),
                    }
                }
            }
        }
        if p.rel == "log.md" {
            for (n, l) in body.lines().enumerate() {
                if l.starts_with("## ") && !LOG_RE.is_match(l) {
                    push(
                        Severity::Warning,
                        "LOG-001",
                        Some(n + 1 + off),
                        "log entry heading should be '## [YYYY-MM-DD] op | Title'".into(),
                    );
                }
            }
        }
        // pack rules
        for pk in packs {
            for r in &pk.rules {
                if let Some(t) = &r.when.ptype {
                    if t != &p.ptype {
                        continue;
                    }
                }
                if let Some(g) = &r.when.folder {
                    if !pack::glob_match(g, &p.rel) {
                        continue;
                    }
                }
                if let Some(s) = p.get("status") {
                    if r.when.status_not.contains(&s) {
                        continue;
                    }
                }
                let sev = Severity::parse(&r.severity);
                let msg = |extra: &str| {
                    if r.message.is_empty() {
                        extra.to_string()
                    } else if extra.is_empty() {
                        r.message.clone()
                    } else {
                        format!("{} ({extra})", r.message)
                    }
                };
                match r.check.as_str() {
                    "required" => {
                        for fld in r
                            .fields
                            .iter()
                            .chain(std::iter::once(&r.field))
                            .filter(|s| !s.is_empty())
                        {
                            if p.get(fld).map(|v| v.is_empty()).unwrap_or(true)
                                && p.list(fld).is_empty()
                            {
                                push(
                                    sev.clone(),
                                    &r.id,
                                    Some(1),
                                    msg(&format!("missing '{fld}'")),
                                );
                            }
                        }
                    }
                    "enum" => {
                        if let Some(v) = p.get(&r.field) {
                            if !v.is_empty() && !r.values.iter().any(|x| x == &v) {
                                push(
                                    sev.clone(),
                                    &r.id,
                                    Some(1),
                                    msg(&format!(
                                        "{} '{v}' not in {}",
                                        r.field,
                                        r.values.join("|")
                                    )),
                                );
                            }
                        }
                    }
                    "link_field" => {
                        let targets = p.list(&r.field);
                        let min = if r.min == 0 { 1 } else { r.min };
                        if targets.len() < min {
                            push(
                                sev.clone(),
                                &r.id,
                                Some(1),
                                msg(&format!(
                                    "'{}' needs {min}+ link(s) to {}",
                                    r.field,
                                    r.to.join("|")
                                )),
                            );
                        }
                        for t in targets
                            .iter()
                            .flat_map(|t| markdown::wikilink_targets_in(t))
                        {
                            if let Some(tp) = vault.resolve_page(&t) {
                                if !r.to.is_empty() && !r.to.iter().any(|x| x == &tp.ptype) {
                                    push(
                                        Severity::Warning,
                                        &r.id,
                                        Some(1),
                                        msg(&format!(
                                            "'{}' links to [[{t}]] of type '{}'",
                                            r.field, tp.ptype
                                        )),
                                    );
                                }
                            }
                        }
                    }
                    "field_compare" => {
                        if let (Some(a), Some(b)) = (p.get(&r.field), p.get(&r.other)) {
                            let ok = crate::query::eval(
                                &crate::query::Expr::Cmp {
                                    field: r.field.clone(),
                                    op: r.op.clone(),
                                    rhs: crate::query::Rhs::Field(r.other.clone()),
                                },
                                p,
                            );
                            if !ok {
                                push(
                                    sev.clone(),
                                    &r.id,
                                    Some(1),
                                    msg(&format!(
                                        "{}={a} {} {}={b} does not hold",
                                        r.field, r.op, r.other
                                    )),
                                );
                            }
                        }
                    }
                    "forbid_regex" => {
                        if let Ok(re) = Regex::new(&r.pattern) {
                            if let Some(m) = re.find(&masked) {
                                push(
                                    sev.clone(),
                                    &r.id,
                                    bl(masked[..m.start()].matches('\n').count() + 1),
                                    msg(&format!("matched '{}'", m.as_str())),
                                );
                            }
                        }
                    }
                    "require_regex" => {
                        if let Ok(re) = Regex::new(&r.pattern) {
                            if !re.is_match(&masked) {
                                push(sev.clone(), &r.id, None, msg(""));
                            }
                        }
                    }
                    "body_regex_requires_page" => {
                        if let Ok(re) = Regex::new(&r.pattern) {
                            for m in re.find_iter(&masked) {
                                let needle = cite::normalise(m.as_str());
                                let line = bl(masked[..m.start()].matches('\n').count() + 1);
                                let found = vault.pages.iter().find(|q| {
                                    q.ptype == r.page_type
                                        && (cite::normalise(
                                            &q.get(&r.page_field).unwrap_or_default(),
                                        ) == needle
                                            || q.list(&r.page_field)
                                                .iter()
                                                .any(|v| cite::normalise(v) == needle))
                                });
                                match found {
                                    None => push(
                                        sev.clone(),
                                        &r.id,
                                        line,
                                        msg(&format!(
                                            "'{}' has no {} page with {} matching it",
                                            m.as_str(),
                                            r.page_type,
                                            r.page_field
                                        )),
                                    ),
                                    Some(q) => {
                                        for (k, want) in &r.page_must {
                                            let have = q.get(k).unwrap_or_default();
                                            if !have.eq_ignore_ascii_case(want) {
                                                push(
                                                    sev.clone(),
                                                    &r.id,
                                                    line,
                                                    msg(&format!(
                                                        "'{}' -> [[{}]] has {k}={have}, needs {want}",
                                                        m.as_str(),
                                                        q.stem
                                                    )),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    "stale_year" => {
                        if let Some(v) = p.get(&r.field) {
                            let year: i32 = today[..4].parse().unwrap_or(0);
                            if let Ok(y) = v.trim().parse::<i32>() {
                                if y < year - r.years_back as i32 {
                                    push(
                                        sev.clone(),
                                        &r.id,
                                        Some(1),
                                        msg(&format!(
                                            "{}={y} is older than {}",
                                            r.field,
                                            year - r.years_back as i32
                                        )),
                                    );
                                }
                            }
                        }
                    }
                    other => push(
                        Severity::Warning,
                        &r.id,
                        None,
                        format!("pack rule uses unknown check '{other}'"),
                    ),
                }
            }
        }
    }
    // pending raw files with no source note
    let raw_dir = vault.root.join(sources::RAW_DIR);
    if let Ok(rd) = std::fs::read_dir(&raw_dir) {
        let manifest = crate::manifest::Manifest::load(&vault.root).unwrap_or_default();
        let registered: HashSet<String> =
            manifest.sources.values().map(|r| r.raw.clone()).collect();
        for e in rd.flatten() {
            let path = e.path();
            if path.is_file()
                && !path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with('.'))
                    .unwrap_or(false)
            {
                let rel = crate::fsutil::rel(&vault.root, &path);
                if !registered.contains(&rel) {
                    f.push(Finding { severity: Severity::Info, rule: "SRC-003".into(), path: rel, line: None, message: "file in sources/raw not registered (use `commonplace ingest`; never copy files in by hand)".into() });
                }
            }
        }
    }
    // inbox
    if let Ok(rd) = std::fs::read_dir(vault.root.join("inbox")) {
        let n = rd
            .flatten()
            .filter(|e| e.path().is_file() && !e.file_name().to_string_lossy().starts_with('.'))
            .count();
        if n > 0 {
            f.push(Finding {
                severity: Severity::Info,
                rule: "INBOX-001".into(),
                path: "inbox/".into(),
                line: None,
                message: format!("{n} unprocessed item(s) in inbox"),
            });
        }
    }
    // index freshness
    if let Some(idx) = vault.pages.iter().find(|p| p.rel == "index.md") {
        let newest = vault
            .pages
            .iter()
            .filter(|p| p.rel != "index.md" && !p.rel.starts_with("indexes/"))
            .filter_map(|p| p.mtime)
            .max();
        if let (Some(n), Some(im)) = (newest, idx.mtime) {
            if n > im {
                f.push(Finding {
                    severity: Severity::Info,
                    rule: "IDX-001".into(),
                    path: "index.md".into(),
                    line: None,
                    message: "index.md is older than the newest page; run `commonplace index`"
                        .into(),
                });
            }
        }
    } else {
        f.push(Finding {
            severity: Severity::Warning,
            rule: "IDX-002".into(),
            path: "index.md".into(),
            line: None,
            message: "no index.md; run `commonplace index`".into(),
        });
    }

    // filter
    let mut findings: Vec<Finding> = f
        .into_iter()
        .filter(|x| !disabled.contains(&x.rule))
        .collect();
    if let Some(rules) = &opts.rules {
        findings.retain(|x| rules.iter().any(|r| x.rule.starts_with(r)));
    }
    if let Some(only) = &opts.only {
        findings.retain(|x| {
            only.contains(&x.path)
                || only
                    .iter()
                    .any(|o| x.path.starts_with(o.trim_end_matches('/')))
        });
    }
    findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then(a.path.cmp(&b.path))
            .then(a.line.cmp(&b.line))
    });
    let errors = findings
        .iter()
        .filter(|x| x.severity == Severity::Error)
        .count();
    let warnings = findings
        .iter()
        .filter(|x| x.severity == Severity::Warning)
        .count();
    let infos = findings
        .iter()
        .filter(|x| x.severity == Severity::Info)
        .count();
    LintReport {
        errors,
        warnings,
        infos,
        pages_checked: vault.pages.len(),
        findings,
    }
}

/// Run only citation verification, returning per-citation results (used by `commonplace cite verify`).
pub fn verify_all_citations(
    vault: &Vault,
    only: Option<&HashSet<String>>,
) -> Vec<cite::CiteResult> {
    let mut out = vec![];
    let known: HashSet<String> = sources::known_source_ids(&vault.root).into_iter().collect();
    for p in &vault.pages {
        if let Some(o) = only {
            if !o.contains(&p.rel) && !o.iter().any(|x| p.rel.starts_with(x.trim_end_matches('/')))
            {
                continue;
            }
        }
        let (cites, _) = cite::parse_citations(&p.doc.body);
        for c in cites {
            let line = c.line + p.doc.body_line_offset;
            let mut res = cite::CiteResult {
                page: p.rel.clone(),
                line,
                label: c.label.clone(),
                source: c.source.clone(),
                locator: c.locator_raw.clone(),
                verdict: Verdict::SourceMissing,
                found_line: None,
                nearest_lines: vec![],
                message: String::new(),
            };
            if !known.contains(&c.source) {
                res.message = "unknown source".into();
                out.push(res);
                continue;
            }
            match sources::load_text_layer(&vault.root, &c.source) {
                Ok(Some(tl)) => {
                    let (v, found, near, msg) = cite::verify_quote(
                        &tl.text,
                        &c,
                        vault.config.lint.quote_min_words,
                        vault.config.lint.quote_max_words,
                    );
                    res.verdict = v;
                    res.found_line = found;
                    res.nearest_lines = near;
                    res.message = if let Some(l) = found {
                        format!("{msg} [{}]", tl.locate(l))
                    } else {
                        msg
                    };
                }
                _ => {
                    res.verdict = Verdict::TextLayerMissing;
                    res.message = "source has no text layer".into();
                }
            }
            out.push(res);
        }
    }
    out
}
