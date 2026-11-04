//! The commonplace as a typed graph (ontology).
//!
//! Nodes are pages (label = `type`);
//! edges come from frontmatter link fields (relation = field name), inline
//! Dataview-style fields in the body (`enabled_by:: [[Page]]`), citations
//! (`cites`), and plain wikilinks (`mentions`). Supports multi-hop queries and
//! export to Cypher (Neo4j), `GraphML`, Mermaid, DOT and JSON.

use crate::cite;
use crate::markdown;
use crate::vault::{Page, Resolution, Vault};
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::sync::LazyLock;

static INLINE_FIELD_RE: LazyLock<Regex> =
    LazyLock::new(|| crate::re(r"(?m)^\s*(?:[-*]\s*)?([A-Za-z][A-Za-z0-9_\-]*)::\s*(.+)$"));

/// A page as a graph node.
#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub(crate) id: String, // page stem (unique within the commonplace)
    pub(crate) rel: String,
    pub(crate) label: String, // type
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) status: String,
    pub(crate) folder: String,
}

/// A typed, directed link between two pages.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
pub struct Edge {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) rel: String,
}

/// The commonplace as a typed graph of pages and links.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Graph {
    /// Every page, in vault order.
    pub nodes: Vec<Node>,
    /// Every link between pages.
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

/// Page types that are navigation or bookkeeping, not graph nodes.
const NON_NODE_TYPES: [&str; 6] = [
    "index",
    "log",
    "registry",
    "questions",
    "error-book",
    "review",
];

/// The node for a page, or `None` for unparseable and navigation pages.
fn graph_node(p: &Page) -> Option<Node> {
    if p.parse_error.is_some()
        || p.rel == "index.md"
        || p.rel.starts_with("indexes/")
        || NON_NODE_TYPES.contains(&p.ptype.as_str())
    {
        return None;
    }
    Some(Node {
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
    })
}

impl Graph {
    /// Add an edge unless it is a self loop or already present.
    fn add_edge(&mut self, seen: &mut HashSet<Edge>, from: &str, to: &str, rel: &str) {
        let e = Edge {
            from: from.to_string(),
            to: to.to_string(),
            rel: rel.to_string(),
        };
        if from.eq_ignore_ascii_case(to) || !seen.insert(e.clone()) {
            return;
        }
        let i = self.edges.len();
        self.edges.push(e);
        self.out.entry(from.to_lowercase()).or_default().push(i);
        self.inn.entry(to.to_lowercase()).or_default().push(i);
    }
}

/// The stem of the one page `target` resolves to.
fn resolve_stem(vault: &Vault, target: &str) -> Option<String> {
    match vault.resolve(target) {
        Resolution::Found(i) => Some(vault.pages[i].stem.clone()),
        _ => None,
    }
}

/// A page's outgoing edges, in order: frontmatter link fields (`sources` becomes
/// `cites`), inline `field:: [[x]]` fields, citations, then plain wikilinks as
/// `mentions` when asked for. Unresolvable targets are skipped.
fn page_edges(vault: &Vault, p: &Page, include_mentions: bool) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    for (field, target) in &p.fm_links {
        if NON_RELATION_FIELDS.contains(&field.as_str()) {
            continue;
        }
        if let Some(t) = resolve_stem(vault, target) {
            let rel = if field == "sources" { "cites" } else { field };
            out.push((t, rel.to_string()));
        }
    }
    let masked = markdown::mask_comments(&markdown::mask_code(&p.doc.body));
    for c in INLINE_FIELD_RE.captures_iter(&masked) {
        let field = c[1].to_lowercase();
        for target in markdown::wikilink_targets_in(&c[2]) {
            if let Some(t) = resolve_stem(vault, &target) {
                out.push((t, field.clone()));
            }
        }
    }
    let (cites, _) = cite::parse_citations(&p.doc.body);
    for c in &cites {
        if let Some(t) = resolve_stem(vault, &c.source) {
            out.push((t, "cites".into()));
        }
    }
    if include_mentions {
        for l in &p.body_links {
            if let Some(t) = resolve_stem(vault, &l.target) {
                out.push((t, "mentions".into()));
            }
        }
    }
    out
}

/// Build the graph of `vault`; `include_mentions` adds unlinked title mentions as `mentions` edges.
pub fn build(vault: &Vault, include_mentions: bool) -> Graph {
    let mut g = Graph::default();
    for node in vault.pages.iter().filter_map(graph_node) {
        g.index.insert(node.id.to_lowercase(), g.nodes.len());
        g.nodes.push(node);
    }
    let mut seen: HashSet<Edge> = HashSet::new();
    for p in &vault.pages {
        if !g.index.contains_key(&p.stem.to_lowercase()) {
            continue;
        }
        for (to, rel) in page_edges(vault, p, include_mentions) {
            g.add_edge(&mut seen, &p.stem, &to, &rel);
        }
    }
    g
}

impl Graph {
    /// The node with id `id` (page stem), if any.
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
        let (Some(start), Some(goal)) = (self.node(from), self.node(to)) else {
            return vec![];
        };
        let target = goal.id.to_lowercase();
        let mut results: Vec<Vec<String>> = vec![];
        let mut queue: VecDeque<(String, Vec<String>, HashSet<String>)> = VecDeque::new();
        let mut start_seen = HashSet::new();
        start_seen.insert(start.id.to_lowercase());
        queue.push_back((start.id.clone(), vec![start.id.clone()], start_seen));
        let mut best_len: Option<usize> = None;
        while let Some((id, path, visited)) = queue.pop_front() {
            if let Some(bl) = best_len
                && path.len() > bl
            {
                break;
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
                queue.push_back((other, p, v));
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

    /// Node, edge and component counts, hubs and isolated pages.
    pub fn stats(&self) -> Stats {
        let mut deg: Vec<(String, usize, usize)> = self
            .nodes
            .iter()
            .map(|n| {
                (
                    n.id.clone(),
                    self.out
                        .get(&n.id.to_lowercase())
                        .map_or(0, std::vec::Vec::len),
                    self.inn
                        .get(&n.id.to_lowercase())
                        .map_or(0, std::vec::Vec::len),
                )
            })
            .collect();
        deg.sort_by_key(|d| std::cmp::Reverse(d.1 + d.2));
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

    /// Export as Cypher `CREATE` statements (Neo4j, Memgraph).
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
            let _ = writeln!(
                s,
                "MERGE (n:Page {{id: '{}'}}) SET n:{}, n.title = '{}', n.path = '{}', n.description = '{}', n.status = '{}';",
                esc(&n.id),
                label(&n.label),
                esc(&n.title),
                esc(&n.rel),
                esc(&n.description),
                esc(&n.status)
            );
        }
        for e in &self.edges {
            let _ = writeln!(
                s,
                "MATCH (a:Page {{id: '{}'}}), (b:Page {{id: '{}'}}) MERGE (a)-[:{}]->(b);",
                esc(&e.from),
                esc(&e.to),
                label(&e.rel).to_uppercase()
            );
        }
        s
    }

    /// Export as `GraphML` (yEd, Gephi).
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
            let _ = writeln!(
                s,
                "<node id=\"{}\"><data key=\"label\">{}</data><data key=\"title\">{}</data><data key=\"path\">{}</data></node>",
                esc(&n.id),
                esc(&n.label),
                esc(&n.title),
                esc(&n.rel)
            );
        }
        for (i, e) in self.edges.iter().enumerate() {
            let _ = writeln!(
                s,
                "<edge id=\"e{i}\" source=\"{}\" target=\"{}\"><data key=\"rel\">{}</data></edge>",
                esc(&e.from),
                esc(&e.to),
                esc(&e.rel)
            );
        }
        s.push_str("</graph>\n</graphml>\n");
        s
    }

    /// Export as a Mermaid flowchart, limited to `ids` when given.
    pub fn to_mermaid(&self, ids: Option<&HashSet<String>>) -> String {
        let mut s = String::from("graph LR\n");
        let key = |id: &str| -> String {
            format!("n{}", &crate::fsutil::sha256_str(&id.to_lowercase())[..8])
        };
        for n in &self.nodes {
            if ids.is_none_or(|set| set.contains(&n.id.to_lowercase())) {
                let _ = writeln!(
                    s,
                    "  {}[\"{}<br/><i>{}</i>\"]",
                    key(&n.id),
                    n.id.replace('"', "'"),
                    n.label
                );
            }
        }
        for e in &self.edges {
            if ids.is_none_or(|set| {
                set.contains(&e.from.to_lowercase()) && set.contains(&e.to.to_lowercase())
            }) {
                let _ = writeln!(s, "  {} -->|{}| {}", key(&e.from), e.rel, key(&e.to));
            }
        }
        s
    }

    /// Export as Graphviz DOT.
    pub fn to_dot(&self) -> String {
        let esc = |s: &str| s.replace('"', "\\\"");
        let mut s =
            String::from("digraph commonplace {\n  rankdir=LR; node [shape=box, fontsize=10];\n");
        for n in &self.nodes {
            let _ = writeln!(
                s,
                "  \"{}\" [label=\"{}\\n({})\"];",
                esc(&n.id),
                esc(&n.id),
                n.label
            );
        }
        for e in &self.edges {
            let _ = writeln!(
                s,
                "  \"{}\" -> \"{}\" [label=\"{}\"];",
                esc(&e.from),
                esc(&e.to),
                e.rel
            );
        }
        s.push_str("}\n");
        s
    }
}

/// A page reached by a walk.
#[derive(Debug, Clone, Serialize)]
pub struct WalkRow {
    /// Node id (page stem).
    pub node: String,
    /// Node label (page type).
    pub label: String,
    /// Hops from the start page.
    pub depth: usize,
    /// The chain of relations that reached it.
    pub path: String,
    /// The page's `description`.
    pub description: String,
}

/// The ontology in use: page types and relation types.
#[derive(Debug, Clone, Serialize)]
pub struct Schema {
    /// Count of pages per type.
    pub labels: BTreeMap<String, usize>,
    /// Relation types with their `from→to` type pairs.
    pub relations: Vec<RelationSpec>,
}

/// One relation type and where it occurs.
#[derive(Debug, Clone, Serialize)]
pub struct RelationSpec {
    /// Relation name (frontmatter field, or `links`/`mentions`).
    pub rel: String,
    /// Count of edges per `from→to` type pair.
    pub pairs: BTreeMap<String, usize>,
}

/// Summary statistics of the graph.
#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    /// Number of pages.
    pub nodes: usize,
    /// Number of links.
    pub edges: usize,
    /// Number of connected components.
    pub components: usize,
    /// Sizes of the largest components.
    pub largest_components: Vec<usize>,
    /// Pages with the most links, with their degree.
    pub hubs: Vec<(String, usize)>,
    /// Pages with no links at all.
    pub isolated: Vec<String>,
}
