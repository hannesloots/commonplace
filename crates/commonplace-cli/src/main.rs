//! `commonplace` — deterministic hygiene tools for an LLM-maintained research wiki.
//! Exit codes: 0 ok · 1 findings at error level · 2 usage error · 3 integrity violation.

mod cli;
mod cmd;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Cmd};
use cmd::{Ctx, check, find, graph, pages, review, setup, source};

fn main() {
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e:#}");
            2
        }
    };
    std::process::exit(code);
}

fn run(cli: Cli) -> Result<i32> {
    let ctx = Ctx {
        root: cli.root,
        json: cli.json,
    };
    match cli.cmd {
        Cmd::Init(a) => setup::init(&ctx, a),
        Cmd::Doctor => setup::doctor(&ctx),
        Cmd::Status => setup::status(&ctx),
        Cmd::Pack { cmd } => setup::pack(&ctx, cmd),
        Cmd::Ingest(a) => source::ingest(&ctx, a),
        Cmd::Source { cmd } => source::source(&ctx, cmd),
        Cmd::New(a) => pages::new(&ctx, &a),
        Cmd::Index => pages::index(&ctx),
        Cmd::Rename { old, new } => pages::rename(&ctx, &old, &new),
        Cmd::Archive { page } => pages::archive(&ctx, &page),
        Cmd::Export(a) => pages::export(&ctx, a),
        Cmd::Log { cmd } => pages::log(&ctx, cmd),
        Cmd::Lint(a) => check::lint(&ctx, a),
        Cmd::Cite { cmd } => check::cite(&ctx, cmd),
        Cmd::Links { cmd } => check::links(&ctx, cmd),
        Cmd::Dedupe { threshold } => check::dedupe(&ctx, threshold),
        Cmd::Search(a) => find::search(&ctx, a),
        Cmd::Query(a) => find::query(&ctx, a),
        Cmd::Trace(a) => find::trace(&ctx, a),
        Cmd::Review { cmd } => review::review(&ctx, cmd),
        Cmd::Graph { cmd } => graph::graph(&ctx, cmd),
    }
}
