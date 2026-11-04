//! The commonplace as a typed graph (ontology). Nodes are pages (label = `type`);
//! edges come from frontmatter link fields (relation = field name), inline
//! Dataview-style fields in the body (`enabled_by:: [[Page]]`), citations
//! (`cites`), and plain wikilinks (`mentions`). Supports multi-hop queries and
//! export to Cypher (Neo4j), GraphML, Mermaid, DOT and JSON.

use crate::cite;
use crate::markdown;
use crate::vault::{Resolution, Vault};
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::LazyLock;

static INLINE_FIELD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*(?:[-*]\s*)?([A-Za-z][A-Za-z0-9_\-]*)::\s*(.+)$").unwrap()
});

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id: String, // page stem (unique within the commonplace)
    pub rel: String,
    pub label: String, // type
    pub title: String,
    pub description: String,
    pub status: String,
    pub folder: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub rel: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    #[serde(skip)]
    index: HashMap<String, usize>,
    #[serde(skip)]
    out: HashMap<String, Vec<usize>>,
    #[serde(skip)]
    inn: HashMap<String, Vec<usize>>,
}

/// Frontmatter fields that are structural, not relations.
const NON_RELATION_FIELDS: &[&str] = &["aliases", "tags"];

pub fn build(vault: &Vault, include_mentions: bool) -> Graph {
    let mut g = Graph::default();
    for p in &vault.pages {
        if p.parse_error.is_some()
            || p.rel == "index.md"
            || p.rel.starts_with("indexes/")
            || [
                "index",
                "log",
                "registry",
                "questions",
                "error-book",
                "review",
            ]
            .contains(&p.ptype.as_str())
        {
            continue;
        }
        g.index.insert(p.stem.to_lowercase(), g.nodes.len());
        g.nodes.push(Node {
            id: p.stem.clone(),
            rel: p.rel.clone(),
            label: if p.ptype.is_empty() {
                "page".into()
            } else {
                p.ptype.clone()
            },
            title: p.title.clone(),
            description: p.get("description").unwrap_or_default(),
            status: p.get("status").unwrap_or_default(),
            folder: p.folder(),
        });
    }
    let mut seen: HashSet<Edge> = HashSet::new();
    let mut add = |g: &mut Graph, from: &str, to: &str, rel: &str| {
        let e = Edge {
            from: from.to_string(),
            to: to.to_string(),
            rel: rel.to_string(),
        };
        if from.eq_ignore_ascii_case(to) || !seen.insert(e.clone()) {
            return;
        }
        let i = g.edges.len();
        g.edges.push(e);
        g.out.entry(from.to_lowercase()).or_default().push(i);
        g.inn.entry(to.to_lowercase()).or_default().push(i);
    };
    let resolve = |t: &str| -> Option<String> {
        match vault.resolve(t) {
            Resolution::Found(i) => Some(vault.pages[i].stem.clone()),
            _ => None,
        }
    };
    for p in &vault.pages {
        if !g.index.contains_key(&p.stem.to_lowercase()) {
            continue;
        }
        // typed: frontmatter link fields
        for (field, target) in &p.fm_links {
            if NON_RELATION_FIELDS.contains(&field.as_str()) {
                continue;
            }
            if let Some(t) = resolve(target) {
                let rel = if field == "sources" {
                    "cites"
                } else {
                    field.as_str()
                };
                add(&mut g, &p.stem, &t, rel);
            }
        }
        // typed: inline fields in body
        let masked = markdown::mask_comments(&markdown::mask_code(&p.doc.body));
        for c in INLINE_FIELD_RE.captures_iter(&masked) {
            let field = c[1].to_lowercase();
            for target in markdown::wikilink_targets_in(&c[2]) {
                if let Some(t) = resolve(&target) {
                    add(&mut g, &p.stem, &t, &field);
                }
            }
        }
        // citations
        let (cites, _) = cite::parse_citations(&p.doc.body);
        for c in &cites {
            if let Some(t) = resolve(&c.source) {
                add(&mut g, &p.stem, &t, "cites");
            }
        }
        // mentions
        if include_mentions {
            for l in &p.body_links {
                if let Some(t) = resolve(&l.target) {
                    add(&mut g, &p.stem, &t, "mentions");
                }
            }
        }
    }
    g
}

impl Graph {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.index.get(&id.to_lowercase()).map(|&i| &self.nodes[i])
    }

    fn neighbours(&self, id: &str, rels: &[String], direction: &str) -> Vec<(&Edge, String)> {
        let mut v = vec![];
        let key = id.to_lowercase();
        if direction != "in" {
            for &i in self.out.get(&key).into_iter().flatten() {
                let e = &self.edges[i];
                if rels.is_empty() || rels.iter().any(|r| r == &e.rel) {
                    v.push((e, e.to.clone()));
                }
            }
        }
        if direction != "out" {
            for &i in self.inn.get(&key).into_iter().flatten() {
                let e = &self.edges[i];
                if rels.is_empty() || rels.iter().any(|r| r == &e.rel) {
                    v.push((e, e.from.clone()));
                }
            }
        }
        v
    }

    /// Breadth-first neighbourhood up to `depth` hops.
    pub fn walk(
        &self,
        start: &str,
        depth: usize,
        rels: &[String],
        direction: &str,
    ) -> Vec<WalkRow> {
        let Some(s) = self.node(start) else {
            return vec![];
        };
        let mut seen: HashSet<String> = HashSet::new();
        seen.insert(s.id.to_lowercase());
        let mut q: VecDeque<(String, usize, Vec<String>)> = VecDeque::new();
        q.push_back((s.id.clone(), 0, vec![]));
        let mut out = vec![];
        while let Some((id, d, path)) = q.pop_front() {
            if d >= depth {
                continue;
            }
            for (e, other) in self.neighbours(&id, rels, direction) {
                let step = if e.from.eq_ignore_ascii_case(&id) {
                    format!("-[{}]-> {}", e.rel, other)
                } else {
                    format!("<-[{}]- {}", e.rel, other)
                };
                let mut p = path.clone();
                p.push(step);
                if seen.insert(other.to_lowercase()) {
                    let n = self.node(&other);
                    out.push(WalkRow {
                        node: other.clone(),
                        label: n.map(|n| n.label.clone()).unwrap_or_default(),
                        depth: d + 1,
                        path: p.join(" "),
                        description: n.map(|n| n.description.clone()).unwrap_or_default(),
                    });
                    q.push_back((other, d + 1, p));
                }
            }
        }
        out
    }

    /// Shortest paths (up to `limit`) between two nodes, ignoring direction unless given.
    pub fn paths(
        &self,
        from: &str,
        to: &str,
        max_depth: usize,
        rels: &[String],
        direction: &str,
        limit: usize,
    ) -> Vec<Vec<String>> {
        let (Some(a), Some(b)) = (self.node(from), self.node(to)) else {
            return vec![];
        };
        let target = b.id.to_lowercase();
        let mut results: Vec<Vec<String>> = vec![];
        let mut q: VecDeque<(String, Vec<String>, HashSet<String>)> = VecDeque::new();
        let mut start_seen = HashSet::new();
        start_seen.insert(a.id.to_lowercase());
        q.push_back((a.id.clone(), vec![a.id.clone()], start_seen));
        let mut best_len: Option<usize> = None;
        while let Some((id, path, visited)) = q.pop_front() {
            if let Some(bl) = best_len {
                if path.len() > bl {
                    break;
                }
            }
            if path.len() > max_depth * 2 + 1 {
                continue;
            }
            for (e, other) in self.neighbours(&id, rels, direction) {
                let step = if e.from.eq_ignore_ascii_case(&id) {
                    format!("-[{}]->", e.rel)
                } else {
                    format!("<-[{}]-", e.rel)
                };
                if visited.contains(&other.to_lowercase()) {
                    continue;
                }
                let mut p = path.clone();
                p.push(step);
                p.push(other.clone());
                if other.to_lowercase() == target {
                    best_len = Some(p.len());
                    results.push(p);
                    if results.len() >= limit {
                        return results;
                    }
                    continue;
                }
                let mut v = visited.clone();
                v.insert(other.to_lowercase());
                q.push_back((other, p, v));
            }
        }
        results
    }

    /// The ontology: node labels with counts, relation types with from→to label pairs.
    pub fn schema(&self) -> Schema {
        let mut labels: BTreeMap<String, usize> = BTreeMap::new();
        for n in &self.nodes {
            *labels.entry(n.label.clone()).or_default() += 1;
        }
        let mut rels: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        for e in &self.edges {
            let fl = self
                .node(&e.from)
                .map(|n| n.label.clone())
                .unwrap_or_default();
            let tl = self
                .node(&e.to)
                .map(|n| n.label.clone())
                .unwrap_or_default();
            *rels
                .entry(e.rel.clone())
                .or_default()
                .entry(format!("{fl} → {tl}"))
                .or_default() += 1;
        }
        Schema {
            labels,
            relations: rels
                .into_iter()
                .map(|(rel, pairs)| RelationSpec { rel, pairs })
                .collect(),
        }
    }

    pub fn stats(&self) -> Stats {
        let mut deg: Vec<(String, usize, usize)> = self
            .nodes
            .iter()
            .map(|n| {
                (
                    n.id.clone(),
                    self.out
                        .get(&n.id.to_lowercase())
                        .map(|v| v.len())
                        .unwrap_or(0),
                    self.inn
                        .get(&n.id.to_lowercase())
                        .map(|v| v.len())
                        .unwrap_or(0),
                )
            })
            .collect();
        deg.sort_by(|a, b| (b.1 + b.2).cmp(&(a.1 + a.2)));
        let hubs: Vec<(String, usize)> = deg
            .iter()
            .take(10)
            .map(|(id, o, i)| (id.clone(), o + i))
            .collect();
        let isolated: Vec<String> = deg
            .iter()
            .filter(|(_, o, i)| o + i == 0)
            .map(|(id, _, _)| id.clone())
            .collect();
        // connected components (undirected)
        let mut comp: HashMap<String, usize> = HashMap::new();
        let mut ncomp = 0;
        for n in &self.nodes {
            let key = n.id.to_lowercase();
            if comp.contains_key(&key) {
                continue;
            }
            ncomp += 1;
            let mut stack = vec![n.id.clone()];
            while let Some(id) = stack.pop() {
                let k = id.to_lowercase();
                if comp.contains_key(&k) {
                    continue;
                }
                comp.insert(k, ncomp);
                for (_, other) in self.neighbours(&id, &[], "both") {
                    stack.push(other);
                }
            }
        }
        let mut sizes: HashMap<usize, usize> = HashMap::new();
        for c in comp.values() {
            *sizes.entry(*c).or_default() += 1;
        }
        let mut comp_sizes: Vec<usize> = sizes.values().copied().collect();
        comp_sizes.sort_by(|a, b| b.cmp(a));
        Stats {
            nodes: self.nodes.len(),
            edges: self.edges.len(),
            components: ncomp,
            largest_components: comp_sizes.into_iter().take(5).collect(),
            hubs,
            isolated,
        }
    }

    pub fn to_cypher(&self) -> String {
        let esc = |s: &str| s.replace('\\', "\\\\").replace('\'', "\\'");
        let label = |s: &str| {
            let mut l: String = s
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect();
            if l.is_empty() {
                l = "Page".into();
            }
            let mut cs = l.chars();
            cs.next()
                .map(|f| f.to_uppercase().collect::<String>() + cs.as_str())
                .unwrap_or(l)
        };
        let mut s = String::from(
            "// Generated by `commonplace graph export --format cypher`. Load with cypher-shell or Neo4j Browser.\nCREATE CONSTRAINT commonplace_page_id IF NOT EXISTS FOR (p:Page) REQUIRE p.id IS UNIQUE;\n",
        );
        for n in &self.nodes {
            s.push_str(&format!("MERGE (n:Page {{id: '{}'}}) SET n:{}, n.title = '{}', n.path = '{}', n.description = '{}', n.status = '{}';\n", esc(&n.id), label(&n.label), esc(&n.title), esc(&n.rel), esc(&n.description), esc(&n.status)));
        }
        for e in &self.edges {
            s.push_str(&format!(
                "MATCH (a:Page {{id: '{}'}}), (b:Page {{id: '{}'}}) MERGE (a)-[:{}]->(b);\n",
                esc(&e.from),
                esc(&e.to),
                label(&e.rel).to_uppercase()
            ));
        }
        s
    }

    pub fn to_graphml(&self) -> String {
        let esc = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
        };
        let mut s = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n<key id=\"label\" for=\"node\" attr.name=\"label\" attr.type=\"string\"/>\n<key id=\"title\" for=\"node\" attr.name=\"title\" attr.type=\"string\"/>\n<key id=\"path\" for=\"node\" attr.name=\"path\" attr.type=\"string\"/>\n<key id=\"rel\" for=\"edge\" attr.name=\"rel\" attr.type=\"string\"/>\n<graph id=\"commonplace\" edgedefault=\"directed\">\n",
        );
        for n in &self.nodes {
            s.push_str(&format!("<node id=\"{}\"><data key=\"label\">{}</data><data key=\"title\">{}</data><data key=\"path\">{}</data></node>\n", esc(&n.id), esc(&n.label), esc(&n.title), esc(&n.rel)));
        }
        for (i, e) in self.edges.iter().enumerate() {
            s.push_str(&format!("<edge id=\"e{i}\" source=\"{}\" target=\"{}\"><data key=\"rel\">{}</data></edge>\n", esc(&e.from), esc(&e.to), esc(&e.rel)));
        }
        s.push_str("</graph>\n</graphml>\n");
        s
    }

    pub fn to_mermaid(&self, ids: Option<&HashSet<String>>) -> String {
        let mut s = String::from("graph LR\n");
        let key = |id: &str| -> String {
            format!(
                "n{}",
                crate::fsutil::sha256_str(&id.to_lowercase())[..8].to_string()
            )
        };
        for n in &self.nodes {
            if ids
                .map(|set| set.contains(&n.id.to_lowercase()))
                .unwrap_or(true)
            {
                s.push_str(&format!(
                    "  {}[\"{}<br/><i>{}</i>\"]\n",
                    key(&n.id),
                    n.id.replace('"', "'"),
                    n.label
                ));
            }
        }
        for e in &self.edges {
            if ids
                .map(|set| {
                    set.contains(&e.from.to_lowercase()) && set.contains(&e.to.to_lowercase())
                })
                .unwrap_or(true)
            {
                s.push_str(&format!(
                    "  {} -->|{}| {}\n",
                    key(&e.from),
                    e.rel,
                    key(&e.to)
                ));
            }
        }
        s
    }

    pub fn to_dot(&self) -> String {
        let esc = |s: &str| s.replace('"', "\\\"");
        let mut s =
            String::from("digraph commonplace {\n  rankdir=LR; node [shape=box, fontsize=10];\n");
        for n in &self.nodes {
            s.push_str(&format!(
                "  \"{}\" [label=\"{}\\n({})\"];\n",
                esc(&n.id),
                esc(&n.id),
                n.label
            ));
        }
        for e in &self.edges {
            s.push_str(&format!(
                "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
                esc(&e.from),
                esc(&e.to),
                e.rel
            ));
        }
        s.push_str("}\n");
        s
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WalkRow {
    pub node: String,
    pub label: String,
    pub depth: usize,
    pub path: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Schema {
    pub labels: BTreeMap<String, usize>,
    pub relations: Vec<RelationSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelationSpec {
    pub rel: String,
    pub pairs: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub nodes: usize,
    pub edges: usize,
    pub components: usize,
    pub largest_components: Vec<usize>,
    pub hubs: Vec<(String, usize)>,
    pub isolated: Vec<String>,
}
