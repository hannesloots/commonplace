//! In-memory BM25 search over pages (title, aliases, tags boosted) and,
//! optionally, source text layers. Built per run: a few hundred pages take
//! milliseconds, and nothing on disk can go stale.

use crate::vault::{Page, Vault};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub rel: String,
    pub(crate) title: String,
    #[serde(rename = "type")]
    pub ptype: String,
    pub score: f64,
    pub snippet: String,
    pub line: Option<usize>,
}

pub struct SearchOptions {
    pub scope: Option<String>, // folder prefix, e.g. "projects/foo"
    pub include_archive: bool,
    pub include_text_layer: bool,
    pub ptype: Option<String>,
    pub limit: usize,
}

pub(crate) fn tokenize(s: &str) -> Vec<String> {
    let n = crate::cite::normalise(s);
    n.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'')
        .filter(|w| w.len() > 1)
        .map(|w| w.trim_matches(|c| c == '-' || c == '\'').to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

struct Doc {
    rel: String,
    title: String,
    ptype: String,
    tf: HashMap<String, f64>,
    len: f64,
    text: String,
}

impl Doc {
    fn new(
        rel: String,
        title: String,
        ptype: String,
        tf: HashMap<String, f64>,
        text: String,
    ) -> Self {
        let len = tf.values().sum::<f64>().max(1.0);
        Doc {
            rel,
            title,
            ptype,
            tf,
            len,
            text,
        }
    }
}

fn add_terms(tf: &mut HashMap<String, f64>, text: &str, weight: f64) {
    for w in tokenize(text) {
        *tf.entry(w).or_default() += weight;
    }
}

/// Whether a page passes the archive, scope and type filters.
fn page_in_scope(p: &Page, opts: &SearchOptions) -> bool {
    (opts.include_archive || !p.is_archived())
        && opts
            .scope
            .as_ref()
            .is_none_or(|s| p.rel.starts_with(s.trim_end_matches('/')))
        && opts.ptype.as_ref().is_none_or(|t| &p.ptype == t)
}

/// A page as a search document: title weighs 3, aliases, tags and description 2, body 1.
fn page_doc(p: &Page) -> Doc {
    let mut tf: HashMap<String, f64> = HashMap::new();
    add_terms(&mut tf, &p.title, 3.0);
    for a in &p.aliases {
        add_terms(&mut tf, a, 2.0);
    }
    for t in &p.tags {
        add_terms(&mut tf, t, 2.0);
    }
    if let Some(d) = p.get("description") {
        add_terms(&mut tf, &d, 2.0);
    }
    add_terms(&mut tf, &p.doc.body, 1.0);
    Doc::new(
        p.rel.clone(),
        p.title.clone(),
        p.ptype.clone(),
        tf,
        p.doc.body.clone(),
    )
}

/// Every source text layer in `sources/text` as a search document.
fn text_layer_docs(vault: &Vault) -> Vec<Doc> {
    let Ok(rd) = std::fs::read_dir(vault.root.join("sources").join("text")) else {
        return vec![];
    };
    let mut docs = vec![];
    for path in rd.flatten().map(|e| e.path()) {
        if !crate::fsutil::is_markdown(&path) {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            let mut tf: HashMap<String, f64> = HashMap::new();
            add_terms(&mut tf, &text, 1.0);
            docs.push(Doc::new(
                crate::fsutil::rel(&vault.root, &path),
                crate::fsutil::stem(&path),
                "source-text".into(),
                tf,
                text,
            ));
        }
    }
    docs
}

pub fn search(vault: &Vault, query: &str, opts: &SearchOptions) -> Vec<Hit> {
    let mut docs: Vec<Doc> = vault
        .pages
        .iter()
        .filter(|p| page_in_scope(p, opts))
        .map(page_doc)
        .collect();
    if opts.include_text_layer {
        docs.extend(text_layer_docs(vault));
    }
    let mut hits = bm25(&docs, &tokenize(query));
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits.truncate(opts.limit);
    hits
}

/// Okapi BM25 (k1 = 1.2, b = 0.75); documents without a matching term are dropped.
fn bm25(docs: &[Doc], qtokens: &[String]) -> Vec<Hit> {
    let n = crate::count_f64(docs.len());
    if n == 0.0 {
        return vec![];
    }
    let avg_len = docs.iter().map(|d| d.len).sum::<f64>() / n;
    let mut df: HashMap<&str, f64> = HashMap::new();
    for q in qtokens {
        df.insert(
            q,
            crate::count_f64(docs.iter().filter(|d| d.tf.contains_key(q)).count()),
        );
    }
    let (k1, b) = (1.2, 0.75);
    docs.iter()
        .filter_map(|d| {
            let mut score = 0.0;
            for q in qtokens {
                let f = *d.tf.get(q).unwrap_or(&0.0);
                if f == 0.0 {
                    continue;
                }
                let idf = ((n - df[q.as_str()] + 0.5) / (df[q.as_str()] + 0.5) + 1.0).ln();
                score += idf * (f * (k1 + 1.0)) / (f + k1 * (1.0 - b + b * d.len / avg_len));
            }
            if score <= 0.0 {
                return None;
            }
            let (snippet, line) = snippet(&d.text, qtokens);
            Some(Hit {
                rel: d.rel.clone(),
                title: d.title.clone(),
                ptype: d.ptype.clone(),
                score,
                snippet,
                line,
            })
        })
        .collect()
}

fn snippet(text: &str, q: &[String]) -> (String, Option<usize>) {
    let mut best: Option<(usize, usize, &str)> = None;
    for (i, line) in text.lines().enumerate() {
        let lt = crate::cite::normalise(line);
        let c = q.iter().filter(|t| lt.contains(t.as_str())).count();
        if c > 0 && best.is_none_or(|b| c > b.1) {
            best = Some((i + 1, c, line));
        }
    }
    match best {
        Some((ln, _, line)) => {
            let s: String = line.trim().chars().take(160).collect();
            (s, Some(ln))
        }
        None => (String::new(), None),
    }
}
