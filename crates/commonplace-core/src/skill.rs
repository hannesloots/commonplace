//! The skills layer: agent skills compiled from the wiki.
//!
//! WikiSkill (Tang et al., 2026) keeps three layers: immutable raw
//! experience, a wiki that distils it, and executable skills built from the
//! wiki. Here the raw layer is `sources/` plus the use records below, the
//! wiki is the commonplace (playbooks hold what worked, the error book what
//! failed), and each skill is a folder `skills/<name>/` with
//!
//! - `SKILL.md`: the procedure an agent loads (Agent Skills format). It must
//!   stand alone: in the paper, agents that read the wiki mid-task produced
//!   worse skills.
//! - `PURPOSE.md`: `[[links]]` to the wiki pages that motivated it.
//!
//! An edit under `skills/<name>/` is a proposal. `accept` copies it to the
//! accepted store (`.commonplace/skills/<name>/`) and installs it where agents
//! look; `reject` restores the accepted version. Both append the diff to
//! `skill-impact.md`, which is never rolled back, so a rejected idea is not
//! proposed again. Agents only ever see accepted versions.

use crate::config::Config;
use crate::lint::{Finding, Severity};
use crate::vault::Vault;
use crate::{Error, Result, frontmatter, fsutil, markdown};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Folder (relative to the root) holding the skill sources.
pub const SKILLS_DIR: &str = "skills";
/// The append-only record of skill decisions and uses.
pub const IMPACT_FILE: &str = "skill-impact.md";
const SKILL_FILE: &str = "SKILL.md";
const PURPOSE_FILE: &str = "PURPOSE.md";
/// Marks an installed copy as written by commonplace and names the root that
/// wrote it, so an install never overwrites a skill someone else put there.
const MARKER_FILE: &str = ".commonplace-skill";
/// Agent Skills limits on `name` and `description` (agentskills.io specification).
const MAX_NAME_CHARS: usize = 64;
const MAX_DESCRIPTION_CHARS: usize = 1024;

const SKILL_TEMPLATE: &str = include_str!("../../../skills/commonplace/templates/skill.md");
const PURPOSE_TEMPLATE: &str = include_str!("../../../skills/commonplace/templates/purpose.md");
pub(crate) const IMPACT_TEMPLATE: &str =
    include_str!("../../../skills/commonplace/templates/skill-impact.md");

/// A skill folder's files by relative path (forward slashes), sorted.
type Files = BTreeMap<String, Vec<u8>>;

/// Where a skill's source stands against its accepted version.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillState {
    /// Never accepted: a proposal for a new skill.
    New,
    /// The source is the accepted version.
    Accepted,
    /// The source differs from the accepted version: a pending proposal.
    Changed,
    /// Accepted, but the source folder was deleted: accepting retires it.
    Removed,
}

/// One skill, as `skill list` reports it.
#[derive(Debug, Serialize)]
pub struct Skill {
    /// The folder name, which is also the skill's `name`.
    pub name: String,
    /// `description` from `SKILL.md` (the source, or the accepted copy when the source is gone).
    pub description: String,
    /// Source against accepted version.
    pub state: SkillState,
    /// Wiki pages `PURPOSE.md` links to.
    pub purpose: Vec<String>,
    /// Uses recorded with `skill used`.
    pub uses: usize,
    /// Of those, uses recorded as failed.
    pub failures: usize,
    /// Date of the last recorded use.
    pub last_used: Option<String>,
    /// Date of the last accept or reject.
    pub last_decision: Option<String>,
}

/// The outcome of one use of a skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The skill did its job.
    Ok,
    /// The skill was followed and the task still went wrong.
    Failed,
}

/// The gate's verdict on a pending proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Keep the proposal: it becomes the accepted version and is installed.
    Accept,
    /// Drop the proposal: the source returns to the accepted version.
    Reject,
}

impl Decision {
    fn label(self) -> &'static str {
        match self {
            Decision::Accept => "accepted",
            Decision::Reject => "rejected",
        }
    }
}

/// What `skill accept` or `skill reject` did.
#[derive(Debug, Serialize)]
pub struct DecisionReport {
    /// The skill.
    pub name: String,
    /// `accepted` or `rejected`.
    pub decision: &'static str,
    /// Files the proposal changed.
    pub files: Vec<String>,
    /// Folders the accepted version was installed to (or removed from, for a retired skill).
    pub installed: Vec<String>,
}

/// What `skill sync` did.
#[derive(Debug, Serialize, Default)]
pub struct SyncReport {
    /// Folders an accepted skill was written to.
    pub installed: Vec<String>,
    /// Installed copies removed because their skill was retired.
    pub removed: Vec<String>,
    /// Skills with a proposal waiting for `accept` or `reject` (installed as last accepted, if ever).
    pub pending: Vec<String>,
}

/// Why `name` breaks the Agent Skills naming rules, if it does.
fn name_problem(name: &str) -> Option<String> {
    let ok = !name.is_empty()
        && name.len() <= MAX_NAME_CHARS
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    (!ok).then(|| {
        format!(
            "'{name}' is not a valid skill name: use 1-{MAX_NAME_CHARS} lowercase letters, digits and single hyphens, e.g. `weekly-review`"
        )
    })
}

/// `description` from the `SKILL.md` next to the `PURPOSE.md` at `purpose`.
pub(crate) fn description_beside(purpose: &Path) -> Option<String> {
    let text = read_text(&purpose.with_file_name(SKILL_FILE))?;
    frontmatter::get_str(&frontmatter::parse(&text).ok()?.fm, "description")
}

/// The accepted copy of `name`'s `PURPOSE.md`.
pub(crate) fn accepted_purpose(root: &Path, name: &str) -> PathBuf {
    accepted_dir(root, name).join(PURPOSE_FILE)
}

fn source_dir(root: &Path, name: &str) -> PathBuf {
    root.join(SKILLS_DIR).join(name)
}

fn accepted_dir(root: &Path, name: &str) -> PathBuf {
    crate::vault::tool_dir(root).join("skills").join(name)
}

/// "weekly-review" -> "Weekly review".
fn title_of(name: &str) -> String {
    let words = name.replace('-', " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().chain(chars).collect::<String>()
    })
}

/// Names of the sub-folders of `dir`, sorted; empty when `dir` does not exist.
fn subdirs(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut v: Vec<String> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| !n.starts_with('.'))
        .collect();
    v.sort();
    v
}

/// Whether `skills/<name>/` is a skill: it has a `SKILL.md` or an accepted
/// copy. Any other folder under `skills/` (notes on skills one is learning,
/// say) holds ordinary wiki pages.
fn is_skill_folder(root: &Path, name: &str) -> bool {
    source_dir(root, name).join(SKILL_FILE).is_file() || accepted_dir(root, name).is_dir()
}

/// For a vault-relative path inside a skill folder: the skill's name, and
/// whether the file is its `PURPOSE.md` (the skill's wiki page). `None` for
/// every other path, including notes in folders under `skills/` that are not skills.
pub(crate) fn skill_file<'a>(root: &Path, rel: &'a str) -> Option<(&'a str, bool)> {
    let rest = rel.strip_prefix(SKILLS_DIR)?.strip_prefix('/')?;
    let (name, file) = rest.split_once('/')?;
    is_skill_folder(root, name).then_some((name, file == PURPOSE_FILE))
}

/// Every skill name: skill folders under `skills/` and accepted copies.
fn names(root: &Path) -> Vec<String> {
    let mut all: BTreeSet<String> = subdirs(&root.join(SKILLS_DIR))
        .into_iter()
        .filter(|n| is_skill_folder(root, n))
        .collect();
    all.extend(subdirs(&crate::vault::tool_dir(root).join("skills")));
    all.into_iter().collect()
}

/// A skill's source and accepted files, read once (`None`: no such folder).
struct Snapshot {
    source: Option<Files>,
    accepted: Option<Files>,
}

impl Snapshot {
    fn read(root: &Path, name: &str) -> Result<Self> {
        let read = |dir: PathBuf| -> Result<Option<Files>> {
            if dir.is_dir() {
                read_files(&dir).map(Some)
            } else {
                Ok(None)
            }
        };
        Ok(Self {
            source: read(source_dir(root, name))?,
            accepted: read(accepted_dir(root, name))?,
        })
    }

    fn state(&self) -> SkillState {
        match (&self.source, &self.accepted) {
            (Some(_), None) => SkillState::New,
            (None, Some(_)) => SkillState::Removed,
            (s, a) if s == a => SkillState::Accepted,
            _ => SkillState::Changed,
        }
    }
}

/// Every file under `dir` except the install marker; empty when `dir` does not exist.
fn read_files(dir: &Path) -> Result<Files> {
    let mut out = Files::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for e in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let e = e.map_err(|e| Error::io("reading", dir, e.into()))?;
        if !e.file_type().is_file() {
            continue;
        }
        let rel = fsutil::rel(dir, e.path());
        if rel == MARKER_FILE {
            continue;
        }
        let data = std::fs::read(e.path()).map_err(|err| Error::io("reading", e.path(), err))?;
        out.insert(rel, data);
    }
    Ok(out)
}

fn remove_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| Error::io("removing", dir, e))?;
    }
    Ok(())
}

/// Replace `dir` with `files` (none: remove it). `PURPOSE.md` is left out of installed copies.
fn write_files(dir: &Path, files: &Files, for_agents: bool) -> Result<()> {
    remove_dir(dir)?;
    for (rel, data) in files {
        if for_agents && rel == PURPOSE_FILE {
            continue;
        }
        fsutil::write_atomic(&dir.join(rel), data)?;
    }
    Ok(())
}

/// Fail with `invalid_input` for a name that is not a valid skill name (so
/// `..` or `a/b` never reach a path), else with `skill_not_found` unless
/// `name` has a source or an accepted copy.
fn require_known(root: &Path, name: &str) -> Result<()> {
    if let Some(problem) = name_problem(name) {
        return Err(Error::invalid_input(problem));
    }
    if source_dir(root, name).is_dir() || accepted_dir(root, name).is_dir() {
        Ok(())
    } else {
        Err(Error::skill_not_found(name))
    }
}

/// `s` on one line: a newline in a note or reason would otherwise start a
/// forged `## [date] used | name` entry in the impact record.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn read_text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// `PURPOSE.md` link targets with their 1-based lines.
fn purpose_links(dir: &Path) -> Vec<(String, usize)> {
    read_text(&dir.join(PURPOSE_FILE))
        .map(|t| {
            markdown::wikilinks(&t)
                .into_iter()
                .map(|l| (l.target, l.line))
                .collect()
        })
        .unwrap_or_default()
}

/// Create `skills/<name>/` with `SKILL.md` and `PURPOSE.md` from the templates.
/// The new skill is a proposal until it is accepted.
///
/// # Errors
///
/// `invalid_input` for a bad name or an empty or overlong description;
/// `already_exists` when the folder exists or a page already has the name
/// (the skill's page is named after it); `io` on write failure.
pub fn new_skill(vault: &Vault, name: &str, description: &str) -> Result<String> {
    let root = &vault.root;
    if let Some(problem) = name_problem(name) {
        return Err(Error::invalid_input(problem));
    }
    if let Some(page) = vault.resolve_page(name) {
        return Err(Error::already_exists_with(
            format!("a page named '{name}' ({})", page.rel),
            "the skill's page would share its name and make [[links]] ambiguous; choose another skill name",
        ));
    }
    let description = one_line(description);
    if description.is_empty() || description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(Error::invalid_input(format!(
            "give a --description of 1-{MAX_DESCRIPTION_CHARS} characters saying what the skill does and when to use it"
        )));
    }
    let dir = source_dir(root, name);
    if dir.exists() || accepted_dir(root, name).exists() {
        return Err(Error::already_exists_with(
            format!("{SKILLS_DIR}/{name}"),
            "edit the existing skill, or choose another name",
        ));
    }
    let title = title_of(name);
    let mut doc = frontmatter::parse(SKILL_TEMPLATE)?;
    frontmatter::set_str(&mut doc.fm, "name", name);
    frontmatter::set_str(&mut doc.fm, "description", &description);
    let skill = frontmatter::render(&doc.fm, &doc.body.replace("{{title}}", &title))?;
    fsutil::write_atomic(&dir.join(SKILL_FILE), skill.as_bytes())?;
    let purpose = PURPOSE_TEMPLATE
        .replace("{{name}}", name)
        .replace("{{title}}", &title);
    fsutil::write_atomic(&dir.join(PURPOSE_FILE), purpose.as_bytes())?;
    Ok(fsutil::rel(root, &dir))
}

/// Per-skill counts read back from `skill-impact.md`.
#[derive(Default)]
struct Usage {
    uses: usize,
    failures: usize,
    last_used: Option<String>,
    last_decision: Option<String>,
}

fn usage(root: &Path) -> HashMap<String, Usage> {
    let mut map: HashMap<String, Usage> = HashMap::new();
    let Some(text) = read_text(&root.join(IMPACT_FILE)) else {
        return map;
    };
    let re = crate::re(r"(?m)^## \[(\d{4}-\d{2}-\d{2})\] (used|accepted|rejected) \| (\S+)[ \t]*$");
    let caps: Vec<_> = re.captures_iter(&text).collect();
    for (i, c) in caps.iter().enumerate() {
        let end = caps
            .get(i + 1)
            .map_or(text.len(), |n| n.get_match().start());
        let body = &text[c.get_match().end()..end];
        let u = map.entry(c[3].to_string()).or_default();
        let date = Some(c[1].to_string());
        if &c[2] == "used" {
            u.uses += 1;
            if body.lines().any(|l| l.trim() == "- outcome: failed") {
                u.failures += 1;
            }
            u.last_used = date;
        } else {
            u.last_decision = date;
        }
    }
    map
}

/// Every skill: its state, purpose links and recorded uses.
///
/// # Errors
///
/// `io` when a skill folder cannot be read.
pub fn list(root: &Path) -> Result<Vec<Skill>> {
    let mut used = usage(root);
    let mut out = vec![];
    for name in names(root) {
        let snap = Snapshot::read(root, &name)?;
        let state = snap.state();
        // the source, or the accepted copy of a skill whose folder was deleted
        let files = snap.source.or(snap.accepted).unwrap_or_default();
        let text = |f: &str| files.get(f).and_then(|b| std::str::from_utf8(b).ok());
        let description = text(SKILL_FILE)
            .and_then(|t| frontmatter::parse(t).ok())
            .and_then(|d| frontmatter::get_str(&d.fm, "description"))
            .unwrap_or_default();
        let purpose = text(PURPOSE_FILE)
            .map(|t| {
                markdown::wikilinks(t)
                    .into_iter()
                    .map(|l| l.target)
                    .collect()
            })
            .unwrap_or_default();
        let u = used.remove(&name).unwrap_or_default();
        out.push(Skill {
            purpose,
            name,
            description,
            state,
            uses: u.uses,
            failures: u.failures,
            last_used: u.last_used,
            last_decision: u.last_decision,
        });
    }
    Ok(out)
}

fn as_text(f: Option<&Vec<u8>>) -> Option<&str> {
    f.map_or(Some(""), |b| std::str::from_utf8(b).ok())
}

/// A unified diff from `old` to `new`, file by file.
fn diff_files(old: &Files, new: &Files) -> String {
    let mut out = String::new();
    let keys: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    for k in keys {
        let (a, b) = (old.get(k), new.get(k));
        if a == b {
            continue;
        }
        let (Some(a_txt), Some(b_txt)) = (as_text(a), as_text(b)) else {
            let _ = writeln!(out, "Binary file {k} changed");
            continue;
        };
        let from = a.map_or_else(|| "/dev/null".to_string(), |_| format!("a/{k}"));
        let to = b.map_or_else(|| "/dev/null".to_string(), |_| format!("b/{k}"));
        let diff = similar::TextDiff::from_lines(a_txt, b_txt);
        let _ = write!(out, "{}", diff.unified_diff().header(&from, &to));
    }
    out
}

/// The pending proposal for `name` as a unified diff against the accepted
/// version (empty when there is none).
///
/// # Errors
///
/// `skill_not_found` for an unknown skill; `io` when a folder cannot be read.
pub fn diff(root: &Path, name: &str) -> Result<String> {
    require_known(root, name)?;
    let snap = Snapshot::read(root, name)?;
    Ok(diff_files(
        &snap.accepted.unwrap_or_default(),
        &snap.source.unwrap_or_default(),
    ))
}

/// A code fence longer than any run of backticks in `text`.
fn fence_for(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

/// Append `## [date] op | name` with `bullets` and an optional diff to `skill-impact.md`.
fn append_impact(
    root: &Path,
    op: &str,
    name: &str,
    bullets: &[String],
    diff: Option<&str>,
) -> Result<()> {
    let block = diff.map(|d| {
        let fence = fence_for(d);
        let nl = if d.ends_with('\n') { "" } else { "\n" };
        format!("{fence}diff\n{d}{nl}{fence}\n")
    });
    let entry = crate::ops::Entry {
        op,
        title: name,
        bullets,
        trailer: block.as_deref(),
    };
    crate::ops::append_entry(&root.join(IMPACT_FILE), IMPACT_TEMPLATE, &entry)
}

/// Record one use of `name` and how it went.
///
/// # Errors
///
/// `skill_not_found` for an unknown skill; `io` on write failure.
pub fn record_use(root: &Path, name: &str, outcome: Outcome, note: Option<&str>) -> Result<()> {
    require_known(root, name)?;
    let mut bullets = vec![format!(
        "outcome: {}",
        match outcome {
            Outcome::Ok => "ok",
            Outcome::Failed => "failed",
        }
    )];
    if let Some(n) = note.map(one_line).filter(|n| !n.is_empty()) {
        bullets.push(format!("note: {n}"));
    }
    append_impact(root, "used", name, &bullets, None)
}

/// Each evidence page as a `[[link]]` to its resolved name.
fn evidence_links(vault: &Vault, evidence: &[String]) -> Result<Vec<String>> {
    evidence
        .iter()
        .map(|e| {
            vault
                .resolve_page(e)
                .map(|p| format!("[[{}]]", p.stem))
                .ok_or_else(|| vault.page_not_found(e))
        })
        .collect()
}

/// Paths that differ between `old` and `new`, sorted.
fn changed_files(old: &Files, new: &Files) -> Vec<String> {
    old.keys()
        .chain(new.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|k| old.get(*k) != new.get(*k))
        // owned: the report outlives the file maps
        .map(ToString::to_string)
        .collect()
}

/// Why `SKILL.md` in `files` cannot be loaded as skill `name`: missing,
/// unparseable, a `name` that is invalid or differs from the folder, or a
/// missing or overlong `description`. Empty when it is fine.
fn skill_md_problems(files: &Files, name: &str) -> Vec<String> {
    let Some(text) = files
        .get(SKILL_FILE)
        .and_then(|b| std::str::from_utf8(b).ok())
    else {
        return vec!["skill folder has no SKILL.md; agents cannot load it".into()];
    };
    let doc = match frontmatter::parse(text) {
        Ok(d) => d,
        Err(e) => return vec![format!("SKILL.md frontmatter does not parse: {e}")],
    };
    let mut out: Vec<String> = name_problem(name).into_iter().collect();
    let declared = frontmatter::get_str(&doc.fm, "name").unwrap_or_default();
    if declared != name {
        out.push(format!(
            "`name: {declared}` must match the folder name '{name}'"
        ));
    }
    let description = frontmatter::get_str(&doc.fm, "description").unwrap_or_default();
    if description.trim().is_empty() {
        out.push("`description` is empty; agents choose skills by it".into());
    } else if description.chars().count() > MAX_DESCRIPTION_CHARS {
        out.push(format!(
            "`description` is longer than {MAX_DESCRIPTION_CHARS} characters"
        ));
    }
    out
}

/// Accept or reject the pending proposal for `name`. The diff, reason and
/// evidence go to `skill-impact.md` first; then an accepted proposal becomes
/// the accepted version and is installed, and a rejected one is undone.
///
/// # Errors
///
/// `skill_not_found` for an unknown skill; `invalid_input` for an invalid
/// name, an empty reason, no pending change, or (accepting) a `SKILL.md`
/// agents cannot load; `page_not_found` for unknown evidence;
/// `already_exists` when an install target holds a skill commonplace did not
/// install; `io` on read or write failure. Nothing is recorded or changed
/// when any of these is returned before the write.
pub fn decide(
    vault: &Vault,
    name: &str,
    decision: Decision,
    reason: &str,
    evidence: &[String],
) -> Result<DecisionReport> {
    let root = &vault.root;
    require_known(root, name)?;
    let reason = one_line(reason);
    if reason.is_empty() {
        return Err(Error::invalid_input(
            "give a --reason: the impact record is what stops a rejected idea from being proposed again",
        ));
    }
    let links = evidence_links(vault, evidence)?;
    let snap = Snapshot::read(root, name)?;
    let (old, new) = (
        snap.accepted.unwrap_or_default(),
        snap.source.unwrap_or_default(),
    );
    if old == new {
        return Err(Error::invalid_input(format!(
            "skill '{name}' has no pending change; edit {SKILLS_DIR}/{name}/ first"
        )));
    }
    // check everything that can refuse an accept before the record is written
    if decision == Decision::Accept && !new.is_empty() {
        let problems = skill_md_problems(&new, name);
        if !problems.is_empty() {
            return Err(Error::invalid_input(format!(
                "{SKILLS_DIR}/{name}/{SKILL_FILE} cannot be loaded by agents: {}",
                problems.join("; ")
            )));
        }
        check_targets(root, &vault.config, name)?;
    }
    let files = changed_files(&old, &new);
    let label = decision.label();
    let mut bullets = vec![format!("reason: {reason}")];
    if !links.is_empty() {
        bullets.push(format!("evidence: {}", links.join(", ")));
    }
    bullets.push(format!("files: {}", files.join(", ")));
    append_impact(root, label, name, &bullets, Some(&diff_files(&old, &new)))?;
    crate::ops::log_append(root, "skill", &format!("{label} {name}"), &[reason])?;
    let installed = match decision {
        Decision::Accept => {
            write_files(&accepted_dir(root, name), &new, false)?;
            install(root, &vault.config, name, &new)?
        }
        Decision::Reject => {
            write_files(&source_dir(root, name), &old, false)?;
            vec![]
        }
    };
    Ok(DecisionReport {
        name: name.to_string(),
        decision: label,
        files,
        installed,
    })
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            Error::invalid_input("cannot find the home folder: neither HOME nor USERPROFILE is set")
        })
}

/// The folders skills are installed into: the project targets, plus the
/// same targets under the home folder when `user` is set.
fn target_bases(root: &Path, cfg: &Config, user: bool) -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = cfg.skills.targets.iter().map(|t| root.join(t)).collect();
    if user {
        let home = home_dir()?;
        v.extend(cfg.skills.targets.iter().map(|t| home.join(t)));
    }
    Ok(v)
}

/// Whether `dest` was installed by the commonplace at `root`. Inside the
/// root any marker counts, so a commonplace that moved still owns its copies.
fn installed_by(dest: &Path, root: &Path) -> bool {
    read_text(&dest.join(MARKER_FILE))
        .is_some_and(|m| dest.starts_with(root) || m.trim() == root.display().to_string())
}

/// The install folders for `name`: the project targets, plus the home ones
/// when it is listed in `[skills] user`.
fn targets_for(root: &Path, cfg: &Config, name: &str) -> Result<Vec<PathBuf>> {
    let user = cfg.skills.user.iter().any(|u| u == name);
    Ok(target_bases(root, cfg, user)?
        .into_iter()
        .map(|b| b.join(name))
        .collect())
}

/// Fail with `already_exists` if any install folder for `name` holds a
/// skill commonplace did not install, before anything is written.
fn check_targets(root: &Path, cfg: &Config, name: &str) -> Result<()> {
    for dest in targets_for(root, cfg, name)? {
        if dest.exists() && !installed_by(&dest, root) {
            return Err(Error::already_exists_with(
                dest.display().to_string(),
                "a skill commonplace did not install has this name; rename one of them",
            ));
        }
    }
    Ok(())
}

/// Install `files` (the accepted version of `name`) into every target, or
/// remove the installed copies of a retired skill (`files` empty). Returns
/// the folders touched.
fn install(root: &Path, cfg: &Config, name: &str, files: &Files) -> Result<Vec<String>> {
    let mut out = vec![];
    if !files.is_empty() {
        check_targets(root, cfg, name)?;
    }
    for dest in targets_for(root, cfg, name)? {
        if files.is_empty() {
            if installed_by(&dest, root) {
                remove_dir(&dest)?;
                out.push(dest.display().to_string());
            }
            continue;
        }
        write_files(&dest, files, true)?;
        // inside the root the marker's presence is enough; outside it names the owner
        let owner = if dest.starts_with(root) {
            ".".to_string()
        } else {
            root.display().to_string()
        };
        fsutil::write_atomic(&dest.join(MARKER_FILE), owner.as_bytes())?;
        out.push(dest.display().to_string());
    }
    Ok(out)
}

/// Install every accepted skill and remove installed copies of retired ones.
/// With `user`, `names` (or every accepted skill when empty) are also
/// installed under the home folder and remembered in `[skills] user`.
///
/// # Errors
///
/// `skill_not_found` for an unknown name; `invalid_input` when the home
/// folder is unknown; `already_exists` when a target holds a skill
/// commonplace did not install; `io` or `parse` on read or write failure.
pub fn sync(root: &Path, user: bool, names_wanted: &[String]) -> Result<SyncReport> {
    let mut cfg = Config::load(root)?;
    let all = names(root);
    if user {
        for n in names_wanted {
            require_known(root, n)?;
        }
        let chosen = if names_wanted.is_empty() {
            &all
        } else {
            names_wanted
        };
        for n in chosen {
            if !cfg.skills.user.contains(n) {
                // the config owns its list; `n` is borrowed from the caller or `all`
                cfg.skills.user.push(n.clone());
            }
        }
        cfg.save(root)?;
    }
    let mut rep = SyncReport::default();
    for name in all {
        let snap = Snapshot::read(root, &name)?;
        let state = snap.state();
        if let Some(accepted) = &snap.accepted {
            rep.installed.extend(install(root, &cfg, &name, accepted)?);
        }
        if state != SkillState::Accepted {
            rep.pending.push(name);
        }
    }
    let any_user = !cfg.skills.user.is_empty();
    for base in target_bases(root, &cfg, any_user)? {
        for n in subdirs(&base) {
            let dest = base.join(&n);
            if !accepted_dir(root, &n).is_dir() && installed_by(&dest, root) {
                remove_dir(&dest)?;
                rep.removed.push(dest.display().to_string());
            }
        }
    }
    Ok(rep)
}

// ---------- lint ----------

fn finding(
    severity: Severity,
    rule: &str,
    path: &str,
    line: Option<usize>,
    message: String,
) -> Finding {
    Finding {
        severity,
        rule: rule.into(),
        path: path.into(),
        line,
        message,
    }
}

/// SKILL-002/004: `PURPOSE.md` links the skill to wiki pages that are current.
/// (It is a page, so a link that does not resolve is LINK-001.)
fn check_purpose(vault: &Vault, name: &str, out: &mut Vec<Finding>) {
    let dir = source_dir(&vault.root, name);
    let rel = format!("{SKILLS_DIR}/{name}/{PURPOSE_FILE}");
    if !dir.join(PURPOSE_FILE).is_file() {
        out.push(finding(Severity::Warning, "SKILL-002", &rel, None,
            "no PURPOSE.md: link the playbooks, error-book entries and concepts this skill was built from".into()));
        return;
    }
    let links = purpose_links(&dir);
    if links.is_empty() {
        out.push(finding(Severity::Warning, "SKILL-002", &rel, None,
            "PURPOSE.md links no wiki pages: a skill without its evidence cannot be revised when the wiki changes".into()));
    }
    for (target, line) in links {
        if let Some(p) = vault.resolve_page(&target)
            && p.get("status").as_deref() == Some("superseded")
        {
            out.push(finding(
                Severity::Warning,
                "SKILL-004",
                &rel,
                Some(line),
                format!("[[{target}]] is superseded; revise the skill against what replaced it"),
            ));
        }
    }
}

/// SKILL-005: wiki links in files agents load, which they cannot follow.
fn check_self_contained(name: &str, source: &Files, out: &mut Vec<Finding>) {
    for (rel, data) in source {
        if rel == PURPOSE_FILE || !fsutil::is_markdown(Path::new(rel)) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(data) else {
            continue;
        };
        if let Some(l) = markdown::wikilinks(text).first() {
            out.push(finding(Severity::Warning, "SKILL-005", &format!("{SKILLS_DIR}/{name}/{rel}"), Some(l.line),
                format!("[[{}]]: agents using the skill do not read the wiki; copy in what they need and put the link in PURPOSE.md", l.target)));
        }
    }
}

/// SKILL-006: a proposal waiting for the gate. SKILL-007: an installed copy out of date.
fn check_state(vault: &Vault, name: &str, snap: &Snapshot, out: &mut Vec<Finding>) {
    let rel = format!("{SKILLS_DIR}/{name}/{SKILL_FILE}");
    let what = match snap.state() {
        SkillState::New => "new skill not yet accepted, so agents do not have it",
        SkillState::Changed => "proposed change not yet accepted",
        SkillState::Removed => "skill folder deleted; accepting retires it",
        SkillState::Accepted => {
            let Some(accepted) = &snap.accepted else {
                return;
            };
            let Ok(bases) = target_bases(&vault.root, &vault.config, false) else {
                return;
            };
            if bases.iter().any(|b| {
                read_files(&b.join(name))
                    .map_or(true, |installed| !is_installed_copy(accepted, &installed))
            }) {
                out.push(finding(Severity::Warning, "SKILL-007", &rel, None,
                    "installed copy is missing or differs from the accepted version; run `commonplace skill sync`".into()));
            }
            return;
        }
    };
    out.push(finding(
        Severity::Info,
        "SKILL-006",
        &rel,
        None,
        format!(
            "{what}; show it with `commonplace skill diff {name}`, then `skill accept` or `skill reject` it"
        ),
    ));
}

/// Whether `installed` is exactly `accepted` without `PURPOSE.md`.
fn is_installed_copy(accepted: &Files, installed: &Files) -> bool {
    let for_agents = accepted.len() - usize::from(accepted.contains_key(PURPOSE_FILE));
    installed.len() == for_agents
        && installed
            .iter()
            .all(|(k, v)| k != PURPOSE_FILE && accepted.get(k) == Some(v))
}

/// Lint every skill (SKILL-001, 002 and 004 to 007).
pub(crate) fn lint(vault: &Vault, out: &mut Vec<Finding>) {
    for name in names(&vault.root) {
        let Ok(snap) = Snapshot::read(&vault.root, &name) else {
            continue;
        };
        if let Some(source) = &snap.source {
            // SKILL-001: SKILL.md exists and its name and description follow the Agent Skills rules
            let rel = format!("{SKILLS_DIR}/{name}/{SKILL_FILE}");
            for problem in skill_md_problems(source, &name) {
                out.push(finding(Severity::Error, "SKILL-001", &rel, None, problem));
            }
            check_purpose(vault, &name, out);
            check_self_contained(&name, source, out);
        }
        check_state(vault, &name, &snap, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_agent_skills_rules() {
        for good in ["a", "weekly-review", "x2"] {
            assert!(name_problem(good).is_none(), "{good}");
        }
        for bad in [
            "",
            "Weekly",
            "-a",
            "a-",
            "a--b",
            "a_b",
            "a b",
            &"a".repeat(65),
        ] {
            assert!(name_problem(bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn only_folders_with_a_skill_md_are_skills() {
        let root = std::env::temp_dir().join(format!("cp-skill-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("skills/weekly-review")).unwrap();
        std::fs::create_dir_all(root.join("skills/cooking")).unwrap();
        std::fs::write(root.join("skills/weekly-review/SKILL.md"), "x").unwrap();
        assert_eq!(
            skill_file(&root, "skills/weekly-review/PURPOSE.md"),
            Some(("weekly-review", true))
        );
        assert_eq!(
            skill_file(&root, "skills/weekly-review/SKILL.md"),
            Some(("weekly-review", false))
        );
        for notes in [
            "skills/cooking/knife-work.md",
            "skills/rust.md",
            "skillsx/a/PURPOSE.md",
        ] {
            assert_eq!(skill_file(&root, notes), None, "{notes}");
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn one_line_flattens_newlines() {
        assert_eq!(one_line(" a\n\n## [x] used | b \n"), "a ## [x] used | b");
    }

    #[test]
    fn title_of_capitalises_the_first_word() {
        assert_eq!(title_of("weekly-review"), "Weekly review");
    }

    #[test]
    fn fence_is_longer_than_any_backtick_run() {
        assert_eq!(fence_for("plain"), "```");
        assert_eq!(fence_for("```rust\n```"), "````");
    }

    #[test]
    fn diff_marks_added_and_removed_files() {
        let old = Files::from([("SKILL.md".into(), b"a\nb\n".to_vec())]);
        let new = Files::from([
            ("SKILL.md".into(), b"a\nc\n".to_vec()),
            ("PURPOSE.md".into(), b"x\n".to_vec()),
        ]);
        let d = diff_files(&old, &new);
        assert!(d.contains("--- /dev/null\n+++ b/PURPOSE.md"), "{d}");
        assert!(d.contains("-b\n+c\n"), "{d}");
    }
}
