//! Hygiene checks: lint, citations, links, duplicates.

use super::{Ctx, paths_set, print_json};
use crate::cli::{CiteCmd, LinksCmd, LintArgs};
use anyhow::Result;
use commonplace_core::Error;
use commonplace_core::{cite, fsutil, lint, manifest::Manifest, ops, sources, vault::Vault};
use regex::Regex;
use std::sync::LazyLock;

pub(crate) fn lint(ctx: &Ctx, a: LintArgs) -> Result<i32> {
    let v = ctx.open()?;
    let packs = super::load_packs(&v)?;
    let only = paths_set(&a.paths, a.changed, &v.root)?;
    if a.changed
        && only
            .as_ref()
            .is_some_and(std::collections::HashSet::is_empty)
    {
        if ctx.json {
            print_json(&lint::LintReport::default())?;
        } else {
            println!("nothing changed since the last commit/lint");
        }
        return Ok(0);
    }
    let opts = lint::LintOptions {
        only,
        verify_citations: !a.no_cite,
        rules: if a.rule.is_empty() {
            None
        } else {
            Some(a.rule)
        },
    };
    let rep = lint::lint(&v, &packs, &opts);
    let mut m = Manifest::load(&v.root)?;
    m.last_lint = Some(fsutil::now_iso());
    m.save(&v.root)?;
    if ctx.json {
        print_json(&rep)?;
    } else {
        println!(
            "lint: {} pages — {} errors, {} warnings, {} info",
            rep.pages_checked, rep.errors, rep.warnings, rep.infos
        );
        for f in &rep.findings {
            println!(
                "  [{:<7}] {:<9} {}{}: {}",
                f.severity,
                f.rule,
                f.path,
                f.line.map(|l| format!(":{l}")).unwrap_or_default(),
                f.message
            );
        }
    }
    let fail = match a.fail_on.as_str() {
        "warning" | "warn" => rep.errors + rep.warnings > 0,
        _ => rep.errors > 0,
    };
    Ok(i32::from(fail))
}

pub(crate) fn cite(ctx: &Ctx, cmd: CiteCmd) -> Result<i32> {
    let v = ctx.open()?;
    match cmd {
        CiteCmd::Verify { changed, paths } => cite_verify(ctx, &v, &paths, changed),
        CiteCmd::Find { source, quote } => cite_find(ctx, &v, &source, &quote),
    }
}

fn is_failure(v: &cite::Verdict) -> bool {
    matches!(
        v,
        cite::Verdict::QuoteNotFound | cite::Verdict::QuoteNotInSpan | cite::Verdict::SourceMissing
    )
}

fn cite_verify(ctx: &Ctx, v: &Vault, paths: &[String], changed: bool) -> Result<i32> {
    let only = paths_set(paths, changed, &v.root)?;
    let res = lint::verify_all_citations(v, only.as_ref());
    let fails = res.iter().filter(|r| is_failure(&r.verdict)).count();
    if ctx.json {
        print_json(&res)?;
    } else {
        for r in &res {
            let tag = match r.verdict {
                cite::Verdict::Pass => "PASS ",
                ref x if is_failure(x) => "FAIL ",
                _ => "WARN ",
            };
            println!(
                "{tag} {}:{} [^{}] → [[{}]] {} — {}",
                r.page, r.line, r.label, r.source, r.locator, r.message
            );
        }
        println!("{} citations, {} failed", res.len(), fails);
    }
    Ok(i32::from(fails > 0))
}

#[expect(
    clippy::expect_used,
    reason = "patterns are compile-time literals; an invalid one is a programming error"
)]
fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("built-in regex literal is valid")
}

static PAGE_MARK_RE: LazyLock<Regex> = LazyLock::new(|| re(r"<!--\s*page:\s*(\d+)\s*-->"));
static PARA_MARK_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(?:<!--\s*para:\s*(\d+)\s*-->|\[(\d+)\]\s)"));

/// The last page and paragraph markers at or before line `upto`.
fn page_and_para(lines: &[(usize, &str)], upto: usize) -> (Option<String>, Option<String>) {
    let mut page = None;
    let mut para = None;
    for (_, line) in lines.iter().take_while(|(ln, _)| *ln <= upto) {
        if let Some(c) = PAGE_MARK_RE.captures(line) {
            page = Some(c[1].to_string());
        }
        if let Some(c) = PARA_MARK_RE.captures(line) {
            para = c.get(1).or(c.get(2)).map(|m| m.as_str().to_string());
        }
    }
    (page, para)
}

fn cite_find(ctx: &Ctx, v: &Vault, source: &str, quote: &str) -> Result<i32> {
    let tl = sources::load_text_layer(&v.root, source)?
        .ok_or_else(|| Error::text_layer_missing(format!("no text layer for '{source}'")))?;
    let lines: Vec<(usize, &str)> = tl
        .text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l))
        .collect();
    let hay = cite::Haystack::new(&lines);
    let Some(l) = hay.verify(quote) else {
        let near = hay.nearest_lines(quote, 5);
        if ctx.json {
            print_json(&serde_json::json!({"found": false, "nearest_lines": near}))?;
        } else {
            println!(
                "not found verbatim. Nearest lines: {}",
                near.iter()
                    .map(|l| format!("L{l}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            for l in near.iter().take(3) {
                if let Some((_, line)) = l.checked_sub(1).and_then(|i| lines.get(i)) {
                    println!("  L{l}: {}", line.trim());
                }
            }
        }
        return Ok(1);
    };
    let (page, para) = page_and_para(&lines, l);
    let loc = match (&para, &page) {
        (Some(p), _) => format!("para {p}"),
        (None, Some(p)) => format!("p.{p}"),
        (None, None) => tl.locate(l),
    };
    if ctx.json {
        print_json(
            &serde_json::json!({"found": true, "line": l, "where": tl.locate(l), "page": page, "para": para, "locator": loc}),
        )?;
    } else {
        println!(
            "found at {} → use locator: {loc}\n[^n]: [[{source}]] {loc} \"{quote}\"",
            tl.locate(l)
        );
    }
    Ok(0)
}

pub(crate) fn links(ctx: &Ctx, cmd: LinksCmd) -> Result<i32> {
    let v = ctx.open()?;
    match cmd {
        LinksCmd::Check => {
            let issues = ops::broken_links(&v);
            if ctx.json {
                print_json(&issues)?;
            } else {
                for i in &issues {
                    println!(
                        "{}{}: [[{}]]{}",
                        i.page,
                        i.line.map(|l| format!(":{l}")).unwrap_or_default(),
                        i.target,
                        if i.suggestions.is_empty() {
                            String::new()
                        } else {
                            format!(" → did you mean {}?", super::wikilinks(&i.suggestions))
                        }
                    );
                }
                println!("{} broken links", issues.len());
            }
            Ok(i32::from(!issues.is_empty()))
        }
        LinksCmd::Backlinks { page } => {
            let b = ops::backlinks_of(&v, &page)?;
            if ctx.json {
                print_json(&b)?;
            } else {
                for x in &b {
                    println!("{x}");
                }
                println!("{} backlinks", b.len());
            }
            Ok(0)
        }
    }
}

pub(crate) fn dedupe(ctx: &Ctx, threshold: f64) -> Result<i32> {
    let v = ctx.open()?;
    let d = ops::dedupe(&v, threshold);
    if ctx.json {
        print_json(&d)?;
    } else {
        for c in &d {
            println!("{:.2}  {}  ~  {}  ({})", c.similarity, c.a, c.b, c.reason);
        }
        println!(
            "{} candidate pairs (judge them; merge with `commonplace rename` + edits, never automatically)",
            d.len()
        );
    }
    Ok(0)
}
