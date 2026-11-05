//! `commonplace` — deterministic hygiene tools for an LLM-maintained research wiki.
//! Exit codes: 0 ok · 1 findings at error level · 2 the command failed · 3 integrity violation.
//! On exit 2 the error is printed as `error[code]: message` on stderr, or with
//! `--json` as `{"error": {...}}` on stdout. The codes are listed in the
//! skill's `references/cli.md`.

mod cli;
mod cmd;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Cmd};
use cmd::{Ctx, check, find, graph, pages, review, setup, source};
use serde::Serialize;

fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => usage_error(&e),
    };
    let json = cli.json;
    let code = match run(cli) {
        Ok(c) => c,
        Err(e) => {
            report(&e).print(json);
            2
        }
    };
    std::process::exit(code);
}

/// A failed command, as printed for a person or (with `--json`) an agent.
#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: String,
    hint: Option<String>,
    suggestions: &'a [String],
    path: Option<String>,
}

impl ErrorBody<'_> {
    fn print(&self, json: bool) {
        if json {
            let wrapped = serde_json::json!({ "error": self });
            match serde_json::to_string_pretty(&wrapped) {
                Ok(s) => println!("{s}"),
                Err(_) => eprintln!("error[{}]: {}", self.code, self.message),
            }
        } else {
            eprintln!("error[{}]: {}", self.code, self.message);
            if let Some(h) = &self.hint {
                eprintln!("  hint: {h}");
            }
        }
    }
}

/// Describe a failed command; the code is `internal` when the error did not
/// come from commonplace-core.
fn report(e: &anyhow::Error) -> ErrorBody<'_> {
    let core = e
        .chain()
        .find_map(|c| c.downcast_ref::<commonplace_core::Error>());
    match core {
        Some(ce) => ErrorBody {
            code: ce.code(),
            message: message_through_core(e),
            hint: ce.hint(),
            suggestions: ce.suggestions(),
            path: ce.path().map(|p| p.display().to_string()),
        },
        None => ErrorBody {
            code: "internal",
            message: format!("{e:#}"),
            hint: None,
            suggestions: &[],
            path: None,
        },
    }
}

/// The error chain joined with ": ", stopping at the core error, whose
/// message already includes its own cause.
fn message_through_core(e: &anyhow::Error) -> String {
    let mut parts = vec![];
    for c in e.chain() {
        parts.push(c.to_string());
        if c.is::<commonplace_core::Error>() {
            break;
        }
    }
    parts.join(": ")
}

/// Handle a command line clap rejected. Help and version print as usual;
/// with `--json`, a real usage error becomes an error object (code `usage`).
fn usage_error(e: &clap::Error) -> ! {
    if !e.use_stderr() || !std::env::args().any(|a| a == "--json") {
        e.exit();
    }
    let rendered = e.to_string();
    let first = rendered.lines().next().unwrap_or_default();
    ErrorBody {
        code: "usage",
        message: first.trim_start_matches("error: ").to_string(),
        hint: Some("run `commonplace <command> --help`".into()),
        suggestions: &[],
        path: None,
    }
    .print(true);
    std::process::exit(2);
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
