//! Persona reviews: build a packet for a fresh-context reviewer, record its findings.

use super::{Ctx, print_json, skill_dir};
use crate::cli::ReviewCmd;
use anyhow::Result;
use commonplace_core::{fsutil, ops, scaffold, vault};
use std::path::PathBuf;

pub(crate) fn review(ctx: &Ctx, cmd: ReviewCmd) -> Result<i32> {
    let v = ctx.open()?;
    match cmd {
        ReviewCmd::Prepare {
            persona,
            targets,
            out,
        } => prepare(ctx, &v, &persona, &targets, out),
        ReviewCmd::Record {
            persona,
            findings,
            scope,
            model,
        } => {
            let fj = std::fs::read_to_string(&findings)
                .map_err(|e| commonplace_core::Error::io("reading", &findings, e))?;
            let rep = ops::review_record(&v, &persona, &scope, &fj, model.as_deref())?;
            if ctx.json {
                print_json(&rep)?;
            } else {
                println!(
                    "recorded {} finding(s) ({} blockers) on {} page(s); see reviews/",
                    rep.findings,
                    rep.blockers,
                    rep.pages_updated.len()
                );
            }
            Ok(i32::from(rep.blockers > 0))
        }
    }
}

fn prepare(
    ctx: &Ctx,
    v: &vault::Vault,
    persona: &str,
    targets: &[String],
    out: Option<PathBuf>,
) -> Result<i32> {
    let text = scaffold::persona_text(&v.root, skill_dir().as_deref(), &v.config, persona)?;
    let packet = ops::review_packet(
        v,
        &ops::Persona {
            name: persona,
            brief: &text,
        },
        targets,
        v.config.review.context_lines,
    )?;
    let out = out.unwrap_or_else(|| {
        vault::tool_dir(&v.root).join("reviews").join(format!(
            "{} {} {} packet.md",
            fsutil::today(),
            persona,
            fsutil::slug(&targets.join(" "), 40)
        ))
    });
    fsutil::write_atomic(&out, packet.as_bytes())?;
    if ctx.json {
        print_json(
            &serde_json::json!({"packet": out.display().to_string(), "persona": persona, "pages": targets}),
        )?;
    } else {
        println!(
            "packet written to {}\nGive it to a fresh-context subagent (ideally another model). It must return a JSON array of findings; then run:\n  commonplace review record {persona} <findings.json> --scope \"{}\"",
            out.display(),
            targets.join(",")
        );
    }
    Ok(0)
}
