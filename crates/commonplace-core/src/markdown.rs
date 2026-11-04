//! Lightweight markdown scanning: code stripping, wikilinks, footnotes,
//! headings, generated blocks. Regex-based on purpose: it keeps the binary
//! small and matches what agents actually write.

use regex::Regex;
use std::sync::LazyLock;

pub static WIKILINK_RE: LazyLock<Regex> =
    LazyLock::new(|| crate::re(r"\[\[([^\]\|#\^]+)(?:#([^\]\|]*))?(?:\|([^\]]*))?\]\]"));
pub static FOOTNOTE_DEF_RE: LazyLock<Regex> =
    LazyLock::new(|| crate::re(r"(?m)^\[\^([A-Za-z0-9_\-]+)\]:[ \t]*(.*)$"));
pub static FOOTNOTE_REF_RE: LazyLock<Regex> =
    LazyLock::new(|| crate::re(r"\[\^([A-Za-z0-9_\-]+)\]"));
pub static HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| crate::re(r"(?m)^(#{1,6})[ \t]+(.+?)[ \t]*#*[ \t]*$"));
static FENCE_RE: LazyLock<Regex> = LazyLock::new(|| crate::re(r"(?m)^[ \t]{0,3}(```|~~~)"));
static INLINE_CODE_RE: LazyLock<Regex> = LazyLock::new(|| crate::re(r"`[^`\n]*`"));

/// Replace fenced code blocks and inline code with spaces of equal length, so
/// byte offsets and line numbers are preserved while links inside code are ignored.
pub fn mask_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_fence: Option<&str> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let fence = FENCE_RE.find(line).map(|m| {
            if m.as_str().trim_start().starts_with("```") {
                "```"
            } else {
                "~~~"
            }
        });
        match (in_fence, fence) {
            (None, Some(f)) => {
                in_fence = Some(f);
                out.push_str(&blank(line));
            }
            (Some(f), Some(g))
                if f == g && trimmed.trim_end().chars().all(|c| c == '`' || c == '~') =>
            {
                in_fence = None;
                out.push_str(&blank(line));
            }
            (Some(_), _) => out.push_str(&blank(line)),
            (None, None) => {
                let masked =
                    INLINE_CODE_RE.replace_all(line, |m: &regex::Captures<'_>| blank(&m[0]));
                out.push_str(&masked);
            }
        }
    }
    out
}

/// Replace HTML comments with spaces (offsets preserved).
pub fn mask_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(s) = rest.find("<!--") {
        out.push_str(&rest[..s]);
        if let Some(e) = rest[s..].find("-->") {
            out.push_str(&blank(&rest[s..s + e + 3]));
            rest = &rest[s + e + 3..];
        } else {
            out.push_str(&blank(&rest[s..]));
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

fn blank(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\n' { '\n' } else { ' ' })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    pub target: String,
    pub heading: Option<String>,
    pub alias: Option<String>,
    pub line: usize, // 1-based within the scanned text
}

pub fn wikilinks(text: &str) -> Vec<WikiLink> {
    let masked = mask_comments(&mask_code(text));
    let mut out = vec![];
    for cap in WIKILINK_RE.captures_iter(&masked) {
        let m = cap.get_match();
        let line = masked[..m.start()].matches('\n').count() + 1;
        let target = cap[1].trim().to_string();
        if target.is_empty() {
            continue;
        }
        out.push(WikiLink {
            target,
            heading: cap
                .get(2)
                .map(|h| h.as_str().trim().to_string())
                .filter(|s| !s.is_empty()),
            alias: cap.get(3).map(|a| a.as_str().trim().to_string()),
            line,
        });
    }
    out
}

/// Raw link targets inside a string (e.g. a frontmatter `sources:` entry).
pub fn wikilink_targets_in(s: &str) -> Vec<String> {
    WIKILINK_RE
        .captures_iter(s)
        .map(|c| c[1].trim().to_string())
        .collect()
}

#[derive(Debug, Clone)]
pub struct FootnoteDef {
    pub label: String,
    pub text: String,
    pub line: usize,
}

pub fn footnote_defs(text: &str) -> Vec<FootnoteDef> {
    FOOTNOTE_DEF_RE
        .captures_iter(text)
        .map(|c| {
            let m = c.get_match();
            FootnoteDef {
                label: c[1].to_string(),
                text: c[2].trim().to_string(),
                line: text[..m.start()].matches('\n').count() + 1,
            }
        })
        .collect()
}

pub fn footnote_refs(text: &str) -> Vec<(String, usize)> {
    let masked = mask_code(text);
    // Remove definitions first so the def's own `[^x]` is not counted as a reference.
    let no_defs = FOOTNOTE_DEF_RE.replace_all(&masked, |m: &regex::Captures<'_>| blank(&m[0]));
    FOOTNOTE_REF_RE
        .captures_iter(&no_defs)
        .map(|c| {
            let m = c.get_match();
            (
                c[1].to_string(),
                no_defs[..m.start()].matches('\n').count() + 1,
            )
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct Heading {
    pub level: usize,
    pub text: String,
    pub line: usize,
}

pub fn headings(text: &str) -> Vec<Heading> {
    let masked = mask_code(text);
    HEADING_RE
        .captures_iter(&masked)
        .map(|c| {
            let m = c.get_match();
            Heading {
                level: c[1].len(),
                text: c[2].trim().to_string(),
                line: masked[..m.start()].matches('\n').count() + 1,
            }
        })
        .collect()
}

/// First H1 in the body, if any.
pub fn first_h1(text: &str) -> Option<String> {
    headings(text)
        .into_iter()
        .find(|h| h.level == 1)
        .map(|h| h.text)
}

/// Text under a given heading (until the next heading of same or higher level).
pub fn section<'a>(text: &'a str, heading: &str) -> Option<&'a str> {
    let hs = headings(text);
    let idx = hs
        .iter()
        .position(|h| h.text.eq_ignore_ascii_case(heading))?;
    let start_line = hs[idx].line; // 1-based
    let level = hs[idx].level;
    let end_line = hs[idx + 1..]
        .iter()
        .find(|h| h.level <= level)
        .map(|h| h.line);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let start_byte: usize = lines[..start_line].iter().map(|l| l.len()).sum();
    let end_byte: usize = match end_line {
        Some(e) => lines[..e - 1].iter().map(|l| l.len()).sum(),
        None => text.len(),
    };
    Some(&text[start_byte..end_byte])
}

/// Replace (or append) a generated block delimited by `<!-- commonplace:NAME:start -->`
/// and `<!-- commonplace:NAME:end -->` markers. Returns the new text.
/// Marker prefixes: current, then the pre-0.5.0 one (still recognised; rewritten on update).
const MARKER_PREFIXES: &[&str] = &["commonplace", "brain"];

fn find_block(text: &str, name: &str) -> Option<(usize, usize, usize, usize)> {
    for prefix in MARKER_PREFIXES {
        let start = format!("<!-- {prefix}:{name}:start -->");
        let end = format!("<!-- {prefix}:{name}:end -->");
        if let Some(s) = text.find(&start)
            && let Some(e) = text[s..].find(&end).map(|e| e + s)
        {
            return Some((s, s + start.len(), e, e + end.len()));
        }
    }
    None
}

pub fn replace_generated_block(text: &str, name: &str, content: &str) -> String {
    let start = format!("<!-- commonplace:{name}:start -->");
    let end = format!("<!-- commonplace:{name}:end -->");
    let block = format!("{start}\n{}\n{end}", content.trim_end());
    if let Some((s, _, _, e_end)) = find_block(text, name) {
        let mut out = String::new();
        out.push_str(&text[..s]);
        out.push_str(&block);
        out.push_str(&text[e_end..]);
        return out;
    }
    let mut out = text.trim_end().to_string();
    out.push_str("\n\n");
    out.push_str(&block);
    out.push('\n');
    out
}

/// The text between generated-block markers, if present.
pub fn generated_block<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let (_, s, e, _) = find_block(text, name)?;
    Some(&text[s..e])
}

/// Does the body contain prose (anything other than headings, blank lines, markers)?
pub fn has_prose(text: &str) -> bool {
    mask_comments(text).lines().any(|l| {
        let t = l.trim();
        !(t.is_empty() || t.starts_with('#') || t.starts_with("<!--"))
    })
}

/// Rough word count of the body (code masked).
pub fn word_count(text: &str) -> usize {
    mask_code(text).split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_ignore_code() {
        let t = "See [[A Page|alias]] and `[[not]]`\n```\n[[nope]]\n```\n[[B#Head]]";
        let l = wikilinks(t);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].target, "A Page");
        assert_eq!(l[0].alias.as_deref(), Some("alias"));
        assert_eq!(l[1].heading.as_deref(), Some("Head"));
        assert_eq!(l[1].line, 5);
    }

    #[test]
    fn footnotes() {
        let t = "Claim.[^1] Other.[^c2]\n\n[^1]: [[S]] p.3 \"q\"\n[^c2]: [[T]] L1-2 \"r\"\n";
        let d = footnote_defs(t);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].label, "1");
        let r = footnote_refs(t);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn gen_block() {
        let legacy = "# T\n\n<!-- brain:pages:start -->\nold\n<!-- brain:pages:end -->\n";
        assert_eq!(generated_block(legacy, "pages").unwrap().trim(), "old");
        let up = replace_generated_block(legacy, "pages", "new");
        assert!(
            up.contains("<!-- commonplace:pages:start -->\nnew\n<!-- commonplace:pages:end -->")
                && !up.contains("brain:")
        );
        let t = "# T\n\nintro\n\n<!-- commonplace:pages:start -->\nold\n<!-- commonplace:pages:end -->\n\ntail\n";
        let n = replace_generated_block(t, "pages", "new");
        assert!(
            n.contains("<!-- commonplace:pages:start -->\nnew\n<!-- commonplace:pages:end -->")
        );
        assert!(n.ends_with("tail\n"));
        let a = replace_generated_block("# T\n", "pages", "x");
        assert!(
            a.ends_with("<!-- commonplace:pages:start -->\nx\n<!-- commonplace:pages:end -->\n")
        );
    }

    #[test]
    fn sections() {
        let t = "# T\n\n## A\none\n\n## B\ntwo\n";
        assert_eq!(section(t, "A").unwrap().trim(), "one");
        assert_eq!(section(t, "B").unwrap().trim(), "two");
    }
}
