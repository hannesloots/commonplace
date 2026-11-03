//! In-memory BM25 search over pages (title, aliases, tags boosted) and,
//! optionally, source text layers. Built per run: a few hundred pages take
//! milliseconds, and nothing on disk can go stale.

use crate::vault::Vault;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub rel: String,
    pub title: String,
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

pub fn tokenize(s: &str) -> Vec<String> {
    let n = crate::cite::normalise(s);
    n.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'').filter(|w| w.len() > 1).map(|w| w.trim_matches(|c| c == '-' || c == '\'').to_string()).filter(|w| !w.is_empty()).collect()
}

struct Doc {
    rel: String,
    title: String,
    ptype: String,
    tf: HashMap<String, f64>,
    len: f64,
    text: String,
}

pub fn search(vault: &Vault, query: &str, opts: &SearchOptions) -> Vec<Hit> {
    let mut docs: Vec<Doc> = vec![];
    for p in &vault.pages {
        if !opts.include_archive && p.is_archived() {
            continue;
        }
        if let Some(s) = &opts.scope {
            if !p.rel.starts_with(s.trim_end_matches('/')) {
                continue;
            }
        }
        if let Some(t) = &opts.ptype {
            if &p.ptype != t {
                continue;
            }
        }
        let mut tf: HashMap<String, f64> = HashMap::new();
        for w in tokenize(&p.title) {
            *tf.entry(w).or_default() += 3.0;
        }
        for a in &p.aliases {
            for w in tokenize(a) {
                *tf.entry(w).or_default() += 2.0;
            }
        }
        for t in &p.tags {
            for w in tokenize(t) {
                *tf.entry(w).or_default() += 2.0;
            }
        }
        if let Some(d) = p.get("description") {
            for w in tokenize(&d) {
                *tf.entry(w).or_default() += 2.0;
            }
        }
        let body_tokens = tokenize(&p.doc.body);
        for w in &body_tokens {
            *tf.entry(w.clone()).or_default() += 1.0;
        }
        let len = tf.values().sum::<f64>().max(1.0);
        docs.push(Doc { rel: p.rel.clone(), title: p.title.clone(), ptype: p.ptype.clone(), tf, len, text: p.doc.body.clone() });
    }
    if opts.include_text_layer {
        let dir = vault.root.join("sources").join("text");
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let path = e.path();
                if !crate::fsutil::is_markdown(&path) {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    let mut tf: HashMap<String, f64> = HashMap::new();
                    for w in tokenize(&text) {
                        *tf.entry(w).or_default() += 1.0;
                    }
                    let len = tf.values().sum::<f64>().max(1.0);
                    docs.push(Doc { rel: crate::fsutil::rel(&vault.root, &path), title: crate::fsutil::stem(&path), ptype: "source-text".into(), tf, len, text });
                }
            }
        }
    }
    let n = docs.len() as f64;
    if n == 0.0 {
        return vec![];
    }
    let avg_len = docs.iter().map(|d| d.len).sum::<f64>() / n;
    let qtokens = tokenize(query);
    let mut df: HashMap<&str, f64> = HashMap::new();
    for q in &qtokens {
        df.insert(q, docs.iter().filter(|d| d.tf.contains_key(q)).count() as f64);
    }
    let (k1, b) = (1.2, 0.75);
    let mut hits: Vec<Hit> = docs
        .iter()
        .filter_map(|d| {
            let mut score = 0.0;
            for q in &qtokens {
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
            let (snippet, line) = snippet(&d.text, &qtokens);
            Some(Hit { rel: d.rel.clone(), title: d.title.clone(), ptype: d.ptype.clone(), score, snippet, line })
        })
        .collect();
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    hits.truncate(opts.limit);
    hits
}

fn snippet(text: &str, q: &[String]) -> (String, Option<usize>) {
    let mut best: Option<(usize, usize, &str)> = None;
    for (i, line) in text.lines().enumerate() {
        let lt = crate::cite::normalise(line);
        let c = q.iter().filter(|t| lt.contains(t.as_str())).count();
        if c > 0 && best.map(|b| c > b.1).unwrap_or(true) {
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
