//! The evidence chain: immutable raw original -> faithful text layer with page
//! and section anchors -> source note. Ingest, convert, verify, and read.

use crate::config::Config;
use crate::frontmatter;
use crate::fsutil;
use crate::manifest::{Manifest, SourceRecord};
use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub const RAW_DIR: &str = "sources/raw";
pub const TEXT_DIR: &str = "sources/text";
pub const NOTES_DIR: &str = "sources/notes";

#[derive(Debug, Clone, Serialize)]
pub struct IngestReport {
    pub id: String,
    pub raw: String,
    pub raw_sha256: String,
    pub text: Option<String>,
    pub note: String,
    pub converter: String,
    pub fidelity: String,
    pub pages: Option<usize>,
    pub needs_conversion: bool,
    pub duplicate_of: Option<String>,
    pub warnings: Vec<String>,
}

pub struct IngestOptions {
    pub title: Option<String>,
    pub date: Option<String>,
    pub kind: Option<String>,
    pub origin: Option<String>,
    pub project: Option<String>,
    pub confidentiality: Option<String>,
    pub authority: Option<String>,
    pub converter: Option<String>,
    /// Keep the original in place and only copy (default) vs move.
    pub move_file: bool,
    pub force: bool,
}

/// Build a source id `YYYY-MM-DD Short Title` from a title and date.
pub fn make_id(title: &str, date: &str) -> String {
    let mut t = fsutil::safe_title(title);
    if t.chars().count() > 80 {
        t = t
            .chars()
            .take(80)
            .collect::<String>()
            .trim_end()
            .to_string();
    }
    format!("{} {}", date, t)
}

static DATE_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{4}-\d{2}-\d{2})\s+(.+)$").unwrap());

pub fn ingest(
    root: &Path,
    config: &Config,
    src: &Path,
    opts: &IngestOptions,
) -> Result<IngestReport> {
    if !src.is_file() {
        bail!("{} is not a file", src.display());
    }
    let mut warnings = vec![];
    let sha = fsutil::sha256_file(src)?;
    let mut manifest = Manifest::load(root)?;
    if let Some((id, rec)) = manifest.find_by_hash(&sha) {
        if !opts.force {
            return Ok(IngestReport {
                id: id.clone(),
                raw: rec.raw.clone(),
                raw_sha256: sha,
                text: rec.text.clone(),
                note: rec.note.clone().unwrap_or_default(),
                converter: rec.converter.clone().unwrap_or_default(),
                fidelity: String::new(),
                pages: None,
                needs_conversion: false,
                duplicate_of: Some(id.clone()),
                warnings: vec![
                    "identical file already ingested; nothing done (use --force to ingest again)"
                        .into(),
                ],
            });
        }
    }
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let file_stem = fsutil::stem(src);
    let (title, date) = {
        let t = opts
            .title
            .clone()
            .unwrap_or_else(|| file_stem.replace(['_', '-'], " "));
        let d = opts.date.clone();
        match DATE_PREFIX_RE.captures(&t) {
            Some(c) if d.is_none() => (c[2].to_string(), c[1].to_string()),
            _ => (t, d.unwrap_or_else(fsutil::today)),
        }
    };
    let mut id = make_id(&title, &date);
    let raw_dir = root.join(RAW_DIR);
    let notes_dir = root.join(NOTES_DIR);
    let text_dir = root.join(TEXT_DIR);
    std::fs::create_dir_all(&raw_dir)?;
    std::fs::create_dir_all(&notes_dir)?;
    std::fs::create_dir_all(&text_dir)?;
    // avoid id collisions (same title+date, different content)
    if manifest.sources.contains_key(&id) || notes_dir.join(format!("{id}.md")).exists() {
        let mut n = 2;
        while manifest.sources.contains_key(&format!("{id} ({n})"))
            || notes_dir.join(format!("{id} ({n}).md")).exists()
        {
            n += 1;
        }
        warnings.push(format!("a source named '{id}' already exists; using '{id} ({n})'. If this is the same document in a new version, consider `supersedes`."));
        id = format!("{id} ({n})");
    }
    let raw_name = if ext.is_empty() {
        id.clone()
    } else {
        format!("{id}.{ext}")
    };
    let raw_path = raw_dir.join(&raw_name);
    if opts.move_file {
        std::fs::rename(src, &raw_path).or_else(|_| {
            std::fs::copy(src, &raw_path)
                .map(|_| ())
                .and_then(|_| std::fs::remove_file(src))
        })?;
    } else {
        std::fs::copy(src, &raw_path)
            .with_context(|| format!("copying to {}", raw_path.display()))?;
    }
    set_readonly(&raw_path);

    // Convert
    let kind = opts.kind.clone().unwrap_or_else(|| guess_kind(&ext));
    let conv = convert(
        &raw_path,
        &ext,
        config,
        opts.converter.as_deref(),
        opts.origin.as_deref(),
    )?;
    let text_rel = format!("{TEXT_DIR}/{id}.text.md");
    let mut text_sha = None;
    let mut pages = None;
    if let Some(c) = &conv.markdown {
        pages = c.pages;
        let mut fm = serde_yaml::Mapping::new();
        frontmatter::set_str(&mut fm, "type", "source-text");
        frontmatter::set_str(&mut fm, "source", &id);
        frontmatter::set_str(&mut fm, "raw", &format!("{RAW_DIR}/{raw_name}"));
        frontmatter::set_str(&mut fm, "raw_sha256", &sha);
        frontmatter::set_str(&mut fm, "converter", &conv.converter);
        frontmatter::set_str(&mut fm, "fidelity", &conv.fidelity);
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
        text_sha = Some(fsutil::sha256_str(&text));
    }
    let needs_conversion = conv.markdown.is_none();
    if needs_conversion {
        warnings.push(format!("no converter could handle .{ext}: {}. Write the text layer yourself at {text_rel} (see references/ingest.md) and run `commonplace source attach-text \"{id}\"`.", conv.reason));
    }

    // Source note
    let note_rel = format!("{NOTES_DIR}/{id}.md");
    let mut fm = serde_yaml::Mapping::new();
    frontmatter::set_str(&mut fm, "type", "source");
    frontmatter::set_str(&mut fm, "title", &title);
    frontmatter::set_str(&mut fm, "kind", &kind);
    frontmatter::set_str(&mut fm, "raw", &format!("{RAW_DIR}/{raw_name}"));
    frontmatter::set_str(
        &mut fm,
        "text",
        if needs_conversion { "" } else { &text_rel },
    );
    frontmatter::set_str(&mut fm, "origin", opts.origin.as_deref().unwrap_or(""));
    frontmatter::set_str(&mut fm, "retrieved", &fsutil::today());
    frontmatter::set_str(&mut fm, "published", &date);
    frontmatter::set_str(&mut fm, "author", "");
    frontmatter::set_str(&mut fm, "publisher", "");
    frontmatter::set_str(
        &mut fm,
        "authority",
        opts.authority.as_deref().unwrap_or("primary"),
    );
    frontmatter::set_str(
        &mut fm,
        "confidentiality",
        opts.confidentiality.as_deref().unwrap_or("personal"),
    );
    frontmatter::set_str(&mut fm, "fidelity", &conv.fidelity);
    frontmatter::set_str(&mut fm, "converter", &conv.converter);
    frontmatter::set_str(
        &mut fm,
        "status",
        if needs_conversion {
            "registered"
        } else {
            "converted"
        },
    );
    frontmatter::set_value(
        &mut fm,
        "summary_level",
        serde_yaml::Value::Number(0.into()),
    );
    frontmatter::set_value(&mut fm, "tags", serde_yaml::Value::Sequence(vec![]));
    frontmatter::set_value(
        &mut fm,
        "projects",
        serde_yaml::Value::Sequence(
            opts.project
                .iter()
                .map(|p| serde_yaml::Value::String(format!("[[{p}]]")))
                .collect(),
        ),
    );
    frontmatter::set_str(&mut fm, "description", "");
    frontmatter::set_str(&mut fm, "created", &fsutil::today());
    frontmatter::set_str(&mut fm, "updated", &fsutil::today());
    let body = format!(
        "# {title}\n\n## TL;DR\n\n<!-- one or two sentences: what this is and why it matters (L1) -->\n\n## Summary\n\n<!-- up to ~300 words in your own words (L2) -->\n\n## Key quotes\n\n<!-- verbatim, anchored: > \"…\" — p.12 / para 26 / §3.2 (L3). These feed citations. -->\n\n## Structure\n\n{}\n## Relevance and open questions\n\n## Related\n",
        conv.outline.clone().unwrap_or_default()
    );
    let note_text = frontmatter::render(&fm, &body)?;
    fsutil::write_atomic(&root.join(&note_rel), note_text.as_bytes())?;

    manifest.sources.insert(
        id.clone(),
        SourceRecord {
            raw: format!("{RAW_DIR}/{raw_name}"),
            raw_sha256: sha.clone(),
            text: if needs_conversion {
                None
            } else {
                Some(text_rel.clone())
            },
            text_sha256: text_sha,
            note: Some(note_rel.clone()),
            ingested: fsutil::now_iso(),
            converter: Some(conv.converter.clone()),
            origin: opts.origin.clone(),
        },
    );
    manifest.save(root)?;
    if let Some(w) = conv.warning {
        warnings.push(w);
    }
    Ok(IngestReport {
        id,
        raw: format!("{RAW_DIR}/{raw_name}"),
        raw_sha256: sha,
        text: if needs_conversion {
            None
        } else {
            Some(text_rel)
        },
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
        let _ = std::fs::set_permissions(p, perms);
    }
}

pub fn guess_kind(ext: &str) -> String {
    match ext {
        "pdf" => "document",
        "docx" | "doc" | "odt" | "rtf" => "document",
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
pub struct Converted {
    pub text: String,
    pub pages: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ConvertResult {
    pub markdown: Option<Converted>,
    pub converter: String,
    pub fidelity: String,
    pub reason: String,
    pub warning: Option<String>,
    pub outline: Option<String>,
}

/// Convert a raw file into markdown. Built in: md, txt, html. External (if
/// installed, in the configured preference order): pdftotext for PDF; pandoc
/// for docx/odt/rtf/epub; markitdown for office formats. Anything else is left
/// for the agent (`needs_conversion`).
pub fn convert(
    raw: &Path,
    ext: &str,
    config: &Config,
    forced: Option<&str>,
    origin: Option<&str>,
) -> Result<ConvertResult> {
    let none = |reason: &str| ConvertResult {
        markdown: None,
        converter: "none".into(),
        fidelity: "none".into(),
        reason: reason.into(),
        warning: None,
        outline: None,
    };
    match ext {
        "md" | "markdown" | "txt" | "text" => {
            let t = std::fs::read_to_string(raw)?;
            let t = strip_frontmatter(&t);
            let outline = outline_from_markdown(&t);
            return Ok(ConvertResult {
                markdown: Some(Converted {
                    text: t,
                    pages: None,
                }),
                converter: "copy".into(),
                fidelity: "high".into(),
                reason: String::new(),
                warning: None,
                outline,
            });
        }
        "html" | "htm" => {
            let h = std::fs::read_to_string(raw)?;
            let md = html_to_markdown(&h, origin)?;
            let outline = outline_from_markdown(&md);
            return Ok(ConvertResult {
                markdown: Some(Converted {
                    text: md,
                    pages: None,
                }),
                converter: "htmd".into(),
                fidelity: "high".into(),
                reason: String::new(),
                warning: None,
                outline,
            });
        }
        _ => {}
    }
    let available = available_converters();
    let order: Vec<String> = match forced {
        Some(f) => vec![f.to_string()],
        None => config.converters.prefer.clone(),
    };
    for name in order {
        let present = available.iter().any(|a| a == &name);
        if !present {
            continue;
        }
        match (name.as_str(), ext) {
            ("pdftotext", "pdf") => {
                let out = std::process::Command::new("pdftotext")
                    .args(["-layout", "-enc", "UTF-8"])
                    .arg(raw)
                    .arg("-")
                    .output()?;
                if !out.status.success() {
                    continue;
                }
                let txt = String::from_utf8_lossy(&out.stdout).to_string();
                let (text, pages) = page_markers_from_formfeeds(&txt);
                let chars_per_page = if pages > 0 {
                    text.len() / pages
                } else {
                    text.len()
                };
                let fidelity = if chars_per_page < 200 {
                    "low"
                } else {
                    "medium"
                };
                let warning = if chars_per_page < 200 {
                    Some("very little text per page: probably a scanned PDF that needs OCR (docling/marker) or manual conversion".into())
                } else {
                    None
                };
                return Ok(ConvertResult {
                    markdown: Some(Converted {
                        text,
                        pages: Some(pages),
                    }),
                    converter: "pdftotext".into(),
                    fidelity: fidelity.into(),
                    reason: String::new(),
                    warning,
                    outline: None,
                });
            }
            ("pandoc", "docx" | "odt" | "rtf" | "epub" | "html") => {
                let out = std::process::Command::new("pandoc")
                    .arg(raw)
                    .args(["-t", "gfm", "--wrap=none"])
                    .output()?;
                if !out.status.success() {
                    continue;
                }
                let md = String::from_utf8_lossy(&out.stdout).to_string();
                let outline = outline_from_markdown(&md);
                return Ok(ConvertResult {
                    markdown: Some(Converted {
                        text: md,
                        pages: None,
                    }),
                    converter: "pandoc".into(),
                    fidelity: "high".into(),
                    reason: String::new(),
                    warning: None,
                    outline,
                });
            }
            ("markitdown", "docx" | "pptx" | "xlsx" | "pdf") => {
                let out = std::process::Command::new("markitdown").arg(raw).output()?;
                if !out.status.success() {
                    continue;
                }
                let md = String::from_utf8_lossy(&out.stdout).to_string();
                let outline = outline_from_markdown(&md);
                let fid = if ext == "pdf" { "medium" } else { "high" };
                return Ok(ConvertResult {
                    markdown: Some(Converted {
                        text: md,
                        pages: None,
                    }),
                    converter: "markitdown".into(),
                    fidelity: fid.into(),
                    reason: String::new(),
                    warning: if ext == "pdf" {
                        Some("markitdown drops page boundaries; add <!-- page:N --> markers or re-convert with pdftotext for page-level citations".into())
                    } else {
                        None
                    },
                    outline,
                });
            }
            _ => continue,
        }
    }
    Ok(none(&format!(
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
pub fn html_to_markdown(html: &str, origin: Option<&str>) -> Result<String> {
    let conv = htmd::HtmlToMarkdown::builder()
        .skip_tags(HTML_SKIP_TAGS.to_vec())
        .build();
    let md = conv
        .convert(html)
        .map_err(|e| anyhow!("html conversion failed: {e}"))?;
    Ok(match origin {
        Some(o) if o.contains("://") => absolutise_links(&md, o, |_| false).0,
        _ => md,
    })
}

static MD_LINK_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\]\(<?([^)\s<>]+)>?").unwrap());

/// Rewrite relative markdown link and image targets to absolute URLs against
/// `base`. `keep(href)` returning true leaves that target alone (used to
/// protect links that point at real vault files). Returns the text and the
/// number of targets rewritten.
pub fn absolutise_links(md: &str, base: &str, keep: impl Fn(&str) -> bool) -> (String, usize) {
    let mut n = 0usize;
    let out = MD_LINK_URL_RE.replace_all(md, |c: &regex::Captures| {
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
pub fn resolve_url(base: &str, href: &str) -> Option<String> {
    static SCHEME_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*:").unwrap());
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

#[derive(Debug, Clone, Serialize)]
pub struct RelinkReport {
    pub id: String,
    pub origin: Option<String>,
    pub files_changed: Vec<String>,
    pub links_rewritten: usize,
    pub skipped: Option<String>,
}

/// Rewrite relative web links in an existing source's text layer and note to
/// absolute URLs against the recorded origin, then re-hash the text layer.
/// Repairs sources ingested before the converter did this itself. Line
/// numbers do not move, so `L` locators and quotes stay valid.
pub fn relink(root: &Path, id: &str) -> Result<RelinkReport> {
    let mut manifest = Manifest::load(root)?;
    let rec = manifest
        .sources
        .get_mut(id)
        .ok_or_else(|| anyhow!("unknown source id '{id}'"))?;
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
        let dir = f.parent().map(|d| d.to_path_buf()).unwrap_or_default();
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
pub fn page_markers_from_formfeeds(txt: &str) -> (String, usize) {
    let mut out = String::with_capacity(txt.len() + 64);
    let mut n = 0;
    for (i, page) in txt.split('\u{c}').enumerate() {
        if i == txt.split('\u{c}').count() - 1 && page.trim().is_empty() {
            break;
        }
        n = i + 1;
        out.push_str(&format!("<!-- page:{} -->\n", n));
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
        s.push_str(&format!(
            "{}- {}\n",
            "  ".repeat(h.level.saturating_sub(1)),
            h.text
        ));
    }
    Some(s)
}

/// Locate the text layer for a source id: a single `<id>.text.md` file or a
/// directory `<id>/` of section files (sorted by name). Returns the virtual
/// concatenated text plus a map from virtual line to (file, line).
pub struct TextLayer {
    pub text: String,
    pub parts: Vec<TextPart>,
}

pub struct TextPart {
    pub rel: String,
    pub start_line: usize, // 1-based virtual line where this part's body starts
    pub lines: usize,
}

impl TextLayer {
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

pub fn text_layer_path(root: &Path, id: &str) -> Option<PathBuf> {
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
            header.push_str(&format!("<!-- section:{s} -->\n"));
        }
        if let Some(pf) = frontmatter::get_str(&doc.fm, "page_from")
            .or_else(|| frontmatter::get_str(&doc.fm, "pages_from"))
        {
            if !doc.body.contains("<!-- page:") {
                header.push_str(&format!("<!-- page:{pf} -->\n"));
            }
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

#[derive(Debug, Clone, Serialize)]
pub struct VerifyEntry {
    pub id: String,
    pub raw_ok: Option<bool>,
    pub text_ok: Option<bool>,
    pub message: String,
}

/// Re-hash every registered raw original and text layer.
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
pub fn attach_text(root: &Path, id: &str, converter: &str, fidelity: &str) -> Result<String> {
    let mut manifest = Manifest::load(root)?;
    let rec = manifest
        .sources
        .get_mut(id)
        .ok_or_else(|| anyhow!("unknown source id '{id}'"))?;
    let p = text_layer_path(root, id).ok_or_else(|| {
        anyhow!("no text layer found at {TEXT_DIR}/{id}.text.md or {TEXT_DIR}/{id}/")
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
pub fn known_source_ids(root: &Path) -> Vec<String> {
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
