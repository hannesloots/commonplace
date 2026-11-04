//! Citation parsing and deterministic verification.
//!
//! Syntax (a markdown footnote definition):
//! `[^label]: [[source-id]] <locator> "verbatim quote" — optional comment`
//!
//! The quote must appear, after normalisation, inside the span of the source's
//! text layer that the locator names. Locators: `p.12` / `pp.12-14` (page
//! markers `<!-- page:12 -->`), `para 26` / `¶26` / `[26]` (paragraph markers
//! `<!-- para:26 -->` or a line starting with `[26]`), `L120` / `L120-128`
//! (lines of the text layer), `§3.2` (heading starting with 3.2), `t.3`
//! (table 3 is treated as a search over the whole file), `whole` or no locator.

use crate::markdown;
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

static DEF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\s*\[\[([^\]\|#]+)(?:\|[^\]]*)?\]\]\s*([^"“”]*?)\s*(?:["“](.*?)["”])?\s*(?:[—\-]+\s*(.*))?$"#).unwrap()
});
static PAGE_MARK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!--\s*page:\s*(\d+)\s*-->").unwrap());
static PARA_MARK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:<!--\s*para:\s*(\d+)\s*-->|\[(\d+)\]\s)").unwrap());

#[derive(Debug, Clone, Serialize)]
pub struct Citation {
    pub label: String,
    pub source: String,
    pub locator: Locator,
    pub locator_raw: String,
    pub quote: Option<String>,
    pub comment: Option<String>,
    pub line: usize,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Locator {
    Page {
        from: u32,
        to: u32,
    },
    Para {
        from: u32,
        to: u32,
    },
    Line {
        from: usize,
        to: usize,
    },
    Section {
        id: String,
    },
    Whole,
    /// Something we cannot map to a span (timestamps, free text): verify over the whole file.
    Free {
        text: String,
    },
}

pub fn parse_locator(s: &str) -> Locator {
    let t = s.trim().trim_end_matches(',').trim();
    if t.is_empty() || t.eq_ignore_ascii_case("whole") {
        return Locator::Whole;
    }
    static L_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?:L|lines?\s*)(\d+)(?:\s*[-–]\s*L?(\d+))?$").unwrap());
    static P_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:pp?\.?\s*|pages?\s+)(\d+)(?:\s*[-–]\s*(\d+))?$").unwrap()
    });
    static PARA_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:¶|para(?:graph)?s?\.?\s*|at\s+paras?\s*|\[)\s*(\d+)\]?(?:\s*[-–]\s*\[?(\d+)\]?)?$").unwrap()
    });
    static SEC_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:§|sec(?:tion)?\.?\s*|s\.?\s+)\s*([0-9A-Za-z][0-9A-Za-z.\-()]*)$")
            .unwrap()
    });
    if let Some(c) = L_RE.captures(t) {
        let a: usize = c[1].parse().unwrap_or(1);
        let b: usize = c
            .get(2)
            .map(|m| m.as_str().parse().unwrap_or(a))
            .unwrap_or(a);
        return Locator::Line {
            from: a.min(b),
            to: a.max(b),
        };
    }
    if let Some(c) = P_RE.captures(t) {
        let a: u32 = c[1].parse().unwrap_or(1);
        let b: u32 = c
            .get(2)
            .map(|m| m.as_str().parse().unwrap_or(a))
            .unwrap_or(a);
        return Locator::Page {
            from: a.min(b),
            to: a.max(b),
        };
    }
    if let Some(c) = PARA_RE.captures(t) {
        let a: u32 = c[1].parse().unwrap_or(1);
        let b: u32 = c
            .get(2)
            .map(|m| m.as_str().parse().unwrap_or(a))
            .unwrap_or(a);
        return Locator::Para {
            from: a.min(b),
            to: a.max(b),
        };
    }
    if let Some(c) = SEC_RE.captures(t) {
        return Locator::Section {
            id: c[1].to_string(),
        };
    }
    Locator::Free {
        text: t.to_string(),
    }
}

/// Parse all citation footnotes in a page body. Non-citation footnotes (no
/// leading wikilink) are returned in the second list with their labels.
pub fn parse_citations(body: &str) -> (Vec<Citation>, Vec<markdown::FootnoteDef>) {
    let mut cites = vec![];
    let mut plain = vec![];
    for def in markdown::footnote_defs(body) {
        match DEF_RE.captures(&def.text) {
            Some(c) if def.text.trim_start().starts_with("[[") => {
                let locator_raw = c
                    .get(2)
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or_default();
                cites.push(Citation {
                    label: def.label.clone(),
                    source: c[1].trim().to_string(),
                    locator: parse_locator(&locator_raw),
                    locator_raw,
                    quote: c
                        .get(3)
                        .map(|m| m.as_str().trim().to_string())
                        .filter(|s| !s.is_empty()),
                    comment: c
                        .get(4)
                        .map(|m| m.as_str().trim().to_string())
                        .filter(|s| !s.is_empty()),
                    line: def.line,
                    raw: def.text.clone(),
                });
            }
            _ => plain.push(def),
        }
    }
    (cites, plain)
}

/// Normalise text for quote matching: NFKC, lowercase, straight quotes,
/// plain hyphens, no soft hyphens, single spaces.
pub fn normalise(s: &str) -> String {
    // Markdown links and images compare by their visible text, so a URL that
    // changes (or was copied into a quote) can never break a match.
    // The link's title text (hover text) is kept too: it is visible content.
    static MD_LINK_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"!?\[([^\]]*)\]\(\s*[^)\s]*(?:\s+"([^"]*)")?\s*\)"#).unwrap()
    });
    let s = MD_LINK_RE.replace_all(s, "$1 $2");
    let mut out = String::with_capacity(s.len());
    for c in s.nfkc() {
        let c = match c {
            '\u{AD}' => continue,
            '‘' | '’' | '‚' | '‛' | '`' | '´' => '\'',
            '“' | '”' | '„' | '‟' | '«' | '»' => '"',
            '–' | '—' | '−' | '‐' | '‑' | '‒' | '―' => '-',
            '\u{A0}' | '\u{2007}' | '\u{202F}' => ' ',
            c => c,
        };
        if !(c.is_alphanumeric() || c.is_whitespace() || c == '-' || c == '\'') {
            out.push(' ');
            continue;
        }
        for lc in c.to_lowercase() {
            out.push(lc);
        }
    }
    out.split_whitespace()
        .map(|w| w.trim_matches('\''))
        .map(|w| w.trim_start_matches('-'))
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A span of the text layer with line numbers, flattened to normalised words.
pub struct Haystack {
    words: Vec<String>,
    lines: Vec<usize>, // original 1-based line number of each word
    joined: String,
    word_starts: Vec<usize>, // byte offset of each word in `joined`
}

impl Haystack {
    pub fn new(lines: &[(usize, &str)]) -> Self {
        let mut words: Vec<String> = vec![];
        let mut line_of: Vec<usize> = vec![];
        for (ln, text) in lines {
            let norm = normalise(text);
            let mut iter = norm.split(' ').filter(|w| !w.is_empty()).peekable();
            let mut first = true;
            while let Some(w) = iter.next() {
                // join end-of-line hyphenation: "inter-" + "national"
                if first {
                    if let Some(last) = words.last_mut() {
                        if last.ends_with('-')
                            && last.len() > 1
                            && w.chars().next().map(|c| c.is_lowercase()).unwrap_or(false)
                        {
                            last.pop();
                            last.push_str(w);
                            first = false;
                            continue;
                        }
                    }
                }
                first = false;
                words.push(w.to_string());
                line_of.push(*ln);
            }
        }
        let mut joined = String::new();
        let mut starts = vec![];
        for w in &words {
            starts.push(joined.len());
            joined.push_str(w);
            joined.push(' ');
        }
        Haystack {
            words,
            lines: line_of,
            joined,
            word_starts: starts,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Find a needle (already normalised words) as a whole-word subsequence.
    /// Returns the line number of its first word.
    fn find_from(&self, needle: &str, from_word: usize) -> Option<(usize, usize)> {
        if needle.is_empty() {
            return None;
        }
        let pat = format!("{needle} ");
        let start_byte = self
            .word_starts
            .get(from_word)
            .copied()
            .unwrap_or(self.joined.len());
        let mut search_from = start_byte;
        while let Some(pos) = self.joined[search_from..].find(&pat) {
            let abs = search_from + pos;
            // whole-word alignment: must start at a word boundary
            let at_boundary = abs == 0 || self.joined.as_bytes()[abs - 1] == b' ';
            if at_boundary {
                let widx = self.word_starts.binary_search(&abs).unwrap_or_else(|i| i);
                let nwords = needle.split(' ').count();
                return Some((widx, widx + nwords));
            }
            search_from = abs + 1;
        }
        None
    }

    /// Verify a quote; supports `...` / `…` / `[...]` ellipses between segments.
    pub fn verify(&self, quote: &str) -> Option<usize> {
        let norm = normalise(quote)
            .replace("[...]", "...")
            .replace("[…]", "...")
            .replace('…', "...");
        let segments: Vec<&str> = norm
            .split("...")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        if segments.is_empty() {
            return None;
        }
        let mut from = 0usize;
        let mut first_line = None;
        for seg in segments {
            let seg = seg.trim_matches(|c: char| {
                c == '"' || c == '\'' || c == ',' || c == '.' || c == ';' || c == ':'
            });
            if seg.is_empty() {
                continue;
            }
            let (s, e) = self.find_from(seg, from)?;
            if first_line.is_none() {
                first_line = Some(self.lines[s]);
            }
            from = e;
        }
        first_line
    }

    /// Lines containing the rarest words of the quote, for diagnostics.
    pub fn nearest_lines(&self, quote: &str, n: usize) -> Vec<usize> {
        let norm = normalise(quote);
        let mut qwords: Vec<&str> = norm.split(' ').filter(|w| w.len() > 3).collect();
        if qwords.is_empty() {
            return vec![];
        }
        // rank by rarity in haystack
        let mut freq: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for w in &self.words {
            *freq.entry(w.as_str()).or_default() += 1;
        }
        qwords.sort_by_key(|w| freq.get(w).copied().unwrap_or(0));
        let key: Vec<&str> = qwords
            .into_iter()
            .filter(|w| freq.get(w).copied().unwrap_or(0) > 0)
            .take(3)
            .collect();
        if key.is_empty() {
            return vec![];
        }
        let mut hits: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        for (i, w) in self.words.iter().enumerate() {
            if key.contains(&w.as_str()) {
                *hits.entry(self.lines[i]).or_default() += 1;
            }
        }
        let mut v: Vec<(usize, usize)> = hits.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.into_iter().take(n).map(|(l, _)| l).collect()
    }
}

/// The span of the text layer a locator names, as (line number, line text) pairs.
/// Returns None if the locator cannot be mapped (page marker absent, etc.).
pub fn span<'a>(text: &'a str, loc: &Locator) -> Option<Vec<(usize, &'a str)>> {
    let lines: Vec<(usize, &str)> = text.lines().enumerate().map(|(i, l)| (i + 1, l)).collect();
    match loc {
        Locator::Whole | Locator::Free { .. } => Some(lines),
        Locator::Line { from, to } => {
            if *from > lines.len() {
                return None;
            }
            Some(lines[from - 1..(*to).min(lines.len())].to_vec())
        }
        Locator::Page { from, to } => {
            // page N spans from its marker to the next page marker
            let mut cur: Option<u32> = None;
            let mut out = vec![];
            let mut seen = false;
            for (ln, l) in &lines {
                if let Some(c) = PAGE_MARK_RE.captures(l) {
                    cur = c[1].parse().ok();
                    if cur.map(|p| p >= *from && p <= *to).unwrap_or(false) {
                        seen = true;
                    }
                    continue;
                }
                if cur.map(|p| p >= *from && p <= *to).unwrap_or(false) {
                    out.push((*ln, *l));
                }
            }
            if seen { Some(out) } else { None }
        }
        Locator::Para { from, to } => {
            let mut cur: Option<u32> = None;
            let mut out = vec![];
            let mut seen = false;
            for (ln, l) in &lines {
                if let Some(c) = PARA_MARK_RE.captures(l) {
                    cur = c.get(1).or(c.get(2)).and_then(|m| m.as_str().parse().ok());
                    if cur.map(|p| p >= *from && p <= *to).unwrap_or(false) {
                        seen = true;
                    }
                }
                if cur.map(|p| p >= *from && p <= *to).unwrap_or(false) {
                    out.push((*ln, *l));
                }
            }
            if seen { Some(out) } else { None }
        }
        Locator::Section { id } => {
            let hs = markdown::headings(text);
            let idx = hs.iter().position(|h| {
                let t = h.text.trim_start_matches(['§', ' ']);
                t.starts_with(id.as_str())
                    && t[id.len()..]
                        .chars()
                        .next()
                        .map(|c| !c.is_ascii_digit())
                        .unwrap_or(true)
            })?;
            let level = hs[idx].level;
            let start = hs[idx].line;
            let end = hs[idx + 1..]
                .iter()
                .find(|h| h.level <= level)
                .map(|h| h.line - 1)
                .unwrap_or(lines.len());
            Some(lines[start - 1..end].to_vec())
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    QuoteNotFound,
    QuoteNotInSpan,
    SpanNotFound,
    NoQuote,
    SourceMissing,
    TextLayerMissing,
    QuoteTooShort,
    QuoteTooLong,
}

#[derive(Debug, Clone, Serialize)]
pub struct CiteResult {
    pub page: String,
    pub line: usize,
    pub label: String,
    pub source: String,
    pub locator: String,
    pub verdict: Verdict,
    pub found_line: Option<usize>,
    pub nearest_lines: Vec<usize>,
    pub message: String,
}

/// Verify one citation against the text layer of its source.
pub fn verify_quote(
    text_layer: &str,
    cit: &Citation,
    min_words: usize,
    max_words: usize,
) -> (Verdict, Option<usize>, Vec<usize>, String) {
    let Some(q) = &cit.quote else {
        return (
            Verdict::NoQuote,
            None,
            vec![],
            "citation has no verbatim quote".into(),
        );
    };
    let nwords = normalise(q).split(' ').filter(|w| !w.is_empty()).count();
    if nwords < min_words {
        return (
            Verdict::QuoteTooShort,
            None,
            vec![],
            format!("quote has {nwords} words; minimum is {min_words}"),
        );
    }
    if nwords > max_words {
        return (
            Verdict::QuoteTooLong,
            None,
            vec![],
            format!("quote has {nwords} words; maximum is {max_words}"),
        );
    }
    let whole = Haystack::new(&span(text_layer, &Locator::Whole).unwrap_or_default());
    match span(text_layer, &cit.locator) {
        None => {
            let near = whole.nearest_lines(q, 3);
            let in_whole = whole.verify(q);
            let msg = match in_whole {
                Some(l) => format!(
                    "locator '{}' not found in text layer, but the quote exists at L{l}",
                    cit.locator_raw
                ),
                None => format!(
                    "locator '{}' not found in text layer and quote not found anywhere",
                    cit.locator_raw
                ),
            };
            (Verdict::SpanNotFound, in_whole, near, msg)
        }
        Some(sp) => {
            let hay = Haystack::new(&sp);
            if let Some(l) = hay.verify(q) {
                return (Verdict::Pass, Some(l), vec![], format!("ok (L{l})"));
            }
            match whole.verify(q) {
                Some(l) => (
                    Verdict::QuoteNotInSpan,
                    Some(l),
                    vec![l],
                    format!(
                        "quote found at L{l}, which is outside the cited span '{}'",
                        cit.locator_raw
                    ),
                ),
                None => {
                    let near = whole.nearest_lines(q, 3);
                    let hint = if near.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "; closest lines: {}",
                            near.iter()
                                .map(|l| format!("L{l}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    (
                        Verdict::QuoteNotFound,
                        None,
                        near,
                        format!("quote not found in source text{hint}"),
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_strips_link_markup() {
        assert_eq!(
            normalise("The [rebate](/x/y) is tested."),
            "the rebate is tested"
        );
        assert_eq!(
            normalise("The [rebate](https://ato.gov.au/x) is tested."),
            normalise("The rebate is tested.")
        );
        assert_eq!(normalise("see ![logo](/a.svg) here"), "see logo here");
        assert_eq!(
            normalise(r#"* [Age-based Discount](/x.htm "may be available to people") now"#),
            "age-based discount may be available to people now"
        );
    }

    #[test]
    fn parse_def() {
        let body = "x[^1]\n\n[^1]: [[2025-01-08 Mavundla v MEC]] para 26 “Counsel lazily omitted to verify” — overreach?\n[^2]: plain note\n";
        let (c, p) = parse_citations(body);
        assert_eq!(c.len(), 1);
        assert_eq!(p.len(), 1);
        assert_eq!(c[0].source, "2025-01-08 Mavundla v MEC");
        assert_eq!(c[0].locator, Locator::Para { from: 26, to: 26 });
        assert_eq!(
            c[0].quote.as_deref(),
            Some("Counsel lazily omitted to verify")
        );
        assert_eq!(c[0].comment.as_deref(), Some("overreach?"));
    }

    #[test]
    fn locators() {
        assert_eq!(parse_locator("p.12"), Locator::Page { from: 12, to: 12 });
        assert_eq!(
            parse_locator("pp. 12-14"),
            Locator::Page { from: 12, to: 14 }
        );
        assert_eq!(parse_locator("¶26"), Locator::Para { from: 26, to: 26 });
        assert_eq!(parse_locator("[26]"), Locator::Para { from: 26, to: 26 });
        assert_eq!(
            parse_locator("at para 26"),
            Locator::Para { from: 26, to: 26 }
        );
        assert_eq!(
            parse_locator("L120-128"),
            Locator::Line { from: 120, to: 128 }
        );
        assert_eq!(parse_locator("§3.2"), Locator::Section { id: "3.2".into() });
        assert_eq!(parse_locator(""), Locator::Whole);
    }

    #[test]
    fn verify_with_pages_and_hyphenation() {
        let text = "<!-- page:1 -->\nThe inter-\nnational “court” said: counsel must verify.\n<!-- page:2 -->\nOther text here.\n";
        let cit = Citation {
            label: "1".into(),
            source: "s".into(),
            locator: parse_locator("p.1"),
            locator_raw: "p.1".into(),
            quote: Some("the international \"court\" said: counsel must verify".into()),
            comment: None,
            line: 1,
            raw: String::new(),
        };
        let (v, l, _, _) = verify_quote(text, &cit, 3, 80);
        assert_eq!(v, Verdict::Pass);
        assert_eq!(l, Some(2));
        let cit2 = Citation {
            locator: parse_locator("p.2"),
            locator_raw: "p.2".into(),
            ..cit.clone()
        };
        let (v, _, _, _) = verify_quote(text, &cit2, 3, 80);
        assert_eq!(v, Verdict::QuoteNotInSpan);
        let cit3 = Citation {
            quote: Some("counsel must ... verify".into()),
            ..cit.clone()
        };
        let (v, _, _, _) = verify_quote(text, &cit3, 3, 80);
        assert_eq!(v, Verdict::Pass);
        let cit4 = Citation {
            quote: Some("counsel must never verify".into()),
            ..cit.clone()
        };
        let (v, _, near, _) = verify_quote(text, &cit4, 3, 80);
        assert_eq!(v, Verdict::QuoteNotFound);
        assert!(!near.is_empty());
    }
}
