//! The source layer: ingest and the `source` subcommands.

use super::{Ctx, print_json};
use crate::cli::{IngestArgs, SourceCmd};
use anyhow::Result;
use commonplace_core::Error;
use commonplace_core::{cite, config, manifest::Manifest, sources, vault::Vault};
use serde::Serialize;
use std::path::Path;

pub(crate) fn ingest(ctx: &Ctx, a: IngestArgs) -> Result<i32> {
    let root = ctx.find_root()?;
    let cfg = config::Config::load(&root)?;
    if a.files.is_empty() {
        return Err(Error::invalid_input("give at least one file").into());
    }
    let opts = sources::IngestOptions {
        // a title only makes sense for a single file
        title: if a.files.len() == 1 { a.title } else { None },
        date: a.date,
        kind: a.kind,
        origin: a.origin,
        project: a.project,
        confidentiality: a.confidentiality,
        authority: a.authority,
        converter: a.converter,
        move_file: a.r#move,
        force: a.force,
    };
    let mut reports = vec![];
    for f in &a.files {
        let rep = sources::ingest(&root, &cfg, f, &opts)?;
        if !ctx.json {
            print_ingest(f, &rep);
        }
        reports.push(rep);
    }
    if ctx.json {
        print_json(&reports)?;
    } else {
        println!(
            "Next: read the text layer, write the note's TL;DR / summary / key quotes, set status: summarised, then integrate into concept and project pages."
        );
    }
    Ok(0)
}

fn print_ingest(file: &Path, rep: &sources::IngestReport) {
    if let Some(d) = &rep.duplicate_of {
        println!("{}: already ingested as [[{d}]]", file.display());
    } else {
        println!(
            "ingested [[{}]]\n  raw:  {}\n  text: {}\n  note: {}\n  converter: {} (fidelity {}{})",
            rep.id,
            rep.raw,
            rep.text.as_deref().unwrap_or("(none — needs conversion)"),
            rep.note,
            rep.converter,
            rep.fidelity,
            rep.pages
                .map(|p| format!(", {p} pages"))
                .unwrap_or_default()
        );
    }
    for w in &rep.warnings {
        println!("  warning: {w}");
    }
}

pub(crate) fn source(ctx: &Ctx, cmd: SourceCmd) -> Result<i32> {
    let root = ctx.find_root()?;
    match cmd {
        SourceCmd::Verify => verify(ctx, &root),
        SourceCmd::AttachText {
            id,
            converter,
            fidelity,
        } => {
            let rel = sources::attach_text(&root, &id, &converter, &fidelity)?;
            if ctx.json {
                print_json(&serde_json::json!({"id": id, "text": rel}))?;
            } else {
                println!("attached {rel} to [[{id}]] (converter {converter}, fidelity {fidelity})");
            }
            Ok(0)
        }
        SourceCmd::List => list(ctx, &root),
        SourceCmd::Relink { id, all } => relink(ctx, &root, id, all),
        SourceCmd::Show { id, text, at } => show(&root, &id, text, at.as_deref()),
    }
}

fn verify(ctx: &Ctx, root: &Path) -> Result<i32> {
    let res = sources::verify_integrity(root)?;
    let bad = res.iter().any(|r| r.raw_ok != Some(true));
    if ctx.json {
        print_json(&res)?;
    } else {
        for r in &res {
            let tag = if r.message == "ok" { "ok   " } else { "FAIL " };
            println!("{tag} {}: {}", r.id, r.message);
        }
        println!("{} sources checked", res.len());
    }
    Ok(if bad { 3 } else { 0 })
}

/// One row of `source list`. Fields are in alphabetical order to keep the JSON
/// key order stable.
#[derive(Serialize)]
struct SourceRow<'a> {
    confidentiality: String,
    fidelity: String,
    id: &'a str,
    kind: String,
    status: String,
    title: &'a str,
}

fn list(ctx: &Ctx, root: &Path) -> Result<i32> {
    let v = Vault::open(root)?;
    let rows: Vec<SourceRow<'_>> = v
        .pages_of_type("source")
        .map(|p| SourceRow {
            confidentiality: p.get("confidentiality").unwrap_or_default(),
            fidelity: p.get("fidelity").unwrap_or_default(),
            id: &p.stem,
            kind: p.get("kind").unwrap_or_default(),
            status: p.get("status").unwrap_or_default(),
            title: &p.title,
        })
        .collect();
    if ctx.json {
        print_json(&rows)?;
    } else {
        for r in &rows {
            println!("{:<12} {:<10} {:<8} {}", r.status, r.kind, r.fidelity, r.id);
        }
    }
    Ok(0)
}

fn relink(ctx: &Ctx, root: &Path, id: Option<String>, all: bool) -> Result<i32> {
    let ids: Vec<String> = if all {
        // a BTreeMap: keys come out sorted
        Manifest::load(root)?.sources.into_keys().collect()
    } else {
        vec![id.ok_or_else(|| Error::invalid_input("give a source id or --all"))?]
    };
    let reps = ids
        .iter()
        .map(|id| sources::relink(root, id))
        .collect::<commonplace_core::Result<Vec<_>>>()?;
    if ctx.json {
        print_json(&reps)?;
        return Ok(0);
    }
    for r in &reps {
        match &r.skipped {
            Some(s) => println!("skip  {}: {s}", r.id),
            None if r.links_rewritten == 0 => println!("clean {}", r.id),
            None => println!(
                "ok    {}: {} links rewritten in {}",
                r.id,
                r.links_rewritten,
                r.files_changed.join(", ")
            ),
        }
    }
    let total: usize = reps.iter().map(|r| r.links_rewritten).sum();
    println!(
        "{} sources, {total} links rewritten; run `commonplace cite verify` to confirm quotes still match",
        reps.len()
    );
    Ok(0)
}

fn show(root: &Path, id: &str, text: bool, at: Option<&str>) -> Result<i32> {
    if !text && at.is_none() {
        let p = root.join(sources::NOTES_DIR).join(format!("{id}.md"));
        let note = std::fs::read_to_string(&p).map_err(|_| Error::source_not_found(id))?;
        print!("{note}");
        return Ok(0);
    }
    let tl = sources::load_text_layer(root, id)?
        .ok_or_else(|| Error::text_layer_missing(format!("no text layer for '{id}'")))?;
    match at {
        Some(loc) => {
            let l = cite::parse_locator(loc);
            let sp = cite::span(&tl.text, &l).ok_or_else(|| {
                Error::text_layer_missing(format!("locator '{loc}' not found in the text layer"))
            })?;
            for (ln, line) in sp {
                println!("{ln:>5} | {line}");
            }
        }
        None => print!("{}", tl.text),
    }
    Ok(0)
}
