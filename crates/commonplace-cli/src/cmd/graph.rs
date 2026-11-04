//! The commonplace as a typed graph: statistics, ontology, walks, analysis, exports.

use super::{Ctx, print_json, wikilinks};
use crate::cli::{GraphCmd, GraphExportArgs, PathArgs, WalkArgs};
use anyhow::Result;
use commonplace_core::Error;
use commonplace_core::analysis;
use commonplace_core::fsutil;
use commonplace_core::graph::{self, Graph};
use commonplace_core::vault::Vault;
use std::collections::HashSet;

pub(crate) fn graph(ctx: &Ctx, cmd: GraphCmd) -> Result<i32> {
    let v = ctx.open()?;
    match cmd {
        GraphCmd::Stats { mentions } => stats(ctx, &graph::build(&v, mentions)),
        GraphCmd::Schema { mentions } => schema(ctx, &graph::build(&v, mentions)),
        GraphCmd::Walk(a) => walk(ctx, &v, &a),
        GraphCmd::Path(a) => path(ctx, &v, &a),
        GraphCmd::Related {
            page,
            limit,
            typed_only,
        } => related(ctx, &v, &page, limit, typed_only),
        GraphCmd::Hubs { limit, typed_only } => hubs(ctx, &v, limit, typed_only),
        GraphCmd::Communities {
            min_size,
            typed_only,
        } => communities(ctx, &v, min_size, typed_only),
        GraphCmd::Gaps { typed_only } => gaps(ctx, &v, typed_only),
        GraphCmd::SuggestLinks { page, limit } => suggest_links(ctx, &v, page.as_deref(), limit),
        GraphCmd::Conflicts { limit } => conflicts(ctx, &v, limit),
        GraphCmd::Export(a) => export(ctx, &v, a),
    }
}

fn stats(ctx: &Ctx, g: &Graph) -> Result<i32> {
    let st = g.stats();
    if ctx.json {
        print_json(&st)?;
        return Ok(0);
    }
    println!(
        "{} nodes, {} edges, {} components (largest: {:?})",
        st.nodes, st.edges, st.components, st.largest_components
    );
    println!(
        "hubs: {}",
        st.hubs
            .iter()
            .map(|(n, d)| format!("{n} ({d})"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !st.isolated.is_empty() {
        let shown: Vec<&str> = st.isolated.iter().take(20).map(String::as_str).collect();
        println!("isolated ({}): {}", st.isolated.len(), shown.join(", "));
    }
    Ok(0)
}

fn schema(ctx: &Ctx, g: &Graph) -> Result<i32> {
    let sc = g.schema();
    if ctx.json {
        print_json(&sc)?;
        return Ok(0);
    }
    println!("Node types:");
    for (l, n) in &sc.labels {
        println!("  {l} ({n})");
    }
    println!("Relations:");
    for r in &sc.relations {
        println!("  {}", r.rel);
        for (pair, n) in &r.pairs {
            println!("      {pair} ({n})");
        }
    }
    Ok(0)
}

fn walk(ctx: &Ctx, v: &Vault, a: &WalkArgs) -> Result<i32> {
    let g = graph::build(v, a.mentions);
    if g.node(&a.page).is_none() {
        return Err(v.page_not_found(&a.page).into());
    }
    let rows = g.walk(&a.page, a.depth, &a.rel, a.direction.into());
    if ctx.json {
        print_json(&rows)?;
        return Ok(0);
    }
    for r in &rows {
        println!(
            "{} [[{}]] ({}) via {}{}",
            "  ".repeat(r.depth - 1),
            r.node,
            r.label,
            r.path,
            if r.description.is_empty() {
                String::new()
            } else {
                format!(" — {}", r.description)
            }
        );
    }
    println!("{} nodes within {} hops", rows.len(), a.depth);
    Ok(0)
}

fn path(ctx: &Ctx, v: &Vault, a: &PathArgs) -> Result<i32> {
    let g = graph::build(v, a.mentions);
    for p in [&a.from, &a.to] {
        if g.node(p).is_none() {
            return Err(v.page_not_found(p).into());
        }
    }
    let paths = g.paths(
        &a.from,
        &a.to,
        a.max_depth,
        &a.rel,
        a.direction.into(),
        a.limit,
    );
    if ctx.json {
        print_json(&paths)?;
        return Ok(i32::from(paths.is_empty()));
    }
    for p in &paths {
        let steps: Vec<String> = p
            .iter()
            .map(|x| {
                if x.starts_with('-') || x.starts_with('<') {
                    x.clone()
                } else {
                    format!("[[{x}]]")
                }
            })
            .collect();
        println!("{}", steps.join(" "));
    }
    if paths.is_empty() {
        println!(
            "no path within {} hops{}",
            a.max_depth,
            if a.mentions {
                ""
            } else {
                " (try --mentions to include plain wikilinks)"
            }
        );
    }
    Ok(i32::from(paths.is_empty()))
}

fn related(ctx: &Ctx, v: &Vault, page: &str, limit: usize, typed_only: bool) -> Result<i32> {
    let g = graph::build(v, !typed_only);
    let rows = analysis::related(&g, v, page, limit).ok_or_else(|| v.page_not_found(page))?;
    if ctx.json {
        print_json(&rows)?;
        return Ok(0);
    }
    for r in &rows {
        println!("{:>5.2}  [[{}]] ({})", r.score, r.node, r.label);
        for why in &r.reasons {
            println!("         {why}");
        }
    }
    println!("{} related pages", rows.len());
    Ok(0)
}

fn hubs(ctx: &Ctx, v: &Vault, limit: usize, typed_only: bool) -> Result<i32> {
    let g = graph::build(v, !typed_only);
    let rows = analysis::hubs(&g, limit);
    if ctx.json {
        print_json(&rows)?;
        return Ok(0);
    }
    println!("{:<8} {:<6} {:<8} page", "pagerank", "degree", "between");
    for r in &rows {
        println!(
            "{:<8.4} {:<6} {:<8} [[{}]] ({})",
            r.pagerank, r.degree, r.betweenness, r.node, r.label
        );
    }
    Ok(0)
}

fn communities(ctx: &Ctx, v: &Vault, min_size: usize, typed_only: bool) -> Result<i32> {
    let g = graph::build(v, !typed_only);
    let rows = analysis::communities(&g, min_size);
    if ctx.json {
        print_json(&rows)?;
        return Ok(0);
    }
    for c in &rows {
        println!(
            "community {} ({} pages; {}; {} sources cited; hub: {})",
            c.id,
            c.size,
            c.types
                .iter()
                .map(|(t, n)| format!("{n} {t}"))
                .collect::<Vec<_>>()
                .join(", "),
            c.sources_cited,
            c.hub.as_deref().unwrap_or("NONE")
        );
        println!("    {}", wikilinks(&c.members));
    }
    println!("{} communities of {}+ pages", rows.len(), min_size);
    Ok(0)
}

fn gaps(ctx: &Ctx, v: &Vault, typed_only: bool) -> Result<i32> {
    let g = graph::build(v, !typed_only);
    let gp = analysis::gaps(&g, v);
    if ctx.json {
        print_json(&gp)?;
        return Ok(0);
    }
    print_wanted(&gp.wanted);
    print_page_list("Orphans (no inbound links)", &gp.orphans);
    print_page_list("Dead ends (no outbound links)", &gp.dead_ends);
    print_page_list("Sources no page cites", &gp.uncited_sources);
    print_clusters(&gp);
    if !gp.bridges.is_empty() {
        println!("Bridges (pages that connect clusters; review first):");
        for b in &gp.bridges {
            println!("  [[{}]] betweenness {}", b.node, b.betweenness);
        }
    }
    let total = gp.wanted.len()
        + gp.orphans.len()
        + gp.dead_ends.len()
        + gp.uncited_sources.len()
        + gp.communities_without_hub.len()
        + gp.single_source_communities.len();
    println!("{total} gap items");
    Ok(0)
}

fn print_page_list(title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    println!("{title} ({}):", items.len());
    for i in items.iter().take(30) {
        println!("  [[{i}]]");
    }
}

fn print_wanted(wanted: &[analysis::Wanted]) {
    if wanted.is_empty() {
        return;
    }
    println!("Wanted pages (linked but missing) ({}):", wanted.len());
    for w in wanted.iter().take(30) {
        let referrers: Vec<&str> = w.referrers.iter().take(4).map(String::as_str).collect();
        println!(
            "  [[{}]] wanted by {} page{}: {}{}",
            w.target,
            w.referrers.len(),
            if w.referrers.len() == 1 { "" } else { "s" },
            referrers.join(", "),
            w.suggestion
                .as_ref()
                .map(|s| format!("  (did you mean [[{s}]]?)"))
                .unwrap_or_default()
        );
    }
}

fn print_clusters(gp: &analysis::Gaps) {
    if !gp.communities_without_hub.is_empty() {
        println!(
            "Clusters without a topic/project/area page ({}):",
            gp.communities_without_hub.len()
        );
        for c in &gp.communities_without_hub {
            println!("  {} pages: {}", c.size, wikilinks(&c.members));
        }
    }
    if !gp.single_source_communities.is_empty() {
        println!(
            "Clusters resting on one source or none ({}):",
            gp.single_source_communities.len()
        );
        for c in &gp.single_source_communities {
            println!(
                "  {} pages, {} source(s): {}",
                c.size,
                c.sources_cited,
                wikilinks(c.members.iter().take(5))
            );
        }
    }
}

fn suggest_links(ctx: &Ctx, v: &Vault, page: Option<&str>, limit: usize) -> Result<i32> {
    let g = graph::build(v, true);
    let rows = analysis::suggest_links(&g, v, page, limit);
    if ctx.json {
        print_json(&rows)?;
        return Ok(0);
    }
    for r in &rows {
        println!(
            "{}:{}  \"{}\" → [[{}]]",
            r.path, r.line, r.matched, r.target
        );
        println!("    {}", r.context);
    }
    println!("{} unlinked mentions", rows.len());
    Ok(0)
}

fn conflicts(ctx: &Ctx, v: &Vault, limit: usize) -> Result<i32> {
    let rows = analysis::conflicts(v, limit);
    if ctx.json {
        print_json(&rows)?;
        return Ok(0);
    }
    for r in &rows {
        println!("{:>5.2}  [[{}]] vs [[{}]] ({})", r.score, r.a, r.b, r.ptype);
        println!("         {}", r.reasons.join("; "));
    }
    println!(
        "{} candidate pairs (the tool nominates; you judge)",
        rows.len()
    );
    Ok(0)
}

fn export(ctx: &Ctx, v: &Vault, a: GraphExportArgs) -> Result<i32> {
    let g = graph::build(v, a.mentions);
    let text = match a.format.as_str() {
        "cypher" => g.to_cypher(),
        "graphml" => g.to_graphml(),
        "dot" => g.to_dot(),
        "json" => serde_json::to_string_pretty(&g)?,
        "mermaid" => {
            let ids = a.around.as_ref().map(|around| {
                let mut set: HashSet<String> = g
                    .walk(around, a.depth, &[], graph::Direction::Both)
                    .into_iter()
                    .map(|r| r.node.to_lowercase())
                    .collect();
                set.insert(around.to_lowercase());
                set
            });
            g.to_mermaid(ids.as_ref())
        }
        other => {
            return Err(Error::invalid_input(format!(
                "unknown format '{other}' (cypher|graphml|mermaid|dot|json)"
            ))
            .into());
        }
    };
    match a.out {
        Some(o) => {
            fsutil::write_atomic(&o, text.as_bytes())?;
            if !ctx.json {
                println!(
                    "wrote {} ({} nodes, {} edges)",
                    o.display(),
                    g.nodes.len(),
                    g.edges.len()
                );
            }
        }
        None => print!("{text}"),
    }
    Ok(0)
}
