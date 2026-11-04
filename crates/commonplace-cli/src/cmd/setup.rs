//! Setting up and inspecting a commonplace: init, doctor, status, packs.

use super::{Ctx, print_json, skill_dir};
use crate::cli::{InitArgs, PackCmd};
use anyhow::Result;
use commonplace_core::Error;
use commonplace_core::{config, index, ops, pack, scaffold, sources, vault::Vault};
use serde::Serialize;

pub(crate) fn init(ctx: &Ctx, a: InitArgs) -> Result<i32> {
    let root = match a.path {
        Some(p) => p,
        None => std::env::current_dir()?,
    };
    let rep = scaffold::init(
        &root,
        &a.name,
        &a.purpose,
        &a.pack,
        &a.jurisdiction,
        a.force,
    )?;
    // generate an index so the commonplace lints clean from the start; a
    // failure here is not fatal, `commonplace index` can be rerun later
    let v = Vault::open(&root)?;
    let _ = index::generate(&v);
    if ctx.json {
        return print_json(&rep).map(|()| 0);
    }
    println!("Initialised commonplace at {}", rep.root);
    for c in &rep.created {
        println!("  created {c}");
    }
    for s in &rep.skipped {
        println!("  kept    {s}");
    }
    if !rep.packs.is_empty() {
        println!("  packs   {}", rep.packs.join(", "));
    }
    if rep.packs.iter().any(|p| p == "legal") {
        let j = config::Config::load(&root)
            .map(|c| c.jurisdiction)
            .unwrap_or_default();
        if j.is_empty() {
            println!(
                "  legal pack: no jurisdiction set (generic citation patterns). Set one: `commonplace pack install legal --jurisdiction {}`",
                scaffold::known_jurisdictions("legal").join("|")
            );
        } else {
            println!("  legal pack: jurisdiction {j}");
        }
    }
    println!("Next: read AGENTS.md; `git init` is strongly recommended.");
    Ok(0)
}

/// What `doctor` reports. Fields are in alphabetical order to keep the JSON
/// key order stable.
#[derive(Serialize)]
struct Doctor {
    arch: &'static str,
    converters: Vec<String>,
    git: bool,
    git_repo: bool,
    jurisdiction: String,
    os: &'static str,
    packs_enabled: Vec<String>,
    packs_known: Vec<String>,
    root: Option<String>,
    skill_dir: Option<String>,
    version: &'static str,
}

pub(crate) fn doctor(ctx: &Ctx) -> Result<i32> {
    let root = ctx.find_root().ok();
    let cfg = root.as_ref().and_then(|r| config::Config::load(r).ok());
    let (packs_enabled, jurisdiction) = cfg.map(|c| (c.packs, c.jurisdiction)).unwrap_or_default();
    let d = Doctor {
        arch: std::env::consts::ARCH,
        converters: sources::available_converters(),
        git: sources::which("git"),
        git_repo: root.as_ref().is_some_and(|r| r.join(".git").exists()),
        jurisdiction,
        os: std::env::consts::OS,
        packs_enabled,
        packs_known: scaffold::known_packs(),
        root: root.as_ref().map(|r| r.display().to_string()),
        skill_dir: skill_dir().map(|p| p.display().to_string()),
        version: commonplace_core::VERSION,
    };
    if ctx.json {
        return print_json(&d).map(|()| 0);
    }
    println!("commonplace {} on {}/{}", d.version, d.os, d.arch);
    println!(
        "root:       {}",
        d.root
            .as_deref()
            .unwrap_or("(none found: run `commonplace init` or pass --root)")
    );
    println!(
        "git:        {}{}",
        if d.git { "available" } else { "missing" },
        if d.git_repo {
            ", commonplace is a repo"
        } else {
            ""
        }
    );
    println!(
        "converters: {}",
        if d.converters.is_empty() {
            "none (PDF/DOCX will need manual conversion)".to_string()
        } else {
            d.converters.join(", ")
        }
    );
    println!(
        "packs:      enabled {:?}; known {:?}",
        d.packs_enabled, d.packs_known
    );
    if d.packs_enabled.iter().any(|p| p == "legal") {
        println!(
            "legal:      jurisdiction {}",
            if d.jurisdiction.is_empty() {
                "NOT SET (generic patterns; `commonplace pack install legal --jurisdiction za|au|en`)"
            } else {
                &d.jurisdiction
            }
        );
    }
    println!(
        "skill dir:  {}",
        d.skill_dir
            .as_deref()
            .unwrap_or("(not set; pass COMMONPLACE_SKILL_DIR for persona lookup)")
    );
    Ok(0)
}

pub(crate) fn status(ctx: &Ctx) -> Result<i32> {
    let v = ctx.open()?;
    let s = ops::status(&v)?;
    if ctx.json {
        return print_json(&s).map(|()| 0);
    }
    println!(
        "{} — {} pages, {} sources ({} unconverted, {} not yet summarised), {} citations",
        s.name,
        s.pages,
        s.sources_total,
        s.sources_unconverted,
        s.sources_unsummarised,
        s.citations_total
    );
    println!(
        "active projects: {}",
        if s.active_projects.is_empty() {
            "none".into()
        } else {
            s.active_projects.join(", ")
        }
    );
    if !s.projects_without_next_action.is_empty() {
        println!(
            "  without next action: {}",
            s.projects_without_next_action.join(", ")
        );
    }
    println!(
        "inbox: {} · open questions: {} · open contradictions: {} · last journal: {}",
        s.inbox_items,
        s.open_questions,
        s.open_contradictions,
        s.journal_last.as_deref().unwrap_or("none")
    );
    println!(
        "by type: {}",
        s.by_type
            .iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "packs: {:?} · last lint: {}",
        s.packs,
        s.last_lint.as_deref().unwrap_or("never")
    );
    Ok(0)
}

pub(crate) fn pack(ctx: &Ctx, cmd: PackCmd) -> Result<i32> {
    match cmd {
        PackCmd::List => Ok(pack_list(ctx)),
        PackCmd::Install { name, jurisdiction } => {
            pack_install(ctx, &name, jurisdiction.as_deref())
        }
    }
}

fn pack_list(ctx: &Ctx) -> i32 {
    let cfg = ctx
        .find_root()
        .ok()
        .and_then(|r| config::Config::load(&r).ok());
    let (enabled, jur) = cfg.map(|c| (c.packs, c.jurisdiction)).unwrap_or_default();
    for p in scaffold::known_packs() {
        let extra = if p == "legal" {
            format!(
                "  (jurisdictions: {}; current: {})",
                scaffold::known_jurisdictions("legal").join(", "),
                if jur.is_empty() { "generic" } else { &jur }
            )
        } else {
            String::new()
        };
        println!(
            "{} {p}{extra}",
            if enabled.contains(&p) { "[x]" } else { "[ ]" }
        );
    }
    0
}

fn pack_install(ctx: &Ctx, name: &str, jurisdiction: Option<&str>) -> Result<i32> {
    let root = ctx.find_root()?;
    let mut cfg = config::Config::load(&root)?;
    let (name, alias_jur) = scaffold::normalise_pack(name);
    if !scaffold::known_packs().contains(&name) {
        return Err(Error::invalid_input(format!(
            "unknown pack '{name}'; known: {}",
            scaffold::known_packs().join(", ")
        ))
        .into());
    }
    scaffold::install_pack(&root, &name)?;
    if !cfg.packs.contains(&name) {
        cfg.packs.push(name.clone());
    }
    if let Some(j) = jurisdiction.or(alias_jur) {
        let known = scaffold::known_jurisdictions("legal");
        if !known.iter().any(|k| k == j)
            && !commonplace_core::vault::tool_dir(&root)
                .join("packs/legal/jurisdictions")
                .join(format!("{j}.yaml"))
                .is_file()
        {
            return Err(Error::invalid_input(format!(
                "unknown jurisdiction '{j}'; shipped profiles: {} (or add .commonplace/packs/legal/jurisdictions/{j}.yaml)",
                known.join(", ")
            ))
            .into());
        }
        cfg.jurisdiction = j.to_string();
    }
    cfg.save(&root)?;
    // prove the pack loads with this jurisdiction
    pack::load_packs(
        &commonplace_core::vault::tool_dir(&root).join("packs"),
        std::slice::from_ref(&name),
        &cfg.jurisdiction,
    )?;
    if ctx.json {
        return Ok(0);
    }
    println!(
        "pack '{name}' installed and enabled; read its workflow in the skill's packs/{name}/ folder"
    );
    if name == "legal" {
        if cfg.jurisdiction.is_empty() {
            println!(
                "no jurisdiction set: generic citation patterns apply. Set one with `commonplace pack install legal --jurisdiction {}`",
                scaffold::known_jurisdictions("legal").join("|")
            );
        } else {
            println!(
                "jurisdiction: {} (profile .commonplace/packs/legal/jurisdictions/{}.yaml)",
                cfg.jurisdiction, cfg.jurisdiction
            );
        }
    }
    Ok(0)
}
