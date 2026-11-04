//! Creating and maintaining pages: new, index, rename, archive, export, log.

use super::{Ctx, print_json};
use crate::cli::{ExportArgs, LogCmd, NewArgs};
use anyhow::Result;
use commonplace_core::{config, fsutil, index, manifest::Manifest, ops, scaffold};

pub(crate) fn new(ctx: &Ctx, a: &NewArgs) -> Result<i32> {
    let root = ctx.find_root()?;
    let cfg = config::Config::load(&root)?;
    let extra: Vec<(String, String)> = a
        .set
        .iter()
        .filter_map(|s| {
            s.split_once('=')
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        })
        .collect();
    let rep = scaffold::new_page(
        &root,
        &cfg,
        &a.r#type,
        &a.title,
        a.folder.as_deref(),
        &extra,
    )?;
    if ctx.json {
        print_json(&rep)?;
    } else {
        println!("created {} ({})", rep.path, rep.ptype);
    }
    Ok(0)
}

pub(crate) fn index(ctx: &Ctx) -> Result<i32> {
    let v = ctx.open()?;
    let rep = index::generate(&v)?;
    let mut m = Manifest::load(&v.root)?;
    m.last_index = Some(fsutil::now_iso());
    m.save(&v.root)?;
    if ctx.json {
        print_json(&rep)?;
    } else {
        println!(
            "index.md regenerated ({} lines); hubs: {}; page lists refreshed in {} pages",
            rep.index_lines,
            if rep.hubs.is_empty() {
                "none".into()
            } else {
                rep.hubs.join(", ")
            },
            rep.mocs_updated.len()
        );
    }
    Ok(0)
}

pub(crate) fn rename(ctx: &Ctx, old: &str, new: &str) -> Result<i32> {
    let v = ctx.open()?;
    let rep = ops::rename(&v, old, new)?;
    if ctx.json {
        print_json(&rep)?;
    } else {
        println!(
            "renamed {} → {} ({} links rewritten in {} files; old name kept as alias)",
            rep.from,
            rep.to,
            rep.links_rewritten,
            rep.files_updated.len()
        );
    }
    Ok(0)
}

pub(crate) fn archive(ctx: &Ctx, page: &str) -> Result<i32> {
    let v = ctx.open()?;
    let moved = ops::archive(&v, page)?;
    if ctx.json {
        print_json(&moved)?;
    } else {
        println!(
            "archived {} file(s). Promote any reusable concepts to resources/ and run `commonplace index`.",
            moved.len()
        );
    }
    Ok(0)
}

pub(crate) fn export(ctx: &Ctx, a: ExportArgs) -> Result<i32> {
    let v = ctx.open()?;
    let text = ops::export(&v, &a.targets, a.include_confidential)?;
    match a.out {
        Some(o) => {
            fsutil::write_atomic(&o, text.as_bytes())?;
            if !ctx.json {
                println!("wrote {}", o.display());
            }
        }
        None => print!("{text}"),
    }
    Ok(0)
}

pub(crate) fn log(ctx: &Ctx, cmd: LogCmd) -> Result<i32> {
    let root = ctx.find_root()?;
    match cmd {
        LogCmd::Add { op, title, notes } => {
            ops::log_append(&root, &op, &title, &notes)?;
            if !ctx.json {
                println!("logged: [{}] {op} | {title}", fsutil::today());
            }
        }
        LogCmd::Rotate { keep } => {
            let n = ops::log_rotate(&root, keep)?;
            if !ctx.json {
                println!("moved {n} entries to archive/");
            }
        }
        LogCmd::Show { n } => {
            // a commonplace without a log simply has no entries to show
            let t = std::fs::read_to_string(root.join("log.md")).unwrap_or_default();
            let heads: Vec<&str> = t.lines().filter(|l| l.starts_with("## [")).collect();
            for h in &heads[heads.len().saturating_sub(n)..] {
                println!("{h}");
            }
        }
    }
    Ok(0)
}
