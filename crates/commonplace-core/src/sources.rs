//! The evidence chain: ingest, convert, verify and read sources.
//!
//! Immutable raw original -> faithful text layer with page and section
//! anchors -> source note.

use crate::config::Config;
use crate::frontmatter;
use crate::fsutil;
use crate::manifest::{Manifest, SourceRecord};
use crate::{Error, Result};
use regex::Regex;
use serde::Serialize;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub(crate) const RAW_DIR: &str = "sources/raw";
pub(crate) const TEXT_DIR: &str = "sources/text";
/// Folder of source notes, relative to the root.
pub const NOTES_DIR: &str = "sources/notes";

/// What `ingest` registered.
#[derive(Debug, Clone, Serialize)]
pub struct IngestReport {
    /// The new source id (`YYYY-MM-DD Title`).
    pub id: String,
    /// Vault-relative path of the stored original.
    pub raw: String,
    pub(crate) raw_sha256: String,
    /// Vault-relative path of the text layer, when conversion succeeded.
    pub text: Option<String>,
    /// Vault-relative path of the source note.
    pub note: String,
    /// Converter used (`copy`, `pdftotext`, `pandoc`, ...).
    pub converter: String,
    /// How faithful the text layer is (`high`, `medium`, `low`).
    pub fidelity: String,
    /// Page count, when the converter reports pages.
    pub pages: Option<usize>,
    pub(crate) needs_conversion: bool,
    /// Id of an already registered source with identical content (nothing was ingested).
    pub duplicate_of: Option<String>,
    /// Problems that did not stop the ingest.
    pub warnings: Vec<String>,
}

/// Overrides and metadata for `ingest`.
pub struct IngestOptions {
    /// Title (default: from the file name or document).
    pub title: Option<String>,
    /// Publication date `YYYY-MM-DD` (default: today or a date prefix in the title).
    pub date: Option<String>,
    /// Source kind (document, judgment, article, ...).
    pub kind: Option<String>,
    /// Where it came from (URL, "email from X", "scan").
    pub origin: Option<String>,
    /// Project the source belongs to.
    pub project: Option<String>,
    /// Confidentiality level.
    pub confidentiality: Option<String>,
    /// Authority level (primary, secondary, ...).
    pub authority: Option<String>,
    /// Converter to use instead of the automatic choice.
    pub converter: Option<String>,
    /// Keep the original in place and only copy (default) vs move.
    pub move_file: bool,
    /// Register the file even if the same content is already registered.
    pub force: bool,
}

/// Build a source id `YYYY-MM-DD Short Title` from a title and date.
pub(crate) fn make_id(title: &str, date: &str) -> String {
    let mut t = fsutil::safe_title(title);
    if t.chars().count() > 80 {
        t = t
            .chars()
            .take(80)
            .collect::<String>()
            .trim_end()
            .to_string();
    }
    format!("{date} {t}")
}

static DATE_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| crate::re(r"^(\d{4}-\d{2}-\d{2})\s+(.+)$"));

/// The report for a file whose hash is already registered.
fn duplicate_report(id: &str, rec: &SourceRecord, sha: String) -> IngestReport {
    IngestReport {
        id: id.to_string(),
        raw: rec.raw.clone(),
        raw_sha256: sha,
        text: rec.text.clone(),
        note: rec.note.clone().unwrap_or_default(),
        converter: rec.converter.clone().unwrap_or_default(),
        fidelity: String::new(),
        pages: None,
        needs_conversion: false,
        duplicate_of: Some(id.to_string()),
        warnings: vec![
            "identical file already ingested; nothing done (use --force to ingest again)".into(),
        ],
    }
}

/// Title and date from the options, else from the file name (`YYYY-MM-DD Title` is split).
fn title_and_date(opts: &IngestOptions, file_stem: &str) -> (String, String) {
    let t = opts
        .title
        .clone()
        .unwrap_or_else(|| file_stem.replace(['_', '-'], " "));
    match DATE_PREFIX_RE.captures(&t) {
        Some(c) if opts.date.is_none() => (c[2].to_string(), c[1].to_string()),
        _ => (t, opts.date.clone().unwrap_or_else(fsutil::today)),
    }
}

/// `id`, or `id (n)` with the first free `n` when the id is taken.
fn unique_id(
    manifest: &Manifest,
    notes_dir: &Path,
    id: String,
    warnings: &mut Vec<String>,
) -> String {
    let taken = |candidate: &str| {
        manifest.sources.contains_key(candidate)
            || notes_dir.join(format!("{candidate}.md")).exists()
    };
    if !taken(&id) {
        return id;
    }
    let mut n = 2;
    while taken(&format!("{id} ({n})")) {
        n += 1;
    }
    warnings.push(format!("a source named '{id}' already exists; using '{id} ({n})'. If this is the same document in a new version, consider `supersedes`."));
    format!("{id} ({n})")
}

/// Copy (or move) the original into `sources/raw` and make it read-only.
fn store_raw(src: &Path, raw_path: &Path, move_file: bool) -> Result<()> {
    if move_file {
        std::fs::rename(src, raw_path).or_else(|_| {
            std::fs::copy(src, raw_path)
                .map(|_| ())
                .and_then(|()| std::fs::remove_file(src))
        })?;
    } else {
        std::fs::copy(src, raw_path).map_err(|e| Error::io("copying to", raw_path, e))?;
    }
    set_readonly(raw_path);
    Ok(())
}

/// Write the text layer for a converted source; returns its sha256.
/// Provenance written into a text layer's frontmatter.
struct TextLayerHeader<'a> {
    id: &'a str,
    raw_rel: &'a str,
    raw_sha: &'a str,
    /// Lowercase extension of the original, for the warning.
    ext: &'a str,
    conv: &'a ConvertResult,
}

/// The text layer an ingest wrote: (path, sha256, page count), all `None`
/// when no converter could handle the file.
type WrittenLayer = (Option<String>, Option<String>, Option<usize>);

/// Write the converted text layer, or, when conversion failed, add a warning
/// telling the agent how to write it by hand.
fn write_text_layer(
    root: &Path,
    header: &TextLayerHeader<'_>,
    warnings: &mut Vec<String>,
) -> Result<WrittenLayer> {
    let (id, ext) = (header.id, header.ext);
    let text_rel = format!("{TEXT_DIR}/{id}.text.md");
    let Some(c) = &header.conv.markdown else {
        warnings.push(format!("no converter could handle .{ext}: {}. Write the text layer yourself at {text_rel} (see references/ingest.md) and run `commonplace source attach-text \"{id}\"`.", header.conv.reason));
        return Ok((None, None, None));
    };
    let mut fm = serde_yaml::Mapping::new();
    frontmatter::set_str(&mut fm, "type", "source-text");
    frontmatter::set_str(&mut fm, "source", header.id);
    frontmatter::set_str(&mut fm, "raw", header.raw_rel);
    frontmatter::set_str(&mut fm, "raw_sha256", header.raw_sha);
    frontmatter::set_str(&mut fm, "converter", &header.conv.converter);
    frontmatter::set_str(&mut fm, "fidelity", &header.conv.fidelity);
    if let Some(p) = c.pages {
        frontmatter::set_value(&mut fm, "pages", serde_yaml::Value::Number(p.into()));
    }
    frontmatter::set_str(
        &mut fm,
        "anchors",
        if c.pages.is_some() { "page" } else { "line" },
    );
    frontmatter::set_str(
        &mut fm,
        "note",
        "Faithful rendition of the original. Do not edit; re-convert instead.",
    );
    let text = frontmatter::render(&fm, &c.text)?;
    fsutil::write_atomic(&root.join(&text_rel), text.as_bytes())?;
    Ok((Some(text_rel), Some(fsutil::sha256_str(&text)), c.pages))
}

/// What a new source note records about its source.
struct NoteFields<'a> {
    title: &'a str,
    date: &'a str,
    kind: &'a str,
    raw_rel: &'a str,
    /// The text layer path, or None when the source still needs conversion.
    text_rel: Option<&'a str>,
}

/// The new source note: frontmatter plus the L1/L2/L3 summary skeleton.
fn render_source_note(
    f: &NoteFields<'_>,
    opts: &IngestOptions,
    conv: &ConvertResult,
) -> Result<String> {
    let today = fsutil::today();
    let mut fm = serde_yaml::Mapping::new();
    let strings = [
        ("type", "source"),
        ("title", f.title),
        ("kind", f.kind),
        ("raw", f.raw_rel),
        ("text", f.text_rel.unwrap_or("")),
        ("origin", opts.origin.as_deref().unwrap_or("")),
        ("retrieved", &today),
        ("published", f.date),
        ("author", ""),
        ("publisher", ""),
        ("authority", opts.authority.as_deref().unwrap_or("primary")),
        (
            "confidentiality",
            opts.confidentiality.as_deref().unwrap_or("personal"),
        ),
        ("fidelity", &conv.fidelity),
        ("converter", &conv.converter),
        (
            "status",
            if f.text_rel.is_some() {
                "converted"
            } else {
                "registered"
            },
        ),
    ];
    for (k, v) in strings {
        frontmatter::set_str(&mut fm, k, v);
    }
    frontmatter::set_value(
        &mut fm,
        "summary_level",
        serde_yaml::Value::Number(0.into()),
    );
    frontmatter::set_value(&mut fm, "tags", serde_yaml::Value::Sequence(vec![]));
    let projects = opts
        .project
        .iter()
        .map(|p| serde_yaml::Value::String(format!("[[{p}]]")))
        .collect();
    frontmatter::set_value(&mut fm, "projects", serde_yaml::Value::Sequence(projects));
    frontmatter::set_str(&mut fm, "description", "");
    frontmatter::set_str(&mut fm, "created", &today);
    frontmatter::set_str(&mut fm, "updated", &today);
    let body = format!(
        "# {}\n\n## TL;DR\n\n<!-- one or two sentences: what this is and why it matters (L1) -->\n\n## Summary\n\n<!-- up to ~300 words in your own words (L2) -->\n\n## Key quotes\n\n<!-- verbatim, anchored: > \"…\" — p.12 / para 26 / §3.2 (L3). These feed citations. -->\n\n## Structure\n\n{}\n## Relevance and open questions\n\n## Related\n",
        f.title,
        conv.outline.as_deref().unwrap_or_default()
    );
    frontmatter::render(&fm, &body)
}

/// An original stored under `sources/raw` with its new id.
struct StoredRaw {
    id: String,
    title: String,
    date: String,
    /// Lowercase file extension, or empty.
    ext: String,
    raw_rel: String,
    raw_path: PathBuf,
}

/// Name the source, create the source folders, and copy (or move) the original in.
fn store_original(
    root: &Path,
    manifest: &Manifest,
    src: &Path,
    opts: &IngestOptions,
    warnings: &mut Vec<String>,
) -> Result<StoredRaw> {
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (title, date) = title_and_date(opts, &fsutil::stem(src));
    for dir in [RAW_DIR, NOTES_DIR, TEXT_DIR] {
        std::fs::create_dir_all(root.join(dir))?;
    }
    let id = unique_id(
        manifest,
        &root.join(NOTES_DIR),
        make_id(&title, &date),
        warnings,
    );
    let raw_name = if ext.is_empty() {
        id.clone()
    } else {
        format!("{id}.{ext}")
    };
    let raw_path = root.join(RAW_DIR).join(&raw_name);
    store_raw(src, &raw_path, opts.move_file)?;
    Ok(StoredRaw {
        id,
        title,
        date,
        ext,
        raw_rel: format!("{RAW_DIR}/{raw_name}"),
        raw_path,
    })
}

/// Register `src` as an immutable source: store the original, convert it to a text layer and write its note.
///
/// # Errors
///
/// `invalid_input` when `src` is not a file; `conversion` or `io` failures. A duplicate is not an error: it is reported in `duplicate_of`.
pub fn ingest(
    root: &Path,
    config: &Config,
    src: &Path,
    opts: &IngestOptions,
) -> Result<IngestReport> {
    if !src.is_file() {
        return Err(Error::invalid_input(format!(
            "{} is not a file",
            src.display()
        )));
    }
    let mut warnings = vec![];
    let sha = fsutil::sha256_file(src)?;
    let mut manifest = Manifest::load(root)?;
    if let Some((id, rec)) = manifest.find_by_hash(&sha)
        && !opts.force
    {
        return Ok(duplicate_report(id, rec, sha));
    }
    let StoredRaw {
        id,
        title,
        date,
        ext,
        raw_rel,
        raw_path,
    } = store_original(root, &manifest, src, opts, &mut warnings)?;
    let kind = opts.kind.clone().unwrap_or_else(|| guess_kind(&ext));
    let conv = convert(
        &raw_path,
        &ext,
        config,
        opts.converter.as_deref(),
        opts.origin.as_deref(),
    )?;
    let needs_conversion = conv.markdown.is_none();
    let header = TextLayerHeader {
        id: &id,
        raw_rel: &raw_rel,
        raw_sha: &sha,
        ext: &ext,
        conv: &conv,
    };
    let (text, text_sha, pages) = write_text_layer(root, &header, &mut warnings)?;

    let note_rel = format!("{NOTES_DIR}/{id}.md");
    let fields = NoteFields {
        title: &title,
        date: &date,
        kind: &kind,
        raw_rel: &raw_rel,
        text_rel: text.as_deref(),
    };
    let note_text = render_source_note(&fields, opts, &conv)?;
    fsutil::write_atomic(&root.join(&note_rel), note_text.as_bytes())?;

    let record = SourceRecord {
        raw: raw_rel.clone(),
        raw_sha256: sha.clone(),
        text: text.clone(),
        text_sha256: text_sha,
        note: Some(note_rel.clone()),
        ingested: fsutil::now_iso(),
        converter: Some(conv.converter.clone()),
        origin: opts.origin.clone(),
    };
    manifest.sources.insert(id.clone(), record);
    manifest.save(root)?;
    warnings.extend(conv.warning);
    Ok(IngestReport {
        id,
        raw: raw_rel,
        raw_sha256: sha,
        text,
        note: note_rel,
        converter: conv.converter,
        fidelity: conv.fidelity,
        pages,
        needs_conversion,
        duplicate_of: None,
        warnings,
    })
}

fn set_readonly(p: &Path) {
    if let Ok(md) = std::fs::metadata(p) {
        let mut perms = md.permissions();
        perms.set_readonly(true);
        // best effort: a writable original is still hashed and verified
        let _ = std::fs::set_permissions(p, perms);
    }
}

pub(crate) fn guess_kind(ext: &str) -> String {
    match ext {
        "pdf" | "docx" | "doc" | "odt" | "rtf" => "document",
        "pptx" | "ppt" => "slides",
        "xlsx" | "csv" | "json" => "dataset",
        "html" | "htm" => "web",
        "md" | "txt" => "note",
        "eml" | "msg" => "email",
        "png" | "jpg" | "jpeg" | "gif" | "webp" => "image",
        "mp3" | "m4a" | "wav" | "mp4" | "vtt" | "srt" => "transcript",
        _ => "other",
    }
    .to_string()
}

#[derive(Debug, Clone)]
pub(crate) struct Converted {
    pub(crate) text: String,
    pub(crate) pages: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct ConvertResult {
    pub(crate) markdown: Option<Converted>,
    pub(crate) converter: String,
    pub(crate) fidelity: String,
    pub(crate) reason: String,
    pub(crate) warning: Option<String>,
    pub(crate) outline: Option<String>,
}

impl ConvertResult {
    /// A successful conversion to markdown without page anchors; the outline comes from its headings.
    fn markdown(text: String, converter: &str, fidelity: &str, warning: Option<String>) -> Self {
        let outline = outline_from_markdown(&text);
        ConvertResult {
            markdown: Some(Converted { text, pages: None }),
            converter: converter.into(),
            fidelity: fidelity.into(),
            reason: String::new(),
            warning,
            outline,
        }
    }

    fn none(reason: String) -> Self {
        ConvertResult {
            markdown: None,
            converter: "none".into(),
            fidelity: "none".into(),
            reason,
            warning: None,
            outline: None,
        }
    }
}

/// Run an external converter; `None` when it exits unsuccessfully.
fn run_converter(cmd: &mut std::process::Command) -> Result<Option<String>> {
    let out = cmd.output()?;
    Ok(out
        .status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).to_string()))
}

fn convert_pdftotext(raw: &Path) -> Result<Option<ConvertResult>> {
    let Some(stdout) = run_converter(
        std::process::Command::new("pdftotext")
            .args(["-layout", "-enc", "UTF-8"])
            .arg(raw)
            .arg("-"),
    )?
    else {
        return Ok(None);
    };
    let (text, pages) = page_markers_from_formfeeds(&stdout);
    let sparse = text.len().checked_div(pages).unwrap_or(text.len()) < 200;
    Ok(Some(ConvertResult {
        markdown: Some(Converted {
            text,
            pages: Some(pages),
        }),
        converter: "pdftotext".into(),
        fidelity: if sparse { "low" } else { "medium" }.into(),
        reason: String::new(),
        warning: sparse.then(|| "very little text per page: probably a scanned PDF that needs OCR (docling/marker) or manual conversion".into()),
        outline: None,
    }))
}

/// Convert a raw file into markdown. Built in: md, txt, html. External (if
/// installed, in the configured preference order): pdftotext for PDF; pandoc
/// for docx/odt/rtf/epub; markitdown for office formats. Anything else is left
/// for the agent (`needs_conversion`).
pub(crate) fn convert(
    raw: &Path,
    ext: &str,
    config: &Config,
    forced: Option<&str>,
    origin: Option<&str>,
) -> Result<ConvertResult> {
    match ext {
        "md" | "markdown" | "txt" | "text" => {
            let t = strip_frontmatter(&std::fs::read_to_string(raw)?);
            return Ok(ConvertResult::markdown(t, "copy", "high", None));
        }
        "html" | "htm" => {
            let md = html_to_markdown(&std::fs::read_to_string(raw)?, origin)?;
            return Ok(ConvertResult::markdown(md, "htmd", "high", None));
        }
        _ => {}
    }
    let available = available_converters();
    let order: Vec<String> = match forced {
        Some(f) => vec![f.to_string()],
        None => config.converters.prefer.clone(),
    };
    for name in order.iter().filter(|n| available.contains(n)) {
        let result = match (name.as_str(), ext) {
            ("pdftotext", "pdf") => convert_pdftotext(raw)?,
            ("pandoc", "docx" | "odt" | "rtf" | "epub" | "html") => run_converter(
                std::process::Command::new("pandoc")
                    .arg(raw)
                    .args(["-t", "gfm", "--wrap=none"]),
            )?
            .map(|md| ConvertResult::markdown(md, "pandoc", "high", None)),
            ("markitdown", "docx" | "pptx" | "xlsx" | "pdf") => {
                run_converter(std::process::Command::new("markitdown").arg(raw))?.map(|md| {
                    if ext == "pdf" {
                        ConvertResult::markdown(md, "markitdown", "medium", Some("markitdown drops page boundaries; add <!-- page:N --> markers or re-convert with pdftotext for page-level citations".into()))
                    } else {
                        ConvertResult::markdown(md, "markitdown", "high", None)
                    }
                })
            }
            _ => None,
        };
        if let Some(r) = result {
            return Ok(r);
        }
    }
    Ok(ConvertResult::none(format!(
        "no installed converter handles .{ext} (available: {})",
        if available.is_empty() {
            "none".to_string()
        } else {
            available.join(", ")
        }
    )))
}

/// Tags dropped from web pages: site chrome, not content. The article itself
/// (headings, paragraphs, tables, lists, links) is kept verbatim.
const HTML_SKIP_TAGS: &[&str] = &[
    "script", "style", "noscript", "template", "iframe", "svg", "nav", "footer",
];

/// HTML -> markdown. Site chrome is dropped and every relative link or image is
/// resolved against `origin`, so the text layer holds no links that a vault
/// viewer such as Obsidian would read as links to local files.
pub(crate) fn html_to_markdown(html: &str, origin: Option<&str>) -> Result<String> {
    let conv = htmd::HtmlToMarkdown::builder()
        .skip_tags(HTML_SKIP_TAGS.to_vec())
        .build();
    let md = conv
        .convert(html)
        .map_err(|e| Error::conversion(format!("html conversion failed: {e}")))?;
    Ok(match origin {
        Some(o) if o.contains("://") => absolutise_links(&md, o, |_| false).0,
        _ => md,
    })
}

static MD_LINK_URL_RE: LazyLock<Regex> = LazyLock::new(|| crate::re(r"\]\(<?([^)\s<>]+)>?"));

/// Rewrite relative markdown link and image targets to absolute URLs against
/// `base`. `keep(href)` returning true leaves that target alone (used to
/// protect links that point at real vault files). Returns the text and the
/// number of targets rewritten.
pub(crate) fn absolutise_links(
    md: &str,
    base: &str,
    keep: impl Fn(&str) -> bool,
) -> (String, usize) {
    let mut n = 0usize;
    let out = MD_LINK_URL_RE.replace_all(md, |c: &regex::Captures<'_>| {
        let href = &c[1];
        if keep(href) {
            return c[0].to_string();
        }
        match resolve_url(base, href) {
            Some(abs) => {
                n += 1;
                format!("]({abs}")
            }
            None => c[0].to_string(),
        }
    });
    (out.into_owned(), n)
}

/// Resolve `href` against `base` (an absolute http(s) URL). Returns None when
/// the target is already absolute, a fragment, or a non-web scheme.
pub(crate) fn resolve_url(base: &str, href: &str) -> Option<String> {
    static SCHEME_RE: LazyLock<Regex> = LazyLock::new(|| crate::re(r"^[A-Za-z][A-Za-z0-9+.\-]*:"));
    if href.is_empty() || href.starts_with('#') || SCHEME_RE.is_match(href) {
        return None;
    }
    let (scheme, rest) = base.split_once("://")?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let path = path.split(['?', '#']).next().unwrap_or("/");
    if let Some(r) = href.strip_prefix("//") {
        return Some(format!("{scheme}://{r}"));
    }
    if href.starts_with('/') {
        return Some(format!("{scheme}://{host}{}", normalise_path(href)));
    }
    if href.starts_with('?') {
        return Some(format!("{scheme}://{host}{path}{href}"));
    }
    let dir = &path[..=path.rfind('/').unwrap_or(0)];
    Some(format!(
        "{scheme}://{host}{}",
        normalise_path(&format!("{dir}{href}"))
    ))
}

fn normalise_path(p: &str) -> String {
    let (path, tail) = match p.find(['?', '#']) {
        Some(i) => (&p[..i], &p[i..]),
        None => (p, ""),
    };
    let mut segs: Vec<&str> = vec![];
    for s in path.split('/') {
        match s {
            "." => {}
            ".." => {
                segs.pop();
            }
            _ => segs.push(s),
        }
    }
    let mut out = segs.join("/");
    if !out.starts_with('/') {
        out.insert(0, '/');
    }
    if path.ends_with('/') && !out.ends_with('/') {
        out.push('/');
    }
    out + tail
}

/// What `source relink` changed.
#[derive(Debug, Clone, Serialize)]
pub struct RelinkReport {
    /// Source id.
    pub id: String,
    pub(crate) origin: Option<String>,
    /// Files whose links were rewritten.
    pub files_changed: Vec<String>,
    /// Number of links rewritten.
    pub links_rewritten: usize,
    /// Why the source was skipped, if it was.
    pub skipped: Option<String>,
}

/// Rewrite relative web links in an existing source's text layer and note to
/// absolute URLs against the recorded origin, then re-hash the text layer.
/// Repairs sources ingested before the converter did this itself. Line
/// numbers do not move, so `L` locators and quotes stay valid.
///
/// # Errors
///
/// `source_not_found` for an unknown id; `io` or `parse` when a file cannot be read, written or parsed.
pub fn relink(root: &Path, id: &str) -> Result<RelinkReport> {
    let mut manifest = Manifest::load(root)?;
    let rec = manifest
        .sources
        .get_mut(id)
        .ok_or_else(|| Error::source_not_found(id))?;
    let mut rep = RelinkReport {
        id: id.to_string(),
        origin: rec.origin.clone(),
        files_changed: vec![],
        links_rewritten: 0,
        skipped: None,
    };
    let Some(origin) = rec.origin.clone().filter(|o| o.contains("://")) else {
        rep.skipped = Some("no web origin recorded for this source (set `origin` in .commonplace/manifest.json or re-ingest with --origin)".into());
        return Ok(rep);
    };
    let mut targets: Vec<PathBuf> = vec![];
    if let Some(p) = text_layer_path(root, id) {
        if p.is_file() {
            targets.push(p);
        } else {
            targets.extend(
                std::fs::read_dir(&p)?
                    .flatten()
                    .map(|e| e.path())
                    .filter(|f| fsutil::is_markdown(f)),
            );
        }
    }
    let note_rel = rec
        .note
        .clone()
        .unwrap_or_else(|| format!("{NOTES_DIR}/{id}.md"));
    let note_path = root.join(&note_rel);
    if note_path.is_file() {
        targets.push(note_path);
    }
    for f in targets {
        let t = std::fs::read_to_string(&f)?;
        let dir = f
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        let is_vault_file = |href: &str| {
            let h = href.split(['?', '#']).next().unwrap_or("");
            !h.is_empty() && (dir.join(h).exists() || root.join(h.trim_start_matches('/')).exists())
        };
        let (out, n) = absolutise_links(&t, &origin, is_vault_file);
        if n > 0 {
            fsutil::write_atomic(&f, out.as_bytes())?;
            rep.files_changed.push(fsutil::rel(root, &f));
            rep.links_rewritten += n;
        }
    }
    if rep.links_rewritten > 0 {
        if let Some(p) = text_layer_path(root, id) {
            let hash = if p.is_file() {
                fsutil::sha256_str(&std::fs::read_to_string(&p)?)
            } else {
                fsutil::sha256_str(
                    &load_text_layer(root, id)?
                        .map(|t| t.text)
                        .unwrap_or_default(),
                )
            };
            rec.text_sha256 = Some(hash);
        }
        manifest.save(root)?;
    }
    Ok(rep)
}

/// Converters found on `PATH`.
pub fn available_converters() -> Vec<String> {
    [
        "docling",
        "marker_single",
        "markitdown",
        "pandoc",
        "pdftotext",
    ]
    .iter()
    .filter(|c| which(c))
    .map(|c| {
        if *c == "marker_single" {
            "marker".to_string()
        } else {
            c.to_string()
        }
    })
    .collect()
}

/// Whether `cmd` is an executable on `PATH`.
pub fn which(cmd: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|d| {
        let p = d.join(cmd);
        p.is_file() || d.join(format!("{cmd}.exe")).is_file()
    })
}

/// pdftotext separates pages with form feeds; turn them into page markers.
pub(crate) fn page_markers_from_formfeeds(txt: &str) -> (String, usize) {
    let mut out = String::with_capacity(txt.len() + 64);
    let mut n = 0;
    for (i, page) in txt.split('\u{c}').enumerate() {
        if i == txt.split('\u{c}').count() - 1 && page.trim().is_empty() {
            break;
        }
        n = i + 1;
        let _ = writeln!(out, "<!-- page:{n} -->");
        out.push_str(page.trim_end_matches('\n'));
        out.push('\n');
    }
    (out, n)
}

fn strip_frontmatter(t: &str) -> String {
    match frontmatter::parse(t) {
        Ok(d) if d.has_frontmatter => d.body,
        _ => t.to_string(),
    }
}

fn outline_from_markdown(md: &str) -> Option<String> {
    let hs = crate::markdown::headings(md);
    if hs.is_empty() {
        return None;
    }
    let mut s = String::new();
    for h in hs.iter().take(60) {
        let _ = writeln!(s, "{}- {}", "  ".repeat(h.level.saturating_sub(1)), h.text);
    }
    Some(s)
}

/// Locate the text layer for a source id: a single `<id>.text.md` file or a
/// directory `<id>/` of section files (sorted by name). Returns the virtual
/// concatenated text plus a map from virtual line to (file, line).
pub struct TextLayer {
    /// The concatenated text of every part.
    pub text: String,
    pub(crate) parts: Vec<TextPart>,
}

pub(crate) struct TextPart {
    pub(crate) rel: String,
    pub(crate) start_line: usize, // 1-based virtual line where this part's body starts
    pub(crate) lines: usize,
}

impl TextLayer {
    /// Map a virtual line to `L<n>`, or `<file>:L<n>` for multi-file layers.
    pub fn locate(&self, vline: usize) -> String {
        if self.parts.len() <= 1 {
            return format!("L{vline}");
        }
        for p in &self.parts {
            if vline >= p.start_line && vline < p.start_line + p.lines {
                return format!("{}:L{}", p.rel, vline - p.start_line + 1);
            }
        }
        format!("L{vline}")
    }
}

pub(crate) fn text_layer_path(root: &Path, id: &str) -> Option<PathBuf> {
    let single = root.join(TEXT_DIR).join(format!("{id}.text.md"));
    if single.is_file() {
        return Some(single);
    }
    let dir = root.join(TEXT_DIR).join(id);
    if dir.is_dir() {
        return Some(dir);
    }
    None
}

/// Load the text layer of source `id`; `None` when it has none.
///
/// # Errors
///
/// Returns an `io` or `parse` error when a file cannot be read, written or parsed.
pub fn load_text_layer(root: &Path, id: &str) -> Result<Option<TextLayer>> {
    let Some(p) = text_layer_path(root, id) else {
        return Ok(None);
    };
    if p.is_file() {
        let t = std::fs::read_to_string(&p)?;
        let body = strip_frontmatter(&t);
        let lines = body.lines().count();
        return Ok(Some(TextLayer {
            text: body,
            parts: vec![TextPart {
                rel: fsutil::rel(root, &p),
                start_line: 1,
                lines,
            }],
        }));
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&p)?
        .flatten()
        .map(|e| e.path())
        .filter(|f| fsutil::is_markdown(f))
        .collect();
    files.sort();
    let mut text = String::new();
    let mut parts = vec![];
    let mut cur_line = 1usize;
    for f in files {
        let t = std::fs::read_to_string(&f)?;
        let doc = frontmatter::parse(&t).unwrap_or_default();
        let mut header = String::new();
        let sec = frontmatter::get_str(&doc.fm, "section")
            .or_else(|| frontmatter::get_str(&doc.fm, "section_id"));
        if let Some(s) = sec {
            let _ = writeln!(header, "<!-- section:{s} -->");
        }
        if let Some(pf) = frontmatter::get_str(&doc.fm, "page_from")
            .or_else(|| frontmatter::get_str(&doc.fm, "pages_from"))
            && !doc.body.contains("<!-- page:")
        {
            let _ = writeln!(header, "<!-- page:{pf} -->");
        }
        let body = if doc.has_frontmatter { doc.body } else { t };
        let header_lines = header.lines().count();
        let body_lines = body.lines().count();
        text.push_str(&header);
        text.push_str(&body);
        if !body.ends_with('\n') {
            text.push('\n');
        }
        parts.push(TextPart {
            rel: fsutil::rel(root, &f),
            start_line: cur_line + header_lines,
            lines: body_lines,
        });
        cur_line += header_lines + body_lines;
    }
    Ok(Some(TextLayer { text, parts }))
}

/// The integrity check of one source.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyEntry {
    /// Source id.
    pub id: String,
    /// Whether the original still matches its hash (`None` when it is missing).
    pub raw_ok: Option<bool>,
    pub(crate) text_ok: Option<bool>,
    /// Human-readable result.
    pub message: String,
}

/// Re-hash every registered raw original and text layer.
///
/// # Errors
///
/// `io` or `parse` when a file cannot be read, written or parsed.
pub fn verify_integrity(root: &Path) -> Result<Vec<VerifyEntry>> {
    let manifest = Manifest::load(root)?;
    let mut out = vec![];
    for (id, rec) in &manifest.sources {
        let raw_path = root.join(&rec.raw);
        let raw_ok = if raw_path.is_file() {
            Some(fsutil::sha256_file(&raw_path)? == rec.raw_sha256)
        } else {
            None
        };
        let text_ok = match (&rec.text, &rec.text_sha256) {
            (Some(t), Some(h)) => {
                let p = root.join(t);
                if p.is_file() {
                    Some(fsutil::sha256_str(&std::fs::read_to_string(&p)?) == *h)
                } else {
                    None
                }
            }
            _ => None,
        };
        let message = match (raw_ok, text_ok) {
            (Some(false), _) => "RAW ORIGINAL MODIFIED OR REPLACED since ingest".to_string(),
            (None, _) => "raw original missing".to_string(),
            (_, Some(false)) => "text layer edited since ingest (citations may be stale; re-run `commonplace source attach-text` if intentional)".to_string(),
            _ => "ok".to_string(),
        };
        out.push(VerifyEntry {
            id: id.clone(),
            raw_ok,
            text_ok,
            message,
        });
    }
    Ok(out)
}

/// Register a hand-written or externally converted text layer for a source
/// (sets hashes, updates the note's `text`/`status`/`fidelity`).
///
/// # Errors
///
/// `source_not_found` for an unknown id; `text_layer_missing` when no text layer file exists; `io` or `parse` when a file cannot be read, written or parsed.
pub fn attach_text(root: &Path, id: &str, converter: &str, fidelity: &str) -> Result<String> {
    let mut manifest = Manifest::load(root)?;
    let rec = manifest
        .sources
        .get_mut(id)
        .ok_or_else(|| Error::source_not_found(id))?;
    let p = text_layer_path(root, id).ok_or_else(|| {
        Error::text_layer_missing(format!(
            "no text layer found at {TEXT_DIR}/{id}.text.md or {TEXT_DIR}/{id}/"
        ))
    })?;
    let rel = fsutil::rel(root, &p);
    let hash = if p.is_file() {
        fsutil::sha256_str(&std::fs::read_to_string(&p)?)
    } else {
        fsutil::sha256_str(
            &load_text_layer(root, id)?
                .map(|t| t.text)
                .unwrap_or_default(),
        )
    };
    rec.text = Some(rel.clone());
    rec.text_sha256 = Some(hash);
    rec.converter = Some(converter.to_string());
    let note_rel = rec
        .note
        .clone()
        .unwrap_or_else(|| format!("{NOTES_DIR}/{id}.md"));
    manifest.save(root)?;
    let note_path = root.join(&note_rel);
    if note_path.is_file() {
        let t = std::fs::read_to_string(&note_path)?;
        let mut doc = frontmatter::parse(&t)?;
        frontmatter::set_str(&mut doc.fm, "text", &rel);
        frontmatter::set_str(&mut doc.fm, "converter", converter);
        frontmatter::set_str(&mut doc.fm, "fidelity", fidelity);
        if frontmatter::get_str(&doc.fm, "status").as_deref() == Some("registered") {
            frontmatter::set_str(&mut doc.fm, "status", "converted");
        }
        frontmatter::set_str(&mut doc.fm, "updated", &fsutil::today());
        fsutil::write_atomic(
            &note_path,
            frontmatter::render(&doc.fm, &doc.body)?.as_bytes(),
        )?;
    }
    Ok(rel)
}

/// Source ids known to the manifest or present as notes.
pub(crate) fn known_source_ids(root: &Path) -> Vec<String> {
    let mut ids: Vec<String> = Manifest::load(root)
        .map(|m| m.sources.keys().cloned().collect())
        .unwrap_or_default();
    if let Ok(rd) = std::fs::read_dir(root.join(NOTES_DIR)) {
        for e in rd.flatten() {
            if fsutil::is_markdown(&e.path()) {
                let s = fsutil::stem(&e.path());
                if !ids.contains(&s) {
                    ids.push(s);
                }
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str =
        "https://www.privatehealth.gov.au/health_insurance/howitworks/waiting_periods.htm?x=1#top";

    #[test]
    fn resolves_relative_links() {
        assert_eq!(
            resolve_url(BASE, "/faq.htm").as_deref(),
            Some("https://www.privatehealth.gov.au/faq.htm")
        );
        assert_eq!(
            resolve_url(BASE, "out_of_pocket.htm").as_deref(),
            Some("https://www.privatehealth.gov.au/health_insurance/howitworks/out_of_pocket.htm")
        );
        assert_eq!(
            resolve_url(BASE, "../index.htm").as_deref(),
            Some("https://www.privatehealth.gov.au/health_insurance/index.htm")
        );
        assert_eq!(
            resolve_url(BASE, "//cdn.example.org/a.svg").as_deref(),
            Some("https://cdn.example.org/a.svg")
        );
        assert_eq!(
            resolve_url(BASE, "?page=2").as_deref(),
            Some(
                "https://www.privatehealth.gov.au/health_insurance/howitworks/waiting_periods.htm?page=2"
            )
        );
        assert_eq!(resolve_url(BASE, "https://ato.gov.au/x"), None);
        assert_eq!(resolve_url(BASE, "#section"), None);
        assert_eq!(resolve_url(BASE, "mailto:a@b.c"), None);
        assert_eq!(
            resolve_url("https://example.org", "a/b.htm").as_deref(),
            Some("https://example.org/a/b.htm")
        );
    }

    #[test]
    fn absolutises_markdown_links_and_images() {
        let md = "See [FAQ](/faq.htm \"Questions\") and ![logo](/assets/logo.svg) and [ext](https://x.org/) and [same](#top) and [keep](../raw/x.pdf).";
        let (out, n) = absolutise_links(md, BASE, |h| h == "../raw/x.pdf");
        assert_eq!(n, 2);
        assert!(out.contains("[FAQ](https://www.privatehealth.gov.au/faq.htm \"Questions\")"));
        assert!(out.contains("![logo](https://www.privatehealth.gov.au/assets/logo.svg)"));
        assert!(out.contains("[ext](https://x.org/)"));
        assert!(out.contains("[same](#top)"));
        assert!(out.contains("[keep](../raw/x.pdf)"));
    }

    #[test]
    fn html_conversion_drops_chrome_and_absolutises() {
        let html = r#"<html><head><title>T</title><style>p{}</style></head><body>
<nav><a href="/faq.htm">FAQ</a></nav>
<main><h1>Waiting periods</h1><p>See <a href="out_of_pocket.htm">costs</a>.</p></main>
<footer><a href="/privacy.htm">Privacy</a></footer></body></html>"#;
        let md = html_to_markdown(html, Some(BASE)).unwrap();
        assert!(md.contains("# Waiting periods"));
        assert!(md.contains("[costs](https://www.privatehealth.gov.au/health_insurance/howitworks/out_of_pocket.htm)"));
        assert!(!md.contains("FAQ"));
        assert!(!md.contains("Privacy"));
    }
}
