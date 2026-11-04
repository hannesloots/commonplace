//! Finding pages: full-text search, frontmatter queries, traceability tables.

use super::{Ctx, print_json};
use crate::cli::{QueryArgs, SearchArgs, TraceArgs};
use anyhow::Result;
use commonplace_core::Error;
use commonplace_core::{frontmatter, ops, query, search, vault::Page};

pub(crate) fn search(ctx: &Ctx, a: SearchArgs) -> Result<i32> {
    let v = ctx.open()?;
    let q = a.query.join(" ");
    let hits = search::search(
        &v,
        &q,
        &search::SearchOptions {
            scope: a.scope,
            include_archive: a.include_archive,
            include_text_layer: a.text,
            ptype: a.ptype,
            limit: a.limit,
        },
    );
    if ctx.json {
        print_json(&hits)?;
        return Ok(0);
    }
    for h in &hits {
        println!(
            "{:>6.2}  {} [{}]{}\n        {}",
            h.score,
            h.rel,
            h.ptype,
            h.line.map(|l| format!(" L{l}")).unwrap_or_default(),
            h.snippet
        );
    }
    if hits.is_empty() {
        println!("no hits");
    }
    Ok(0)
}

fn field_text(p: &Page, field: &str) -> String {
    query::field_value(p, field)
        .map(|x| frontmatter::scalar_to_string(&x))
        .unwrap_or_default()
}

pub(crate) fn query(ctx: &Ctx, a: QueryArgs) -> Result<i32> {
    let v = ctx.open()?;
    let expr = query::parse(&a.r#where)?;
    let scope = a.scope.as_deref().map(|s| s.trim_end_matches('/'));
    let pages: Vec<&Page> = v
        .pages
        .iter()
        .filter(|p| {
            (a.include_archive || !p.is_archived())
                && scope.is_none_or(|s| p.rel.starts_with(s))
                && query::eval(&expr, p)
        })
        .collect();
    let fields: Vec<String> = if a.fields.is_empty() {
        vec!["type".into(), "status".into(), "description".into()]
    } else {
        a.fields
    };
    if ctx.json || a.format == "json" {
        let rows: Vec<serde_json::Value> = pages
            .iter()
            .map(|p| {
                let mut m = serde_json::Map::new();
                m.insert("page".into(), p.stem.clone().into());
                m.insert("path".into(), p.rel.clone().into());
                for f in &fields {
                    m.insert(f.clone(), field_text(p, f).into());
                }
                serde_json::Value::Object(m)
            })
            .collect();
        print_json(&rows)?;
    } else if a.format == "list" {
        for p in &pages {
            println!("- [[{}]]", p.stem);
        }
    } else {
        println!("| page | {} |", fields.join(" | "));
        println!("|---|{}", "---|".repeat(fields.len()));
        for p in &pages {
            let cells: Vec<String> = fields
                .iter()
                .map(|f| field_text(p, f).replace('|', "\\|"))
                .collect();
            println!("| [[{}]] | {} |", p.stem, cells.join(" | "));
        }
        eprintln!("{} pages", pages.len());
    }
    Ok(0)
}

pub(crate) fn trace(ctx: &Ctx, a: TraceArgs) -> Result<i32> {
    let v = ctx.open()?;
    let packs = super::load_packs(&v)?;
    let chain: Vec<String> = match a.name {
        Some(n) => ops::pack_trace(&packs, &n)
            .ok_or_else(|| {
                Error::invalid_input(format!(
                    "no pack trace named '{n}' (available: {})",
                    packs
                        .iter()
                        .flat_map(|p| p.traces.iter().map(|t| t.name.as_str()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?
            .chain
            .clone(),
        None => a.chain,
    };
    let (header, rows) = ops::trace(&v, &chain)?;
    if ctx.json || a.format == "json" {
        let cells: Vec<&Vec<String>> = rows.iter().map(|r| &r.cells).collect();
        print_json(&serde_json::json!({"columns": header, "rows": cells}))?;
    } else if a.format == "csv" {
        println!("{}", header.join(","));
        for r in rows {
            println!("{}", r.cells.join(","));
        }
    } else {
        println!("| {} |", header.join(" | "));
        println!("|{}", "---|".repeat(header.len()));
        let gaps = rows
            .iter()
            .filter(|r| r.cells.iter().any(|c| c == "—"))
            .count();
        for r in &rows {
            let cells: Vec<String> = r
                .cells
                .iter()
                .map(|c| {
                    if c == "—" {
                        c.clone()
                    } else {
                        format!("[[{c}]]")
                    }
                })
                .collect();
            println!("| {} |", cells.join(" | "));
        }
        eprintln!("{} rows, {} with gaps", rows.len(), gaps);
    }
    Ok(0)
}
