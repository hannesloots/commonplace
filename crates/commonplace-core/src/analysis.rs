//! Graph analysis over the page graph, in memory: related pages, hubs
//! (`PageRank`), communities (Louvain), gaps (wanted pages, orphans, dead ends,
//! uncited sources, bridges), unlinked mentions and contradiction candidates.
//! Every result carries its reason so the agent can judge, not just trust.

use crate::graph::Graph;
use crate::vault::{Page, Resolution, Vault};
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::LazyLock;

/// Pages that carry knowledge (orphans/dead ends are judged on these).
const SYNTHESIS: &[&str] = &[
    "concept",
    "entity",
    "synthesis",
    "playbook",
    "decision",
    "person",
    "topic",
    "project",
    "area",
];
/// Pages that act as maps: a community without one is a synthesis gap.
const HUB_TYPES: &[&str] = &["topic", "project", "area"];

/// Undirected weighted view of the graph for the algorithms.
pub struct Net {
    pub n: usize,
    pub adj: Vec<Vec<(usize, f64)>>,
    pub id_of: HashMap<String, usize>,
}

impl Net {
    pub fn from_graph(g: &Graph) -> Net {
        let mut id_of = HashMap::new();
        for (i, n) in g.nodes.iter().enumerate() {
            id_of.insert(n.id.to_lowercase(), i);
        }
        let mut pairs: HashMap<(usize, usize), f64> = HashMap::new();
        for e in &g.edges {
            let (Some(&a), Some(&b)) = (
                id_of.get(&e.from.to_lowercase()),
                id_of.get(&e.to.to_lowercase()),
            ) else {
                continue;
            };
            if a == b {
                continue;
            }
            let w = if e.rel == "mentions" { 0.5 } else { 1.0 };
            let key = (a.min(b), a.max(b));
            let cur = pairs.entry(key).or_insert(0.0);
            *cur = f64::max(*cur, w);
        }
        let mut adj = vec![vec![]; g.nodes.len()];
        let mut keys: Vec<_> = pairs.into_iter().collect();
        keys.sort_by_key(|x| x.0);
        for ((a, b), w) in keys {
            adj[a].push((b, w));
            adj[b].push((a, w));
        }
        Net {
            n: g.nodes.len(),
            adj,
            id_of,
        }
    }

    pub fn degree(&self, i: usize) -> usize {
        self.adj[i].len()
    }

    fn strength(&self, i: usize) -> f64 {
        self.adj[i].iter().map(|(_, w)| w).sum()
    }

    /// `PageRank` on the undirected graph (power iteration).
    pub fn pagerank(&self) -> Vec<f64> {
        self.personalised_pagerank(&[], 0.85)
    }

    /// Personalised `PageRank`: the random walker restarts at `seeds` (all nodes
    /// when empty). `damping` is the follow-a-link probability.
    pub fn personalised_pagerank(&self, seeds: &[usize], damping: f64) -> Vec<f64> {
        let n = self.n;
        if n == 0 {
            return vec![];
        }
        let restart: Vec<f64> = if seeds.is_empty() {
            vec![1.0 / crate::count_f64(n); n]
        } else {
            let mut r = vec![0.0; n];
            for &s in seeds {
                r[s] = 1.0 / crate::count_f64(seeds.len());
            }
            r
        };
        let strength: Vec<f64> = (0..n).map(|i| self.strength(i)).collect();
        let mut pr = restart.clone();
        for _ in 0..60 {
            let mut next = vec![0.0; n];
            let mut dangling = 0.0;
            for i in 0..n {
                if strength[i] == 0.0 {
                    dangling += pr[i];
                    continue;
                }
                for &(j, w) in &self.adj[i] {
                    next[j] += damping * pr[i] * w / strength[i];
                }
            }
            let mut delta = 0.0;
            for i in 0..n {
                next[i] += (1.0 - damping) * restart[i] + damping * dangling * restart[i];
                delta += (next[i] - pr[i]).abs();
            }
            pr = next;
            if delta < 1e-9 {
                break;
            }
        }
        pr
    }

    fn neighbour_set(&self, i: usize) -> HashSet<usize> {
        self.adj[i].iter().map(|(j, _)| *j).collect()
    }

    /// Adamic-Adar: shared neighbours weighted by 1/ln(degree), so a shared
    /// niche page counts more than a shared hub.
    pub fn adamic_adar(&self, a: usize, b: usize) -> (f64, Vec<usize>) {
        let na = self.neighbour_set(a);
        let nb = self.neighbour_set(b);
        let mut score = 0.0;
        let mut shared = vec![];
        for z in na.intersection(&nb) {
            let d = self.degree(*z);
            if d > 1 {
                score += 1.0 / crate::count_f64(d).ln();
                shared.push(*z);
            }
        }
        shared.sort_unstable();
        (score, shared)
    }

    /// Louvain community detection (modularity optimisation with
    /// aggregation). Deterministic: nodes are visited in index order.
    pub fn louvain(&self) -> Vec<usize> {
        let mut adj: Vec<Vec<(usize, f64)>> = self.adj.clone();
        let mut membership: Vec<usize> = (0..self.n).collect();
        for _level in 0..20 {
            let n = adj.len();
            let Some(comm) = louvain_local_moves(&adj) else {
                break;
            };
            let (comm, nc) = renumber_first_seen(&comm);
            for m in &mut membership {
                *m = comm[*m];
            }
            adj = louvain_aggregate(&adj, &comm, nc);
            if nc == n {
                break;
            }
        }
        renumber_first_seen(&membership).0
    }

    /// Betweenness centrality (Brandes, unweighted). Sampled above 2000 nodes.
    pub fn betweenness(&self) -> Vec<f64> {
        let n = self.n;
        let mut cb = vec![0.0; n];
        let sources: Vec<usize> = if n > 2000 {
            (0..n).step_by(n / 500 + 1).collect()
        } else {
            (0..n).collect()
        };
        for &s in &sources {
            let mut stack = vec![];
            let mut pred: Vec<Vec<usize>> = vec![vec![]; n];
            let mut sigma = vec![0.0f64; n];
            let mut dist = vec![-1i64; n];
            sigma[s] = 1.0;
            dist[s] = 0;
            let mut q = std::collections::VecDeque::new();
            q.push_back(s);
            while let Some(v) = q.pop_front() {
                stack.push(v);
                for &(w, _) in &self.adj[v] {
                    if dist[w] < 0 {
                        dist[w] = dist[v] + 1;
                        q.push_back(w);
                    }
                    if dist[w] == dist[v] + 1 {
                        sigma[w] += sigma[v];
                        pred[w].push(v);
                    }
                }
            }
            let mut delta = vec![0.0f64; n];
            while let Some(w) = stack.pop() {
                for &v in &pred[w] {
                    delta[v] += sigma[v] / sigma[w] * (1.0 + delta[w]);
                }
                if w != s {
                    cb[w] += delta[w];
                }
            }
        }
        let scale = if sources.len() < n {
            crate::count_f64(n) / crate::count_f64(sources.len())
        } else {
            1.0
        };
        cb.iter().map(|x| x * scale / 2.0).collect()
    }
}

/// One Louvain level: move each node to the neighbouring community with the best
/// modularity gain until nothing moves. `None` when the graph is empty or
/// edgeless, or when no node moved at all.
fn louvain_local_moves(adj: &[Vec<(usize, f64)>]) -> Option<Vec<usize>> {
    let n = adj.len();
    if n == 0 {
        return None;
    }
    // self loops are stored as (i, i, w) and count twice in the strength
    let k: Vec<f64> = (0..n)
        .map(|i| {
            adj[i]
                .iter()
                .map(|(j, w)| if *j == i { 2.0 * w } else { *w })
                .sum()
        })
        .collect();
    let two_m: f64 = k.iter().sum();
    if two_m == 0.0 {
        return None;
    }
    let mut comm: Vec<usize> = (0..n).collect();
    let mut tot: Vec<f64> = k.clone();
    let mut moved_any = false;
    for _pass in 0..50 {
        let mut moved = false;
        for i in 0..n {
            let ci = comm[i];
            tot[ci] -= k[i];
            let mut k_in: BTreeMap<usize, f64> = BTreeMap::new();
            for &(j, w) in &adj[i] {
                if j != i {
                    *k_in.entry(comm[j]).or_default() += w;
                }
            }
            let gain = |c: usize| k_in.get(&c).copied().unwrap_or(0.0) - tot[c] * k[i] / two_m;
            let mut best = ci;
            let mut best_gain = gain(ci);
            for &c in k_in.keys() {
                let g = gain(c);
                if g > best_gain + 1e-12 {
                    best_gain = g;
                    best = c;
                }
            }
            tot[best] += k[i];
            if best != ci {
                comm[i] = best;
                moved = true;
                moved_any = true;
            }
        }
        if !moved {
            break;
        }
    }
    moved_any.then_some(comm)
}

/// Relabel ids as 0, 1, 2, … in order of first appearance; also returns the number of labels.
fn renumber_first_seen(ids: &[usize]) -> (Vec<usize>, usize) {
    let mut renum: HashMap<usize, usize> = HashMap::new();
    let out = ids
        .iter()
        .map(|id| {
            let next = renum.len();
            *renum.entry(*id).or_insert(next)
        })
        .collect();
    (out, renum.len())
}

/// Collapse each community into one node; internal edges become a self loop.
fn louvain_aggregate(
    adj: &[Vec<(usize, f64)>],
    comm: &[usize],
    nc: usize,
) -> Vec<Vec<(usize, f64)>> {
    let mut agg: Vec<HashMap<usize, f64>> = vec![HashMap::new(); nc];
    for (i, edges) in adj.iter().enumerate() {
        for &(j, w) in edges {
            let (a, b) = (comm[i], comm[j]);
            if i == j {
                *agg[a].entry(a).or_default() += w;
            } else if i < j {
                if a == b {
                    *agg[a].entry(a).or_default() += w;
                } else {
                    *agg[a].entry(b).or_default() += w;
                    *agg[b].entry(a).or_default() += w;
                }
            }
        }
    }
    agg.into_iter()
        .map(|m| {
            let mut v: Vec<(usize, f64)> = m.into_iter().collect();
            v.sort_by_key(|x| x.0);
            v
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct RelatedRow {
    pub node: String,
    pub label: String,
    pub score: f64,
    pub reasons: Vec<String>,
    pub description: String,
}

/// Pages related to `page`, each with why: direct links, co-citation,
/// Adamic-Adar shared neighbours, shared tags, random-walk proximity.
pub fn related(g: &Graph, vault: &Vault, page: &str, limit: usize) -> Option<Vec<RelatedRow>> {
    let net = Net::from_graph(g);
    let &focus = net.id_of.get(&page.to_lowercase())?;
    let r = Relatedness::new(g, vault, &net, focus);
    let mut rows: Vec<RelatedRow> = r
        .candidates()
        .into_iter()
        .filter_map(|c| r.row(c))
        .collect();
    rows.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.node.cmp(&b.node)));
    rows.truncate(limit);
    Some(rows)
}

/// Everything `related` knows about the focus page, to score each candidate.
struct Relatedness<'a> {
    g: &'a Graph,
    vault: &'a Vault,
    net: &'a Net,
    focus: usize,
    focus_sources: BTreeSet<String>,
    focus_tags: BTreeSet<String>,
    /// Personalised `PageRank` from the focus page.
    ppr: Vec<f64>,
    /// The highest `ppr` of any other page, to normalise proximity.
    ppr_max: f64,
    /// Relations on direct edges between the focus and each neighbour.
    direct: HashMap<usize, BTreeSet<String>>,
}

impl<'a> Relatedness<'a> {
    fn new(g: &'a Graph, vault: &'a Vault, net: &'a Net, focus: usize) -> Self {
        let ppr = net.personalised_pagerank(&[focus], 0.85);
        let ppr_max = ppr
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != focus)
            .map(|(_, v)| *v)
            .fold(0.0, f64::max);
        let mut r = Relatedness {
            g,
            vault,
            net,
            focus,
            focus_sources: BTreeSet::new(),
            focus_tags: BTreeSet::new(),
            ppr,
            ppr_max,
            direct: HashMap::new(),
        };
        r.focus_sources = r.sources_of(focus);
        r.focus_tags = r.tags_of(focus);
        r.direct = r.direct_relations();
        r
    }

    /// Sources cited by node `i` (lowercase ids).
    fn sources_of(&self, i: usize) -> BTreeSet<String> {
        let id = &self.g.nodes[i].id;
        self.g
            .edges
            .iter()
            .filter(|e| e.rel == "cites" && e.from.eq_ignore_ascii_case(id))
            .map(|e| e.to.to_lowercase())
            .collect()
    }

    fn tags_of(&self, i: usize) -> BTreeSet<String> {
        self.vault
            .resolve_page(&self.g.nodes[i].id)
            .map(|pg| pg.tags.iter().map(|t| t.to_lowercase()).collect())
            .unwrap_or_default()
    }

    fn direct_relations(&self) -> HashMap<usize, BTreeSet<String>> {
        let mut m: HashMap<usize, BTreeSet<String>> = HashMap::new();
        let focus_id = self.g.nodes[self.focus].id.to_lowercase();
        for e in &self.g.edges {
            let (from, to) = (e.from.to_lowercase(), e.to.to_lowercase());
            if from == focus_id {
                if let Some(&j) = self.net.id_of.get(&to) {
                    m.entry(j).or_default().insert(e.rel.clone());
                }
            } else if to == focus_id
                && let Some(&j) = self.net.id_of.get(&from)
            {
                m.entry(j)
                    .or_default()
                    .insert(format!("{} (inbound)", e.rel));
            }
        }
        m
    }

    /// The 2-hop neighbourhood, the top random-walk nodes, co-citers and tag sharers.
    fn candidates(&self) -> BTreeSet<usize> {
        let (net, focus) = (self.net, self.focus);
        let mut cands: BTreeSet<usize> = BTreeSet::new();
        for &(j, _) in &net.adj[focus] {
            cands.insert(j);
            cands.extend(net.adj[j].iter().map(|&(k, _)| k));
        }
        let mut order: Vec<usize> = (0..net.n).collect();
        order.sort_by(|x, y| self.ppr[*y].total_cmp(&self.ppr[*x]));
        cands.extend(order.iter().take(30).copied());
        for i in 0..net.n {
            if i != focus
                && (!self.focus_sources.is_empty()
                    && !self.sources_of(i).is_disjoint(&self.focus_sources)
                    || !self.focus_tags.is_empty()
                        && !self.tags_of(i).is_disjoint(&self.focus_tags))
            {
                cands.insert(i);
            }
        }
        cands.remove(&focus);
        cands
    }

    /// Score candidate `c` with its reasons; `None` when nothing relates it.
    fn row(&self, c: usize) -> Option<RelatedRow> {
        let g = self.g;
        let mut score = 0.0;
        let mut reasons = vec![];
        if let Some(rels) = self.direct.get(&c) {
            score += crate::count_f64(rels.len());
            reasons.push(format!(
                "linked: {}",
                rels.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        let shared_src: Vec<String> = self
            .focus_sources
            .intersection(&self.sources_of(c))
            .cloned()
            .collect();
        if !shared_src.is_empty() {
            score += 0.4 * crate::count_f64(shared_src.len());
            reasons.push(format!(
                "cites {} same source{}: {}",
                shared_src.len(),
                plural(shared_src.len()),
                shared_src
                    .iter()
                    .take(3)
                    .map(|s| display_id(g, s))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        let (aa, shared) = self.net.adamic_adar(self.focus, c);
        if aa > 0.0 {
            score += 0.5 * aa;
            reasons.push(format!(
                "{} shared neighbour{} (Adamic-Adar {:.2}): {}",
                shared.len(),
                plural(shared.len()),
                aa,
                shared
                    .iter()
                    .take(3)
                    .map(|&z| g.nodes[z].id.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        let shared_tags: Vec<String> = self
            .focus_tags
            .intersection(&self.tags_of(c))
            .cloned()
            .collect();
        if !shared_tags.is_empty() {
            score += 0.2 * crate::count_f64(shared_tags.len());
            reasons.push(format!("shares tags {}", shared_tags.join(", ")));
        }
        if self.ppr_max > 0.0 && self.ppr[c] > 0.0 {
            let proximity = self.ppr[c] / self.ppr_max;
            if proximity > 0.05 {
                score += proximity;
                reasons.push(format!("random-walk proximity {proximity:.2}"));
            }
        }
        (score > 0.0).then(|| RelatedRow {
            node: g.nodes[c].id.clone(),
            label: g.nodes[c].label.clone(),
            score: (score * 100.0).round() / 100.0,
            reasons,
            description: g.nodes[c].description.clone(),
        })
    }
}

/// "s" unless `n` is 1.
fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn display_id(g: &Graph, lower: &str) -> String {
    g.node(lower)
        .map_or_else(|| lower.to_string(), |n| n.id.clone())
}

#[derive(Debug, Clone, Serialize)]
pub struct HubRow {
    pub node: String,
    pub label: String,
    pub pagerank: f64,
    pub degree: usize,
    pub betweenness: f64,
}

pub fn hubs(g: &Graph, limit: usize) -> Vec<HubRow> {
    let net = Net::from_graph(g);
    let pr = net.pagerank();
    let bc = net.betweenness();
    let mut rows: Vec<HubRow> = (0..net.n)
        .map(|i| HubRow {
            node: g.nodes[i].id.clone(),
            label: g.nodes[i].label.clone(),
            pagerank: (pr[i] * 10000.0).round() / 10000.0,
            degree: net.degree(i),
            betweenness: bc[i].round(),
        })
        .collect();
    rows.sort_by(|a, b| b.pagerank.total_cmp(&a.pagerank).then(a.node.cmp(&b.node)));
    rows.truncate(limit);
    rows
}

#[derive(Debug, Clone, Serialize)]
pub struct Community {
    pub id: usize,
    pub size: usize,
    pub members: Vec<String>,
    pub types: BTreeMap<String, usize>,
    pub hub: Option<String>,
    pub sources_cited: usize,
}

pub fn communities(g: &Graph, min_size: usize) -> Vec<Community> {
    let net = Net::from_graph(g);
    let memb = net.louvain();
    let pr = net.pagerank();
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, c) in memb.iter().enumerate() {
        groups.entry(*c).or_default().push(i);
    }
    let mut out = vec![];
    for (_, mut members) in groups {
        if members.len() < min_size {
            continue;
        }
        members.sort_by(|a, b| pr[*b].total_cmp(&pr[*a]));
        let mut types: BTreeMap<String, usize> = BTreeMap::new();
        let mut sources: BTreeSet<String> = BTreeSet::new();
        let mut hub = None;
        let set: HashSet<usize> = members.iter().copied().collect();
        for &i in &members {
            let n = &g.nodes[i];
            *types.entry(n.label.clone()).or_default() += 1;
            if n.label == "source" {
                sources.insert(n.id.to_lowercase());
            }
            if hub.is_none() && HUB_TYPES.contains(&n.label.as_str()) {
                hub = Some(n.id.clone());
            }
        }
        for e in &g.edges {
            if e.rel == "cites"
                && let Some(&f) = net.id_of.get(&e.from.to_lowercase())
                && set.contains(&f)
            {
                sources.insert(e.to.to_lowercase());
            }
        }
        out.push(Community {
            id: 0,
            size: members.len(),
            members: members
                .iter()
                .take(8)
                .map(|&i| g.nodes[i].id.clone())
                .collect(),
            types,
            hub,
            sources_cited: sources.len(),
        });
    }
    out.sort_by(|a, b| b.size.cmp(&a.size).then(a.members.cmp(&b.members)));
    for (i, c) in out.iter_mut().enumerate() {
        c.id = i + 1;
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct Wanted {
    pub target: String,
    pub referrers: Vec<String>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Gaps {
    pub wanted: Vec<Wanted>,
    pub orphans: Vec<String>,
    pub dead_ends: Vec<String>,
    pub uncited_sources: Vec<String>,
    pub communities_without_hub: Vec<Community>,
    pub single_source_communities: Vec<Community>,
    pub bridges: Vec<HubRow>,
}

/// Unresolved link targets, ranked by how many distinct pages want them.
fn wanted_pages(vault: &Vault) -> Vec<Wanted> {
    let mut wanted: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    for p in &vault.pages {
        if p.parse_error.is_some() || p.rel == "index.md" || p.rel.starts_with("indexes/") {
            continue;
        }
        for t in p.all_link_targets() {
            if matches!(vault.resolve(&t), Resolution::Missing) {
                let e = wanted
                    .entry(t.to_lowercase())
                    .or_insert_with(|| (t.clone(), BTreeSet::new()));
                e.1.insert(p.stem.clone());
            }
        }
    }
    let mut wanted: Vec<Wanted> = wanted
        .into_values()
        .map(|(t, refs)| Wanted {
            suggestion: vault.suggest(&t, 1).into_iter().next().map(|(s, _)| s),
            target: t,
            referrers: refs.into_iter().collect(),
        })
        .collect();
    wanted.sort_by(|a, b| {
        b.referrers
            .len()
            .cmp(&a.referrers.len())
            .then(a.target.cmp(&b.target))
    });
    wanted
}

/// The ten pages with the highest betweenness: they connect otherwise separate areas.
fn bridges(g: &Graph) -> Vec<HubRow> {
    let mut bridges = hubs(g, usize::MAX);
    bridges.sort_by(|a, b| {
        b.betweenness
            .total_cmp(&a.betweenness)
            .then(a.node.cmp(&b.node))
    });
    bridges.retain(|b| b.betweenness > 0.0);
    bridges.truncate(10);
    bridges
}

pub fn gaps(g: &Graph, vault: &Vault) -> Gaps {
    let net = Net::from_graph(g);
    let mut indeg = vec![0usize; net.n];
    let mut outdeg = vec![0usize; net.n];
    for e in &g.edges {
        if let (Some(&a), Some(&b)) = (
            net.id_of.get(&e.from.to_lowercase()),
            net.id_of.get(&e.to.to_lowercase()),
        ) {
            outdeg[a] += 1;
            indeg[b] += 1;
        }
    }
    let archived = |i: usize| {
        vault
            .resolve_page(&g.nodes[i].id)
            .is_some_and(super::vault::Page::is_archived)
    };
    let is_knowledge = |i: usize| {
        SYNTHESIS.contains(&g.nodes[i].label.as_str())
            || vault.resolve_page(&g.nodes[i].id).is_some_and(|p| {
                !p.list("sources").is_empty()
                    && !["source", "journal", "capture"].contains(&p.ptype.as_str())
            })
    };
    let ids = |keep: &dyn Fn(usize) -> bool| -> Vec<String> {
        (0..net.n)
            .filter(|&i| keep(i))
            .map(|i| g.nodes[i].id.clone())
            .collect()
    };
    let orphans = ids(&|i| indeg[i] == 0 && is_knowledge(i) && !archived(i));
    let dead_ends = ids(&|i| outdeg[i] == 0 && is_knowledge(i) && !archived(i));
    let uncited_sources = ids(&|i| {
        g.nodes[i].label == "source" && indeg[i] == 0 && g.nodes[i].status != "superseded"
    });

    let comms = communities(g, 3);
    let communities_without_hub: Vec<Community> =
        comms.iter().filter(|c| c.hub.is_none()).cloned().collect();
    let single_source_communities: Vec<Community> = comms
        .iter()
        .filter(|c| c.sources_cited <= 1 && c.types.keys().any(|t| SYNTHESIS.contains(&t.as_str())))
        .cloned()
        .collect();

    Gaps {
        wanted: wanted_pages(vault),
        orphans,
        dead_ends,
        uncited_sources,
        communities_without_hub,
        single_source_communities,
        bridges: bridges(g),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkSuggestion {
    pub page: String,
    pub path: String,
    pub line: usize,
    pub target: String,
    pub matched: String,
    pub context: String,
}

static MASK_RES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?s)```.*?```",
        r"`[^`\n]*`",
        r"(?s)<!--.*?-->",
        r"\[\[[^\]]*\]\]",
        r"!?\[[^\]]*\]\([^)]*\)",
        r"(?m)^\[\^[^\]]+\]:.*$",
        r"(?m)^#.*$",
    ]
    .iter()
    .map(|p| crate::re(p))
    .collect()
});

fn mask_keep_len(text: &str, re: &Regex) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for m in re.find_iter(text) {
        out.push_str(&text[last..m.start()]);
        out.extend(
            text[m.start()..m.end()]
                .chars()
                .map(|c| if c == '\n' { '\n' } else { ' ' }),
        );
        last = m.end();
    }
    out.push_str(&text[last..]);
    out
}

/// A compiled matcher for every linkable page name.
struct MentionMatcher {
    re: Regex,
    /// Lowercase name -> target page id.
    lookup: HashMap<String, String>,
}

impl MentionMatcher {
    /// Multi-word names match case-insensitively. A single word only counts
    /// when it is a proper noun or acronym (has a capital) and then matches
    /// exactly, so an alias like "excess" cannot flood the prose with hits.
    /// `None` when there is nothing to match.
    fn new(g: &Graph, vault: &Vault) -> Option<Self> {
        let mut multi: Vec<(String, String)> = vec![]; // (lowercase name, target id)
        let mut single: Vec<(String, String)> = vec![]; // (exact name, target id)
        for n in &g.nodes {
            if ["source", "journal", "profile", "preferences", "glossary"]
                .contains(&n.label.as_str())
            {
                continue;
            }
            let Some(p) = vault.resolve_page(&n.id) else {
                continue;
            };
            for name in [&p.stem, &p.title].into_iter().chain(&p.aliases) {
                let name = name.trim();
                if name.chars().count() < 4 || !name.chars().any(char::is_alphabetic) {
                    continue;
                }
                if name.contains(char::is_whitespace) {
                    multi.push((name.to_lowercase(), n.id.clone()));
                } else if name.chars().any(char::is_uppercase) {
                    single.push((name.to_string(), n.id.clone()));
                }
            }
        }
        for v in [&mut multi, &mut single] {
            v.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(&b.0)));
            v.dedup_by(|a, b| a.0 == b.0);
        }
        let alt = |v: &[(String, String)]| {
            v.iter()
                .map(|(n, _)| regex::escape(n))
                .collect::<Vec<_>>()
                .join("|")
        };
        let pattern = match (multi.is_empty(), single.is_empty()) {
            (true, true) => return None,
            (false, false) => format!(r"\b(?:(?i:{})|(?:{}))\b", alt(&multi), alt(&single)),
            (false, true) => format!(r"(?i)\b(?:{})\b", alt(&multi)),
            (true, false) => format!(r"\b(?:{})\b", alt(&single)),
        };
        let re = Regex::new(&pattern).ok()?;
        let lookup = multi
            .into_iter()
            .chain(single.into_iter().map(|(n, t)| (n.to_lowercase(), t)))
            .collect();
        Some(MentionMatcher { re, lookup })
    }
}

/// Whether `p` is a page whose prose should be scanned for mentions.
fn scans_for_mentions(g: &Graph, p: &Page, only_page: Option<&str>) -> bool {
    if p.parse_error.is_some()
        || g.node(&p.stem).is_none()
        || ["source", "journal", "capture"].contains(&p.ptype.as_str())
    {
        return false;
    }
    only_page
        .is_none_or(|only| p.stem.eq_ignore_ascii_case(only) || p.rel.eq_ignore_ascii_case(only))
}

/// Unlinked mentions: occurrences of another page's title or alias in a
/// page's prose that are not already links, with the line to put the link
/// on. The highest-yield "missing cross-reference" signal.
pub fn suggest_links(
    g: &Graph,
    vault: &Vault,
    only_page: Option<&str>,
    limit: usize,
) -> Vec<LinkSuggestion> {
    let Some(matcher) = MentionMatcher::new(g, vault) else {
        return vec![];
    };
    let mut out = vec![];
    for p in vault
        .pages
        .iter()
        .filter(|p| scans_for_mentions(g, p, only_page))
    {
        let already: HashSet<String> = p
            .all_link_targets()
            .iter()
            .filter_map(|t| vault.resolve_page(t).map(|q| q.stem.to_lowercase()))
            .collect();
        let mut masked = p.doc.body.clone();
        for r in MASK_RES.iter() {
            masked = mask_keep_len(&masked, r);
        }
        let mut seen: HashSet<String> = HashSet::new();
        for m in matcher.re.find_iter(&masked) {
            let Some(target) = matcher.lookup.get(&m.as_str().to_lowercase()) else {
                continue;
            };
            let tl = target.to_lowercase();
            if tl == p.stem.to_lowercase() || already.contains(&tl) || !seen.insert(tl) {
                continue;
            }
            let line_idx = masked[..m.start()].matches('\n').count();
            let context = p
                .doc
                .body
                .lines()
                .nth(line_idx)
                .unwrap_or("")
                .trim()
                .chars()
                .take(160)
                .collect::<String>();
            out.push(LinkSuggestion {
                page: p.stem.clone(),
                path: p.rel.clone(),
                line: p.doc.body_line_offset + line_idx + 1,
                target: target.clone(),
                matched: m.as_str().to_string(),
                context,
            });
            if out.len() >= limit {
                return out;
            }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct ConflictCandidate {
    pub a: String,
    pub b: String,
    pub ptype: String,
    pub score: f64,
    pub reasons: Vec<String>,
    pub updated: (String, String),
}

/// Pages that can contradict each other: knowledge pages, not navigation or raw material.
fn conflict_eligible(vault: &Vault) -> Vec<usize> {
    vault
        .pages
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.parse_error.is_none()
                && !p.is_archived()
                && (SYNTHESIS.contains(&p.ptype.as_str()) || !p.list("sources").is_empty())
                && !["source", "journal", "capture", "topic", "project", "area"]
                    .contains(&p.ptype.as_str())
        })
        .map(|(i, _)| i)
        .collect()
}

/// Same-type page pairs that share a source or a tag.
fn conflict_pairs(vault: &Vault, eligible: &[usize]) -> BTreeSet<(usize, usize)> {
    let mut by_source: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_tag: HashMap<String, Vec<usize>> = HashMap::new();
    for &i in eligible {
        let p = &vault.pages[i];
        for s in p.list("sources") {
            if let Some(q) = vault.resolve_page(&s) {
                by_source.entry(q.stem.to_lowercase()).or_default().push(i);
            }
        }
        for t in &p.tags {
            by_tag.entry(t.to_lowercase()).or_default().push(i);
        }
    }
    let mut pairs: BTreeSet<(usize, usize)> = BTreeSet::new();
    for v in by_source.values().chain(by_tag.values()) {
        for (x, &first) in v.iter().enumerate() {
            for &second in &v[x + 1..] {
                let (a, b) = (first.min(second), first.max(second));
                if a != b && vault.pages[a].ptype == vault.pages[b].ptype {
                    pairs.insert((a, b));
                }
            }
        }
    }
    pairs
}

/// Whether `p` already records a contradiction with, or supersession by, the page `other`.
fn relation_recorded(vault: &Vault, p: &Page, other: &str) -> bool {
    p.fm_links.iter().any(|(f, t)| {
        f == "contradictions"
            && vault
                .resolve_page(t)
                .is_some_and(|q| q.stem.eq_ignore_ascii_case(other))
    }) || p
        .get("superseded_by")
        .is_some_and(|s| s.to_lowercase().contains(&other.to_lowercase()))
}

/// Description words longer than three characters, lowercased.
fn description_tokens(p: &Page) -> HashSet<String> {
    p.get("description")
        .unwrap_or_default()
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 3)
        .map(ToString::to_string)
        .collect()
}

fn resolved_sources(vault: &Vault, p: &Page) -> HashSet<String> {
    p.list("sources")
        .iter()
        .filter_map(|s| vault.resolve_page(s).map(|q| q.stem.to_lowercase()))
        .collect()
}

/// Score a pair by shared sources, title similarity, description overlap and shared tags.
fn score_conflict(vault: &Vault, pa: &Page, pb: &Page) -> Option<ConflictCandidate> {
    let mut score = 0.0;
    let mut reasons = vec![];
    let shared = resolved_sources(vault, pa)
        .intersection(&resolved_sources(vault, pb))
        .count();
    if shared > 0 {
        score += 1.0 + 0.3 * crate::count_f64(shared);
        reasons.push(format!("{shared} shared source{}", plural(shared)));
    }
    let jw = strsim::jaro_winkler(&pa.title.to_lowercase(), &pb.title.to_lowercase());
    if jw > 0.85 {
        score += 1.5;
        reasons.push(format!("titles nearly identical ({jw:.2})"));
    }
    let (ta, tb) = (description_tokens(pa), description_tokens(pb));
    if !ta.is_empty() && !tb.is_empty() {
        let overlap = crate::count_f64(ta.intersection(&tb).count())
            / crate::count_f64(ta.union(&tb).count());
        if overlap >= 0.3 {
            score += overlap * 2.0;
            reasons.push(format!("descriptions overlap ({:.0}%)", overlap * 100.0));
        }
    }
    let shared_tags = pa.tags.iter().filter(|t| pb.tags.contains(t)).count();
    if shared_tags >= 2 {
        score += 0.2 * crate::count_f64(shared_tags);
        reasons.push(format!("{shared_tags} shared tags"));
    }
    let (ua, ub) = (
        pa.get("updated").unwrap_or_default(),
        pb.get("updated").unwrap_or_default(),
    );
    if !ua.is_empty() && !ub.is_empty() && ua != ub && score > 0.0 {
        reasons.push(format!("updated on different dates ({ua} vs {ub})"));
    }
    (score >= 1.3).then(|| ConflictCandidate {
        a: pa.stem.clone(),
        b: pb.stem.clone(),
        ptype: pa.ptype.clone(),
        score: (score * 100.0).round() / 100.0,
        reasons,
        updated: (ua, ub),
    })
}

/// Page pairs worth comparing for contradiction or staleness: same type,
/// overlapping evidence or subject, no recorded contradiction between them.
pub fn conflicts(vault: &Vault, limit: usize) -> Vec<ConflictCandidate> {
    let eligible = conflict_eligible(vault);
    let mut out: Vec<ConflictCandidate> = conflict_pairs(vault, &eligible)
        .into_iter()
        .filter_map(|(a, b)| {
            let (pa, pb) = (&vault.pages[a], &vault.pages[b]);
            if relation_recorded(vault, pa, &pb.stem) || relation_recorded(vault, pb, &pa.stem) {
                return None;
            }
            score_conflict(vault, pa, pb)
        })
        .collect();
    out.sort_by(|x, y| y.score.total_cmp(&x.score).then(x.a.cmp(&y.a)));
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renumbering_labels_by_first_appearance() {
        let (ids, count) = renumber_first_seen(&[7, 3, 7, 9, 3]);
        assert_eq!(ids, [0, 1, 0, 2, 1]);
        assert_eq!(count, 3);
    }

    fn net(edges: &[(usize, usize)], n: usize) -> Net {
        let mut adj = vec![vec![]; n];
        for &(a, b) in edges {
            adj[a].push((b, 1.0));
            adj[b].push((a, 1.0));
        }
        Net {
            n,
            adj,
            id_of: HashMap::new(),
        }
    }

    #[test]
    fn pagerank_prefers_hub() {
        let g = net(&[(0, 1), (0, 2), (0, 3), (3, 4)], 5);
        let pr = g.pagerank();
        assert!(pr[0] > pr[1] && pr[0] > pr[4]);
        assert!((pr.iter().sum::<f64>() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn louvain_splits_two_cliques() {
        // two triangles joined by one edge
        let g = net(&[(0, 1), (1, 2), (0, 2), (3, 4), (4, 5), (3, 5), (2, 3)], 6);
        let m = g.louvain();
        assert_eq!(m[0], m[1]);
        assert_eq!(m[1], m[2]);
        assert_eq!(m[3], m[4]);
        assert_eq!(m[4], m[5]);
        assert_ne!(m[0], m[3]);
    }

    #[test]
    fn betweenness_finds_bridge() {
        let g = net(&[(0, 1), (1, 2), (0, 2), (3, 4), (4, 5), (3, 5), (2, 3)], 6);
        let b = g.betweenness();
        assert!(b[2] > b[0] && b[3] > b[4]);
    }

    #[test]
    fn adamic_adar_counts_shared() {
        let g = net(&[(0, 2), (1, 2), (0, 3), (1, 3), (3, 4)], 5);
        let (s, shared) = g.adamic_adar(0, 1);
        assert_eq!(shared, vec![2, 3]);
        assert!(s > 1.0 / (3f64).ln() && s < 2.0 / (2f64).ln() + 0.01);
    }

    #[test]
    fn masking_keeps_length() {
        let t = "a [[Link]] b `code` c\n[^1]: def";
        let mut m = t.to_string();
        for r in MASK_RES.iter() {
            m = mask_keep_len(&m, r);
        }
        assert_eq!(m.len(), t.len());
        assert!(!m.contains("Link") && !m.contains("code") && !m.contains("def"));
        assert!(m.contains('\n'));
    }
}
