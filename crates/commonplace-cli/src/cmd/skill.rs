//! Agent skills: new, list, diff, accept, reject, used, sync.

use super::{Ctx, print_json};
use crate::cli::{DecisionArgs, SkillCmd};
use anyhow::Result;
use commonplace_core::skill::{self, Decision, SkillState};

pub(crate) fn skill(ctx: &Ctx, cmd: SkillCmd) -> Result<i32> {
    match cmd {
        SkillCmd::New { name, description } => new(ctx, &name, &description),
        SkillCmd::List => list(ctx),
        SkillCmd::Diff { name } => diff(ctx, &name),
        SkillCmd::Accept(a) => decide(ctx, &a, Decision::Accept),
        SkillCmd::Reject(a) => decide(ctx, &a, Decision::Reject),
        SkillCmd::Used {
            name,
            outcome,
            note,
        } => {
            let root = ctx.find_root()?;
            skill::record_use(&root, &name, outcome.into(), note.as_deref())?;
            if ctx.json {
                print_json(&serde_json::json!({ "name": name, "recorded": true }))?;
            } else {
                println!("recorded a use of {name} in {}", skill::IMPACT_FILE);
            }
            Ok(0)
        }
        SkillCmd::Sync { user, names } => sync(ctx, user, &names),
    }
}

fn new(ctx: &Ctx, name: &str, description: &str) -> Result<i32> {
    let root = ctx.find_root()?;
    let path = skill::new_skill(&root, name, description)?;
    if ctx.json {
        print_json(&serde_json::json!({ "name": name, "path": path }))?;
    } else {
        println!(
            "created {path}/ (SKILL.md, PURPOSE.md); a proposal until `commonplace skill accept {name}`"
        );
    }
    Ok(0)
}

fn list(ctx: &Ctx) -> Result<i32> {
    let root = ctx.find_root()?;
    let skills = skill::list(&root)?;
    if ctx.json {
        print_json(&skills)?;
        return Ok(0);
    }
    if skills.is_empty() {
        println!("no skills; create one with `commonplace skill new <name> --description \"...\"`");
    }
    for s in &skills {
        let state = match s.state {
            SkillState::New => "new",
            SkillState::Accepted => "accepted",
            SkillState::Changed => "changed",
            SkillState::Removed => "removed",
        };
        println!(
            "{:<24} {state:<8} uses {} (failed {}) last used {}  purpose {}",
            s.name,
            s.uses,
            s.failures,
            s.last_used.as_deref().unwrap_or("never"),
            s.purpose.len()
        );
        if !s.description.is_empty() {
            println!("  {}", s.description);
        }
    }
    Ok(0)
}

fn diff(ctx: &Ctx, name: &str) -> Result<i32> {
    let root = ctx.find_root()?;
    let d = skill::diff(&root, name)?;
    if ctx.json {
        print_json(&serde_json::json!({ "name": name, "pending": !d.is_empty(), "diff": d }))?;
    } else if d.is_empty() {
        println!("{name}: no pending change");
    } else {
        print!("{d}");
    }
    Ok(0)
}

fn decide(ctx: &Ctx, a: &DecisionArgs, decision: Decision) -> Result<i32> {
    let v = ctx.open()?;
    let rep = skill::decide(&v, &a.name, decision, &a.reason, &a.evidence)?;
    if ctx.json {
        print_json(&rep)?;
    } else {
        println!(
            "{} {} ({}); recorded in {}",
            rep.decision,
            rep.name,
            rep.files.join(", "),
            skill::IMPACT_FILE
        );
        for p in &rep.installed {
            println!("  installed {p}");
        }
    }
    Ok(0)
}

fn sync(ctx: &Ctx, user: bool, names: &[String]) -> Result<i32> {
    let root = ctx.find_root()?;
    let rep = skill::sync(&root, user, names)?;
    if ctx.json {
        print_json(&rep)?;
        return Ok(0);
    }
    println!(
        "installed {} skill folder(s), removed {}",
        rep.installed.len(),
        rep.removed.len()
    );
    for p in &rep.removed {
        println!("  removed {p}");
    }
    if !rep.pending.is_empty() {
        println!(
            "pending proposals (not installed until accepted): {}",
            rep.pending.join(", ")
        );
    }
    Ok(0)
}
