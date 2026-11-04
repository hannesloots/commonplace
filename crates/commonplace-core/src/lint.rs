//! Deterministic hygiene checks. Semantic judgement (does the quote support the
//! claim, are two pages really the same concept) is left to persona reviews.

use crate::cite::{self, Verdict};
use crate::frontmatter;
use crate::markdown;
use crate::pack::{self, Pack};
use crate::sources;
use crate::vault::{Page, Resolution, Vault};
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

static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| crate::re(r"^[a-z0-9]+(?:-[a-z0-9]+)*(?:/[a-z0-9]+(?:-[a-z0-9]+)*)?$"));
static DATE_RE: LazyLock<Regex> = LazyLock::new(|| crate::re(r"^\d{4}-\d{2}-\d{2}"));
static LOG_RE: LazyLock<Regex> =
    LazyLock::new(|| crate::re(r"^## \[\d{4}-\d{2}-\d{2}\] [a-z\-]+ \| .+"));
static SECRET_RE: LazyLock<Regex> = LazyLock::new(|| {
    crate::re(
        r"(?i)(?:sk-[a-z0-9]{20,}|AKIA[0-9A-Z]{16}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY|ghp_[A-Za-z0-9]{30,}|xox[baprs]-[A-Za-z0-9\-]{10,}|AIza[0-9A-Za-z\-_]{30,})",
    )
});
static CURRENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    crate::re(
        r"(?i)\b(?:currently|at present|as of now|the latest|state[- ]of[- ]the[- ]art|right now)\b",
    )
});

fn registered_tags(vault: &Vault) -> Option<HashSet<String>> {
    let p = vault.root.join("TAGS.md");
    let text = std::fs::read_to_string(p).ok()?;
    let section = crate::re(r"(?ms)^## Active\s*$(.*?)(?:^## |\z)")
        .captures(&text)
        .map(|c| c[1].to_string())?;
    let item = crate::re(r"(?m)^\s*-\s*`([^`]+)`");
    Some(
        item.captures_iter(&section)
            .map(|c| c[1].to_string())
            .collect(),
    )
}

/// Vault-wide data that every page check reads.
struct Context<'a> {
    vault: &'a Vault,
    packs: &'a [Pack],
    verify_citations: bool,
    today: String,
    tags: Option<HashSet<String>>,
    backlinks: HashMap<usize, Vec<usize>>,
    known_sources: HashSet<String>,
    /// Index and log pages: links from these do not rescue an orphan.
    hubs: HashSet<usize>,
    pack_types: HashMap<&'a str, &'a pack::TypeSpec>,
}

impl<'a> Context<'a> {
    fn new(vault: &'a Vault, packs: &'a [Pack], opts: &LintOptions) -> Self {
        Context {
            vault,
            packs,
            verify_citations: opts.verify_citations,
            today: crate::fsutil::today(),
            tags: registered_tags(vault),
            backlinks: vault.backlinks(),
            known_sources: sources::known_source_ids(&vault.root).into_iter().collect(),
            hubs: vault
                .pages
                .iter()
                .enumerate()
                .filter(|(_, p)| p.ptype == "index" || p.ptype == "log" || p.rel == "index.md")
                .map(|(i, _)| i)
                .collect(),
            pack_types: packs
                .iter()
                .flat_map(|p| p.types.iter().map(|(k, v)| (k.as_str(), v)))
                .collect(),
        }
    }
}

/// Rank of a confidentiality level (higher = more restricted); unknown values rank as `personal`.
fn confidentiality_rank(level: &str) -> usize {
    CONFIDENTIALITY
        .iter()
        .position(|x| *x == level)
        .unwrap_or(1)
}

/// The rules for a page type after merging the core type with a pack type.
struct EffectiveType {
    required: Vec<String>,
    status: Vec<String>,
    cited: bool,
}

impl EffectiveType {
    fn merge(core: Option<&TypeRules>, pack_type: Option<&pack::TypeSpec>) -> Self {
        let to_strings = |xs: &[&str]| xs.iter().map(ToString::to_string).collect::<Vec<_>>();
        let mut required = core.map(|c| to_strings(&c.required)).unwrap_or_default();
        let mut status = core.map(|c| to_strings(&c.status)).unwrap_or_default();
        let mut cited = core.is_some_and(|c| c.cited);
        if let Some(t) = pack_type {
            required.extend(t.required.iter().cloned());
            if !t.status.is_empty() {
                status.clone_from(&t.status);
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
        EffectiveType {
            required,
            status,
            cited,
        }
    }
}

/// The checks for one page; findings are appended in rule order.
struct PageCheck<'c, 'a> {
    cx: &'c Context<'a>,
    page: &'a Page,
    index: usize,
    pack_type: Option<&'a pack::TypeSpec>,
    /// The body with code masked out, so rules do not fire on examples.
    masked: String,
    findings: Vec<Finding>,
}

impl<'c, 'a> PageCheck<'c, 'a> {
    fn new(cx: &'c Context<'a>, index: usize, page: &'a Page) -> Self {
        PageCheck {
            cx,
            page,
            index,
            pack_type: cx.pack_types.get(page.ptype.as_str()).copied(),
            masked: markdown::mask_code(&page.doc.body),
            findings: vec![],
        }
    }

    fn push(&mut self, severity: Severity, rule: &str, line: Option<usize>, message: String) {
        self.findings.push(Finding {
            severity,
            rule: rule.into(),
            path: self.page.rel.clone(),
            line,
            message,
        });
    }

    /// Convert a body line number to a file line number.
    fn body_line(&self, line: usize) -> usize {
        line + self.page.doc.body_line_offset
    }

    /// The file line of a byte offset into the masked body.
    fn masked_line(&self, byte: usize) -> usize {
        self.body_line(self.masked[..byte].matches('\n').count() + 1)
    }

    fn run(mut self) -> Vec<Finding> {
        if !self.check_parseable() {
            return self.findings;
        }
        let core = core_type(&self.page.ptype);
        if core.is_none() && self.pack_type.is_none() {
            self.push(
                Severity::Warning,
                "FM-003",
                Some(1),
                format!(
                    "unknown type '{}' (not a core type or declared by an enabled pack)",
                    self.page.ptype
                ),
            );
        }
        let ty = EffectiveType::merge(core.as_ref(), self.pack_type);
        self.check_required(&ty.required);
        self.check_enums(&ty.status);
        self.check_pack_links();
        self.check_dates();
        self.check_tags();
        self.check_links();
        self.check_orphan();
        self.check_contradictions();
        self.check_body(ty.cited);
        self.check_source_note();
        if ty.cited
            || self.page.ptype == "topic"
            || self.page.ptype == "synthesis"
            || self.pack_type.is_some()
        {
            self.check_citations(ty.cited);
        }
        self.check_log();
        self.check_pack_rules();
        self.findings
    }

    /// FM-001/FM-002: a page without parseable frontmatter and a type gets no further checks.
    fn check_parseable(&mut self) -> bool {
        if let Some(e) = &self.page.parse_error {
            self.push(
                Severity::Error,
                "FM-001",
                Some(1),
                format!("cannot parse frontmatter: {e}"),
            );
            return false;
        }
        if !self.page.doc.has_frontmatter {
            self.push(
                Severity::Error,
                "FM-001",
                Some(1),
                "no frontmatter (every page starts with --- type: ... ---; use `commonplace new`)"
                    .into(),
            );
            return false;
        }
        if self.page.ptype.is_empty() {
            self.push(Severity::Error, "FM-002", Some(1), "missing `type`".into());
            return false;
        }
        true
    }

    /// FM-004: required properties are present and non-empty.
    fn check_required(&mut self, required: &[String]) {
        const MAY_BE_EMPTY: [&str; 5] = ["text", "origin", "author", "publisher", "description"];
        for r in required {
            match self.page.fm().get(r.as_str()) {
                None => self.push(
                    Severity::Error,
                    "FM-004",
                    Some(1),
                    format!(
                        "missing required property '{r}' for type '{}'",
                        self.page.ptype
                    ),
                ),
                Some(serde_yaml::Value::Null) if !MAY_BE_EMPTY.contains(&r.as_str()) => {
                    self.push(
                        Severity::Error,
                        "FM-004",
                        Some(1),
                        format!("property '{r}' is empty"),
                    );
                }
                _ => {}
            }
        }
    }

    /// FM-005: status, the core enums and pack enums hold allowed values.
    fn check_enums(&mut self, status_vals: &[String]) {
        let p = self.page;
        if let Some(s) = p.get("status")
            && !status_vals.is_empty()
            && !status_vals.iter().any(|v| v == &s)
        {
            self.push(
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
        let core_enums = [
            ("confidence", CONFIDENCE),
            ("confidentiality", CONFIDENTIALITY),
            ("authority", AUTHORITY),
            ("fidelity", FIDELITY),
        ];
        for (field, vals) in core_enums {
            if let Some(v) = p.get(field)
                && !v.is_empty()
                && !vals.contains(&v.as_str())
            {
                self.push(
                    Severity::Error,
                    "FM-005",
                    Some(1),
                    format!("{field} '{v}' is not one of {}", vals.join("|")),
                );
            }
        }
        let Some(t) = self.pack_type else { return };
        for (field, vals) in &t.enums {
            if let Some(v) = p.get(field)
                && !v.is_empty()
                && !vals.iter().any(|x| x == &v)
            {
                self.push(
                    Severity::Error,
                    "FM-005",
                    Some(1),
                    format!("{field} '{v}' is not one of {}", vals.join("|")),
                );
            }
        }
    }

    /// <PACK>-LINK and LINK-004: typed link fields declared by the page's pack type.
    fn check_pack_links(&mut self) {
        let Some(t) = self.pack_type else { return };
        let p = self.page;
        for (field, spec) in &t.links {
            let targets = p.list(field);
            let sev = spec
                .severity
                .as_deref()
                .map_or(Severity::Error, Severity::parse);
            if targets.len() < spec.min {
                let pack_name = self
                    .cx
                    .packs
                    .iter()
                    .find(|pk| pk.types.contains_key(&p.ptype))
                    .map(|pk| pk.name.to_uppercase())
                    .unwrap_or_default();
                self.push(
                    sev,
                    &format!("{pack_name}-LINK"),
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
                for target in markdown::wikilink_targets_in(&tgt) {
                    if let Some(tp) = self.cx.vault.resolve_page(&target)
                        && !spec.to.is_empty()
                        && !spec.to.iter().any(|x| x == &tp.ptype)
                    {
                        self.push(
                            Severity::Warning,
                            "LINK-004",
                            Some(1),
                            format!(
                                "'{field}' links to [[{target}]] of type '{}', expected {}",
                                tp.ptype,
                                spec.to.join("|")
                            ),
                        );
                    }
                }
            }
        }
    }

    /// DATE-001/DATE-002: ISO dates, and `updated` not before `created`.
    fn check_dates(&mut self) {
        let p = self.page;
        for d in ["created", "updated", "date", "published", "retrieved"] {
            if let Some(v) = p.get(d)
                && !v.is_empty()
                && !DATE_RE.is_match(&v)
            {
                self.push(
                    Severity::Warning,
                    "DATE-001",
                    Some(1),
                    format!("'{d}: {v}' is not an ISO date (YYYY-MM-DD)"),
                );
            }
        }
        if let (Some(created), Some(updated)) = (p.get("created"), p.get("updated"))
            && updated < created
        {
            self.push(
                Severity::Warning,
                "DATE-002",
                Some(1),
                "updated is earlier than created".into(),
            );
        }
    }

    /// TAG-001..003: tag format, registration in TAGS.md, and topic-tag count.
    fn check_tags(&mut self) {
        let mut topic_count = 0;
        for t in &self.page.tags {
            if !TAG_RE.is_match(t) {
                self.push(
                    Severity::Error,
                    "TAG-001",
                    Some(1),
                    format!(
                        "tag '{t}' has the wrong format (lowercase kebab, optional one namespace: topic/name)"
                    ),
                );
            } else if let Some(reg) = &self.cx.tags
                && !reg.contains(t)
            {
                self.push(
                    Severity::Error,
                    "TAG-002",
                    Some(1),
                    format!("tag '{t}' is not registered in TAGS.md (propose it there first)"),
                );
            }
            if t.starts_with("topic/") {
                topic_count += 1;
            }
        }
        if topic_count > 5 {
            self.push(
                Severity::Warning,
                "TAG-003",
                Some(1),
                format!("{topic_count} topic tags; keep to the 5 most specific"),
            );
        }
    }

    /// LINK-001/LINK-002: every body and frontmatter link resolves to exactly one page.
    fn check_links(&mut self) {
        let vault = self.cx.vault;
        let p = self.page;
        let all_targets = p
            .body_links
            .iter()
            .map(|l| (&l.target, Some(self.body_line(l.line))))
            .chain(p.fm_links.iter().map(|(_, t)| (t, Some(1))));
        let mut found = vec![];
        for (t, line) in all_targets {
            match vault.resolve(t) {
                Resolution::Found(_) => {}
                Resolution::Ambiguous(ix) => found.push((
                    Severity::Warning,
                    "LINK-002",
                    line,
                    format!(
                        "[[{t}]] is ambiguous: {}; link with the folder path",
                        ix.iter()
                            .map(|&j| vault.pages[j].rel.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )),
                Resolution::Missing => {
                    if self.cx.known_sources.contains(t) {
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
                    found.push((
                        Severity::Error,
                        "LINK-001",
                        line,
                        format!("broken link [[{t}]]{hint}"),
                    ));
                }
            }
        }
        for (sev, rule, line, msg) in found {
            self.push(sev, rule, line, msg);
        }
    }

    /// LINK-003: knowledge pages need an inbound link from a content page.
    fn check_orphan(&mut self) {
        let p = self.page;
        let wants_inbound = SYNTHESIS_TYPES.contains(&p.ptype.as_str())
            || p.ptype == "topic"
            || p.ptype == "person"
            || self.pack_type.is_some_and(|t| t.cited);
        if !wants_inbound {
            return;
        }
        let inbound = self.cx.backlinks.get(&self.index).map_or(0, |v| {
            v.iter().filter(|j| !self.cx.hubs.contains(j)).count()
        });
        if inbound == 0 && !p.is_archived() {
            self.push(Severity::Warning, "LINK-003", None, "orphan: no inbound links from content pages (link it from a topic, project or related page)".into());
        }
    }

    /// SUP-001, and CON-001 for open contradictions recorded in frontmatter.
    fn check_contradictions(&mut self) {
        let p = self.page;
        if p.get("status").as_deref() == Some("superseded")
            && p.get("superseded_by").is_none_or(|s| s.is_empty())
        {
            self.push(
                Severity::Warning,
                "SUP-001",
                Some(1),
                "status is superseded but `superseded_by` is empty".into(),
            );
        }
        let Some(serde_yaml::Value::Sequence(cs)) = p.fm().get("contradictions") else {
            return;
        };
        for m in cs.iter().filter_map(serde_yaml::Value::as_mapping) {
            let sev = frontmatter::get_str(m, "severity").unwrap_or_else(|| "soft".into());
            let status = frontmatter::get_str(m, "status").unwrap_or_else(|| "open".into());
            let with = frontmatter::get_str(m, "with").unwrap_or_default();
            if !["soft", "scope", "hard"].contains(&sev.as_str()) {
                self.push(
                    Severity::Error,
                    "FM-005",
                    Some(1),
                    format!("contradiction severity '{sev}' is not soft|scope|hard"),
                );
            }
            if status == "open" {
                let level = if sev == "hard" {
                    Severity::Error
                } else {
                    Severity::Warning
                };
                self.push(
                    level,
                    "CON-001",
                    Some(1),
                    format!(
                        "open {sev} contradiction with {with}; resolve it or record why it stays open"
                    ),
                );
            }
        }
    }

    /// SEC-001, TERM-001, SIZE-001, EMPTY-001, STALE-001/002 and PROJ-001.
    fn check_body(&mut self, cited: bool) {
        const NO_SIZE_LIMIT: [&str; 8] = [
            "source",
            "journal",
            "log",
            "index",
            "review",
            "deliverable",
            "draft",
            "comparison",
        ];
        let p = self.page;
        let cfg = &self.cx.vault.config.lint;
        let body = &p.doc.body;
        if let Some(m) = SECRET_RE.find(&self.masked) {
            self.push(
                Severity::Error,
                "SEC-001",
                Some(self.masked_line(m.start())),
                "looks like a credential or private key; remove it".into(),
            );
        }
        for term in &cfg.retired_terms {
            if term.is_empty() || p.get("status").as_deref() == Some("superseded") {
                continue;
            }
            if let Some(pos) = self.masked.to_lowercase().find(&term.to_lowercase()) {
                self.push(
                    Severity::Warning,
                    "TERM-001",
                    Some(self.masked_line(pos)),
                    format!(
                        "retired term '{term}' still appears; update to the current term or mark the page superseded"
                    ),
                );
            }
        }
        let words = markdown::word_count(body);
        if words > cfg.max_words && !NO_SIZE_LIMIT.contains(&p.ptype.as_str()) {
            self.push(
                Severity::Warning,
                "SIZE-001",
                None,
                format!(
                    "{words} words; split into atomic pages (limit {})",
                    cfg.max_words
                ),
            );
        }
        if cited && !markdown::has_prose(body) {
            self.push(
                Severity::Info,
                "EMPTY-001",
                None,
                "stub: headings only, no content".into(),
            );
        }
        if cited {
            self.check_staleness();
        }
        if p.ptype == "project"
            && p.get("status").as_deref() == Some("active")
            && p.list("next_actions").is_empty()
            && markdown::section(body, "Next actions").is_none_or(|s| !markdown::has_prose(s))
        {
            self.push(Severity::Info, "PROJ-001", Some(1), "active project without next actions (GTD: every live project has a next action, or park it)".into());
        }
    }

    /// STALE-001/002: mature cited pages that have not been updated for a while.
    fn check_staleness(&mut self) {
        let p = self.page;
        let stale_days = self.cx.vault.config.lint.stale_days;
        if let Some(u) = p.get("updated")
            && stale_days > 0
            && p.get("status").as_deref() == Some("mature")
            && let (Ok(updated), Ok(today)) = (
                chrono::NaiveDate::parse_from_str(&u[..u.len().min(10)], "%Y-%m-%d"),
                chrono::NaiveDate::parse_from_str(&self.cx.today, "%Y-%m-%d"),
            )
        {
            let age = (today - updated).num_days();
            if age > i64::from(stale_days) {
                self.push(
                    Severity::Info,
                    "STALE-001",
                    Some(1),
                    format!(
                        "not updated for {age} days; re-check against newer sources or confirm `updated`"
                    ),
                );
            }
            if age > 90 && CURRENT_RE.is_match(&self.masked) {
                self.push(
                    Severity::Warning,
                    "STALE-002",
                    Some(1),
                    format!("says 'currently/latest' but was last updated {age} days ago"),
                );
            }
        }
    }

    /// SRC-001/002/004: a source note has its raw original and text layer.
    fn check_source_note(&mut self) {
        let p = self.page;
        if p.ptype != "source" {
            return;
        }
        let root = &self.cx.vault.root;
        if let Some(raw) = p.get("raw")
            && !raw.is_empty()
            && !root.join(&raw).exists()
        {
            self.push(
                Severity::Error,
                "SRC-001",
                Some(1),
                format!("raw original '{raw}' is missing"),
            );
        }
        let st = p.get("status").unwrap_or_default();
        if st != "registered" && sources::text_layer_path(root, &p.stem).is_none() {
            self.push(
                Severity::Error,
                "SRC-002",
                Some(1),
                format!(
                    "no text layer for this source (expected sources/text/{}.text.md or a folder); status '{st}' claims one",
                    p.stem
                ),
            );
        }
        if st == "registered" || st == "converted" {
            self.push(Severity::Info, "SRC-004", Some(1), "source not yet summarised (write TL;DR, summary and key quotes, then set status: summarised)".into());
        }
    }

    /// CITE-001..013 and PRIV-001.
    fn check_citations(&mut self, cited: bool) {
        let p = self.page;
        let body = &p.doc.body;
        let (citations, _plain) = cite::parse_citations(body);
        self.check_footnote_pairs();
        if cited
            && markdown::has_prose(body)
            && citations.is_empty()
            && p.list("sources").is_empty()
            && p.get("status").as_deref() != Some("superseded")
        {
            let honest = self.masked.contains("unsourced:")
                && matches!(p.get("confidence").as_deref(), Some("low" | "contested"));
            if honest {
                self.push(
                    Severity::Info,
                    "CITE-012",
                    None,
                    "unsourced page (marked honestly); ingest a source and cite when one exists"
                        .into(),
                );
            } else {
                self.push(Severity::Warning, "CITE-012", None, "no sources and no citations: either cite (footnote with [[source]] locator \"quote\") or mark claims `unsourced:` with confidence: low".into());
            }
        }
        // declared sources must exist
        for s in p.list("sources") {
            for t in markdown::wikilink_targets_in(&s) {
                if !self.cx.known_sources.contains(&t) && self.cx.vault.resolve_page(&t).is_none() {
                    self.push(
                        Severity::Error,
                        "CITE-013",
                        Some(1),
                        format!("sources lists [[{t}]] but no such source note exists"),
                    );
                }
            }
        }
        if self.cx.verify_citations {
            let my_conf = p
                .get("confidentiality")
                .unwrap_or_else(|| "personal".into());
            for c in &citations {
                self.verify_citation(c, &my_conf);
            }
        }
    }

    /// CITE-010/011: every footnote reference has a definition and vice versa.
    fn check_footnote_pairs(&mut self) {
        let body = &self.page.doc.body;
        let refs = markdown::footnote_refs(body);
        let defs = markdown::footnote_defs(body);
        let def_labels: HashSet<&str> = defs.iter().map(|d| d.label.as_str()).collect();
        let ref_labels: HashSet<&str> = refs.iter().map(|(l, _)| l.as_str()).collect();
        for (l, line) in &refs {
            if !def_labels.contains(l.as_str()) {
                self.push(
                    Severity::Error,
                    "CITE-010",
                    Some(self.body_line(*line)),
                    format!("footnote [^{l}] has no definition"),
                );
            }
        }
        for d in &defs {
            if !ref_labels.contains(d.label.as_str()) {
                self.push(
                    Severity::Warning,
                    "CITE-011",
                    Some(self.body_line(d.line)),
                    format!("footnote definition [^{}] is never referenced", d.label),
                );
            }
        }
    }

    /// CITE-001..009 and PRIV-001 for one citation.
    fn verify_citation(&mut self, c: &cite::Citation, my_conf: &str) {
        let vault = self.cx.vault;
        let line = Some(self.body_line(c.line));
        if !self.cx.known_sources.contains(&c.source) {
            match vault.resolve_page(&c.source) {
                Some(tp) if tp.ptype != "source" => self.push(
                    Severity::Warning,
                    "CITE-002",
                    line,
                    format!(
                        "[^{}] cites [[{}]], which is a {} page, not a source; cite the underlying source",
                        c.label, c.source, tp.ptype
                    ),
                ),
                _ => self.push(
                    Severity::Error,
                    "CITE-001",
                    line,
                    format!("[^{}] cites unknown source [[{}]]", c.label, c.source),
                ),
            }
            return;
        }
        if let Some(sp) = vault.resolve_page(&c.source) {
            let sc = sp
                .get("confidentiality")
                .unwrap_or_else(|| "personal".into());
            if confidentiality_rank(&sc) > confidentiality_rank(my_conf) {
                self.push(
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
        let Ok(Some(tl)) = sources::load_text_layer(&vault.root, &c.source) else {
            self.push(
                Severity::Warning,
                "CITE-009",
                line,
                format!(
                    "[^{}] source [[{}]] has no text layer; cannot verify the quote",
                    c.label, c.source
                ),
            );
            return;
        };
        let (verdict, found, _near, msg) = cite::verify_quote(
            &tl.text,
            c,
            vault.config.lint.quote_min_words,
            vault.config.lint.quote_max_words,
        );
        let location = found.map(|l| tl.locate(l)).unwrap_or_default();
        if let Some((sev, rule, message)) = verdict_finding(&verdict, c, &msg, &location) {
            self.push(sev, rule, line, message);
        }
    }

    /// LOG-001: log.md entry headings follow `## [YYYY-MM-DD] op | Title`.
    fn check_log(&mut self) {
        if self.page.rel != "log.md" {
            return;
        }
        let bad: Vec<usize> = self
            .page
            .doc
            .body
            .lines()
            .enumerate()
            .filter(|(_, l)| l.starts_with("## ") && !LOG_RE.is_match(l))
            .map(|(n, _)| n + 1)
            .collect();
        for n in bad {
            self.push(
                Severity::Warning,
                "LOG-001",
                Some(self.body_line(n)),
                "log entry heading should be '## [YYYY-MM-DD] op | Title'".into(),
            );
        }
    }

    /// Rules declared by enabled packs.
    fn check_pack_rules(&mut self) {
        for pk in self.cx.packs {
            for r in &pk.rules {
                if rule_applies(r, self.page) {
                    self.apply_pack_rule(r);
                }
            }
        }
    }

    fn apply_pack_rule(&mut self, r: &pack::Rule) {
        let sev = Severity::parse(&r.severity);
        match r.check.as_str() {
            "required" => self.rule_required(r, &sev),
            "enum" => {
                if let Some(v) = self.page.get(&r.field)
                    && !v.is_empty()
                    && !r.values.iter().any(|x| x == &v)
                {
                    let extra = format!("{} '{v}' not in {}", r.field, r.values.join("|"));
                    self.push(sev, &r.id, Some(1), rule_message(r, &extra));
                }
            }
            "link_field" => self.rule_link_field(r, &sev),
            "field_compare" => self.rule_field_compare(r, sev),
            "forbid_regex" => {
                if let Ok(re) = Regex::new(&r.pattern)
                    && let Some(m) = re.find(&self.masked)
                {
                    let extra = format!("matched '{}'", m.as_str());
                    self.push(
                        sev,
                        &r.id,
                        Some(self.masked_line(m.start())),
                        rule_message(r, &extra),
                    );
                }
            }
            "require_regex" => {
                if let Ok(re) = Regex::new(&r.pattern)
                    && !re.is_match(&self.masked)
                {
                    self.push(sev, &r.id, None, rule_message(r, ""));
                }
            }
            "body_regex_requires_page" => self.rule_body_regex_requires_page(r, &sev),
            "stale_year" => self.rule_stale_year(r, sev),
            other => self.push(
                Severity::Warning,
                &r.id,
                None,
                format!("pack rule uses unknown check '{other}'"),
            ),
        }
    }

    fn rule_required(&mut self, r: &pack::Rule, sev: &Severity) {
        let p = self.page;
        let fields = r
            .fields
            .iter()
            .chain(std::iter::once(&r.field))
            .filter(|s| !s.is_empty());
        for fld in fields {
            if p.get(fld).is_none_or(|v| v.is_empty()) && p.list(fld).is_empty() {
                let extra = format!("missing '{fld}'");
                self.push(sev.clone(), &r.id, Some(1), rule_message(r, &extra));
            }
        }
    }

    fn rule_link_field(&mut self, r: &pack::Rule, sev: &Severity) {
        let targets = self.page.list(&r.field);
        let min = if r.min == 0 { 1 } else { r.min };
        if targets.len() < min {
            let extra = format!("'{}' needs {min}+ link(s) to {}", r.field, r.to.join("|"));
            self.push(sev.clone(), &r.id, Some(1), rule_message(r, &extra));
        }
        for t in targets
            .iter()
            .flat_map(|t| markdown::wikilink_targets_in(t))
        {
            if let Some(tp) = self.cx.vault.resolve_page(&t)
                && !r.to.is_empty()
                && !r.to.iter().any(|x| x == &tp.ptype)
            {
                let extra = format!("'{}' links to [[{t}]] of type '{}'", r.field, tp.ptype);
                self.push(Severity::Warning, &r.id, Some(1), rule_message(r, &extra));
            }
        }
    }

    fn rule_field_compare(&mut self, r: &pack::Rule, sev: Severity) {
        let p = self.page;
        let (Some(a), Some(b)) = (p.get(&r.field), p.get(&r.other)) else {
            return;
        };
        let expr = crate::query::Expr::Cmp {
            field: r.field.clone(),
            op: r.op.clone(),
            rhs: crate::query::Rhs::Field(r.other.clone()),
        };
        if !crate::query::eval(&expr, p) {
            let extra = format!("{}={a} {} {}={b} does not hold", r.field, r.op, r.other);
            self.push(sev, &r.id, Some(1), rule_message(r, &extra));
        }
    }

    fn rule_body_regex_requires_page(&mut self, r: &pack::Rule, sev: &Severity) {
        let Ok(re) = Regex::new(&r.pattern) else {
            return;
        };
        let matches: Vec<(usize, String)> = re
            .find_iter(&self.masked)
            .map(|m| (m.start(), m.as_str().to_string()))
            .collect();
        for (start, text) in matches {
            let line = Some(self.masked_line(start));
            let Some(q) = find_page_by_field(self.cx.vault, &r.page_type, &r.page_field, &text)
            else {
                let extra = format!(
                    "'{text}' has no {} page with {} matching it",
                    r.page_type, r.page_field
                );
                self.push(sev.clone(), &r.id, line, rule_message(r, &extra));
                continue;
            };
            for (k, want) in &r.page_must {
                let have = q.get(k).unwrap_or_default();
                if !have.eq_ignore_ascii_case(want) {
                    let extra = format!("'{text}' -> [[{}]] has {k}={have}, needs {want}", q.stem);
                    self.push(sev.clone(), &r.id, line, rule_message(r, &extra));
                }
            }
        }
    }

    fn rule_stale_year(&mut self, r: &pack::Rule, sev: Severity) {
        let Some(v) = self.page.get(&r.field) else {
            return;
        };
        let year: i32 = self.cx.today[..4].parse().unwrap_or(0);
        let cutoff = year.saturating_sub_unsigned(r.years_back);
        if let Ok(y) = v.trim().parse::<i32>()
            && y < cutoff
        {
            let extra = format!("{}={y} is older than {cutoff}", r.field);
            self.push(sev, &r.id, Some(1), rule_message(r, &extra));
        }
    }
}

/// Whether a pack rule's `when` clause selects this page.
fn rule_applies(r: &pack::Rule, p: &Page) -> bool {
    if let Some(t) = &r.when.ptype
        && t != &p.ptype
    {
        return false;
    }
    if let Some(g) = &r.when.folder
        && !pack::glob_match(g, &p.rel)
    {
        return false;
    }
    !p.get("status")
        .is_some_and(|s| r.when.status_not.contains(&s))
}

/// A pack rule's message, with check-specific detail appended in parentheses.
fn rule_message(r: &pack::Rule, extra: &str) -> String {
    if r.message.is_empty() {
        extra.to_string()
    } else if extra.is_empty() {
        r.message.clone()
    } else {
        format!("{} ({extra})", r.message)
    }
}

/// The first page of `page_type` whose `field` (scalar or list) matches `needle` after normalisation.
fn find_page_by_field<'v>(
    vault: &'v Vault,
    page_type: &str,
    field: &str,
    needle: &str,
) -> Option<&'v Page> {
    let needle = cite::normalise(needle);
    vault.pages.iter().find(|q| {
        q.ptype == page_type
            && (cite::normalise(&q.get(field).unwrap_or_default()) == needle
                || q.list(field).iter().any(|v| cite::normalise(v) == needle))
    })
}

/// The finding (if any) for a quote-verification verdict.
fn verdict_finding(
    verdict: &Verdict,
    c: &cite::Citation,
    msg: &str,
    location: &str,
) -> Option<(Severity, &'static str, String)> {
    let label = &c.label;
    Some(match verdict {
        Verdict::Pass => return None,
        Verdict::QuoteNotFound => (
            Severity::Error,
            "CITE-003",
            format!("[^{label}] quote not found in [[{}]]: {msg}", c.source),
        ),
        Verdict::QuoteNotInSpan => (
            Severity::Error,
            "CITE-004",
            format!("[^{label}] {msg} (found at {location}); fix the locator"),
        ),
        Verdict::SpanNotFound => (Severity::Warning, "CITE-005", format!("[^{label}] {msg}")),
        Verdict::NoQuote => (
            Severity::Warning,
            "CITE-006",
            format!("[^{label}] has no verbatim quote; add one so it can be verified"),
        ),
        Verdict::QuoteTooShort => (Severity::Warning, "CITE-007", format!("[^{label}] {msg}")),
        Verdict::QuoteTooLong => (
            Severity::Warning,
            "CITE-008",
            format!("[^{label}] {msg}; quote less, cite a span"),
        ),
        Verdict::SourceMissing | Verdict::TextLayerMissing => (
            Severity::Warning,
            "CITE-009",
            format!("[^{label}] source has no text layer; cannot verify"),
        ),
    })
}

fn vault_finding(severity: Severity, rule: &str, path: String, message: String) -> Finding {
    Finding {
        severity,
        rule: rule.into(),
        path,
        line: None,
        message,
    }
}

/// DUP-001: a name or alias used by more than one page.
fn check_duplicate_names(vault: &Vault, out: &mut Vec<Finding>) {
    let mut names: HashMap<String, Vec<&str>> = HashMap::new();
    for p in &vault.pages {
        for name in std::iter::once(&p.stem).chain(&p.aliases) {
            names
                .entry(name.to_lowercase())
                .or_default()
                .push(p.rel.as_str());
        }
    }
    for (name, rels) in &names {
        let mut uniq: Vec<&str> = rels.clone();
        uniq.sort_unstable();
        uniq.dedup();
        if uniq.len() > 1 {
            out.push(vault_finding(
                Severity::Warning,
                "DUP-001",
                uniq[0].to_string(),
                format!(
                    "name or alias '{name}' is used by more than one page ({}); merge or disambiguate",
                    uniq.join(", ")
                ),
            ));
        }
    }
}

/// SRC-003: files in sources/raw with no source note.
fn check_unregistered_raw(vault: &Vault, out: &mut Vec<Finding>) {
    let Ok(rd) = std::fs::read_dir(vault.root.join(sources::RAW_DIR)) else {
        return;
    };
    let manifest = crate::manifest::Manifest::load(&vault.root).unwrap_or_default();
    let registered: HashSet<&str> = manifest.sources.values().map(|r| r.raw.as_str()).collect();
    for e in rd.flatten() {
        let path = e.path();
        let hidden = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));
        if path.is_file() && !hidden {
            let rel = crate::fsutil::rel(&vault.root, &path);
            if !registered.contains(rel.as_str()) {
                out.push(vault_finding(
                    Severity::Info,
                    "SRC-003",
                    rel,
                    "file in sources/raw not registered (use `commonplace ingest`; never copy files in by hand)".into(),
                ));
            }
        }
    }
}

/// INBOX-001: unprocessed captures.
fn check_inbox(vault: &Vault, out: &mut Vec<Finding>) {
    let Ok(rd) = std::fs::read_dir(vault.root.join("inbox")) else {
        return;
    };
    let n = rd
        .flatten()
        .filter(|e| e.path().is_file() && !e.file_name().to_string_lossy().starts_with('.'))
        .count();
    if n > 0 {
        out.push(vault_finding(
            Severity::Info,
            "INBOX-001",
            "inbox/".into(),
            format!("{n} unprocessed item(s) in inbox"),
        ));
    }
}

/// IDX-001/002: index.md exists and is newer than every page.
fn check_index_freshness(vault: &Vault, out: &mut Vec<Finding>) {
    let Some(idx) = vault.pages.iter().find(|p| p.rel == "index.md") else {
        out.push(vault_finding(
            Severity::Warning,
            "IDX-002",
            "index.md".into(),
            "no index.md; run `commonplace index`".into(),
        ));
        return;
    };
    let newest = vault
        .pages
        .iter()
        .filter(|p| p.rel != "index.md" && !p.rel.starts_with("indexes/"))
        .filter_map(|p| p.mtime)
        .max();
    if let (Some(n), Some(im)) = (newest, idx.mtime)
        && n > im
    {
        out.push(vault_finding(
            Severity::Info,
            "IDX-001",
            "index.md".into(),
            "index.md is older than the newest page; run `commonplace index`".into(),
        ));
    }
}

/// Drop disabled and out-of-scope findings, sort, and count.
fn finish(vault: &Vault, opts: &LintOptions, all: Vec<Finding>) -> LintReport {
    // rule ids renamed in 0.4.0 (TOG-00n → EA-00n): accept either spelling in disabled_rules
    let disabled: HashSet<String> = vault
        .config
        .lint
        .disabled_rules
        .iter()
        .flat_map(|r| [r.clone(), r.replacen("TOG-", "EA-", 1)])
        .collect();
    let mut findings: Vec<Finding> = all
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
    let count = |s: Severity| findings.iter().filter(|x| x.severity == s).count();
    LintReport {
        errors: count(Severity::Error),
        warnings: count(Severity::Warning),
        infos: count(Severity::Info),
        pages_checked: vault.pages.len(),
        findings,
    }
}

pub fn lint(vault: &Vault, packs: &[Pack], opts: &LintOptions) -> LintReport {
    let cx = Context::new(vault, packs, opts);
    let mut findings = vec![];
    check_duplicate_names(vault, &mut findings);
    for (i, p) in vault.pages.iter().enumerate() {
        findings.extend(PageCheck::new(&cx, i, p).run());
    }
    check_unregistered_raw(vault, &mut findings);
    check_inbox(vault, &mut findings);
    check_index_freshness(vault, &mut findings);
    finish(vault, opts, findings)
}

/// Run only citation verification, returning per-citation results (used by `commonplace cite verify`).
pub fn verify_all_citations<S: std::hash::BuildHasher>(
    vault: &Vault,
    only: Option<&HashSet<String, S>>,
) -> Vec<cite::CiteResult> {
    let mut out = vec![];
    let known: HashSet<String> = sources::known_source_ids(&vault.root).into_iter().collect();
    for p in &vault.pages {
        if let Some(o) = only
            && !o.contains(&p.rel)
            && !o.iter().any(|x| p.rel.starts_with(x.trim_end_matches('/')))
        {
            continue;
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
            if let Ok(Some(tl)) = sources::load_text_layer(&vault.root, &c.source) {
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
            } else {
                res.verdict = Verdict::TextLayerMissing;
                res.message = "source has no text layer".into();
            }
            out.push(res);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{EffectiveType, confidentiality_rank, core_type, rule_message};
    use crate::pack::{Rule, TypeSpec};

    #[test]
    fn rule_message_joins_rule_text_and_detail() {
        let mut r = Rule::default();
        assert_eq!(rule_message(&r, "detail"), "detail");
        r.message = "needs X".into();
        assert_eq!(rule_message(&r, ""), "needs X");
        assert_eq!(rule_message(&r, "detail"), "needs X (detail)");
    }

    #[test]
    fn cited_pack_type_adds_knowledge_fields_once() {
        let core = core_type("topic");
        let pack = TypeSpec {
            required: vec!["owner".into(), "status".into()],
            cited: true,
            ..TypeSpec::default()
        };
        let ty = EffectiveType::merge(core.as_ref(), Some(&pack));
        assert!(ty.cited);
        for f in ["owner", "tags", "confidence", "description"] {
            assert!(
                ty.required.iter().any(|r| r == f),
                "{f} missing: {:?}",
                ty.required
            );
        }
        // "status" comes from the pack and is not added a second time
        assert_eq!(ty.required.iter().filter(|r| *r == "status").count(), 1);
    }

    #[test]
    fn pack_status_list_replaces_core_list() {
        let core = core_type("concept");
        let pack = TypeSpec {
            status: vec!["draft".into()],
            ..TypeSpec::default()
        };
        let ty = EffectiveType::merge(core.as_ref(), Some(&pack));
        assert_eq!(ty.status, ["draft"]);
        assert!(ty.cited, "core concept pages stay cited");
    }

    #[test]
    fn unknown_confidentiality_ranks_as_personal() {
        assert_eq!(
            confidentiality_rank("bogus"),
            confidentiality_rank("personal")
        );
        assert!(confidentiality_rank("privileged") > confidentiality_rank("public"));
    }
}
