//! `commonplace` — deterministic hygiene tools for an LLM-maintained research wiki.
//! Exit codes: 0 ok · 1 findings at error level · 2 usage error · 3 integrity violation.

use anyhow::{anyhow, Result};
use commonplace_core::{cite, config, index, lint, ops, pack, scaffold, search, sources, vault::Vault};
use clap::{Args, Parser, Subcommand};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "commonplace", version = commonplace_core::VERSION, about = "Deterministic tools for a cited markdown research wiki / second brain", long_about = None)]
struct Cli {
    /// Commonplace root (default: walk up from the current directory to find commonplace.toml)
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    /// Machine-readable JSON output
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a new commonplace (folders, AGENTS.md, templates, packs)
    Init {
        path: Option<PathBuf>,
        #[arg(long, default_value = "")]
        name: String,
        #[arg(long, default_value = "")]
        purpose: String,
        /// Comma-separated packs: ea, legal, compare, study
        #[arg(long, value_delimiter = ',')]
        pack: Vec<String>,
        /// Jurisdiction profile for the legal pack: za, au, en (or a code you add)
        #[arg(long, default_value = "")]
        jurisdiction: String,
        #[arg(long)]
        force: bool,
    },
    /// Report environment: root, converters, git, packs
    Doctor,
    /// Counts and pending work
    Status,
    /// Register a file as an immutable source, convert it, create its note
    Ingest {
        files: Vec<PathBuf>,
        #[arg(long)]
        title: Option<String>,
        /// Publication date YYYY-MM-DD (default: today, or a date prefix in the title)
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        /// Where it came from (URL, "email from X", "scan")
        #[arg(long)]
        origin: Option<String>,
        #[arg(long)]
        project: Option<String>,
        /// public | personal | confidential | privileged
        #[arg(long)]
        confidentiality: Option<String>,
        /// primary | secondary | tertiary
        #[arg(long)]
        authority: Option<String>,
        /// Force a converter: pdftotext | pandoc | markitdown
        #[arg(long)]
        converter: Option<String>,
        /// Move the file instead of copying it
        #[arg(long)]
        r#move: bool,
        #[arg(long)]
        force: bool,
    },
    /// Source-layer operations
    Source {
        #[command(subcommand)]
        cmd: SourceCmd,
    },
    /// Create a page from its template
    New {
        r#type: String,
        title: String,
        #[arg(long)]
        folder: Option<String>,
        /// Extra frontmatter: --set key=value (lists as "[a, b]")
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
    },
    /// Deterministic hygiene checks (frontmatter, links, citations, packs)
    Lint {
        /// Only report pages changed since the last commit / lint
        #[arg(long)]
        changed: bool,
        /// Only these rule prefixes (e.g. CITE, LINK-001)
        #[arg(long, value_delimiter = ',')]
        rule: Vec<String>,
        /// Skip citation verification (faster)
        #[arg(long)]
        no_cite: bool,
        /// Exit 1 at this severity or worse: error (default) | warning
        #[arg(long, default_value = "error")]
        fail_on: String,
        /// Restrict to these paths
        paths: Vec<String>,
    },
    /// Citation checks
    Cite {
        #[command(subcommand)]
        cmd: CiteCmd,
    },
    /// Link graph
    Links {
        #[command(subcommand)]
        cmd: LinksCmd,
    },
    /// Candidate duplicate pages
    Dedupe {
        #[arg(long, default_value_t = 0.86)]
        threshold: f64,
    },
    /// Regenerate index.md, hubs and the page lists in project/area/topic pages
    Index,
    /// Ranked full-text search (BM25)
    Search {
        query: Vec<String>,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        include_archive: bool,
        /// Also search source text layers
        #[arg(long)]
        text: bool,
        #[arg(long = "type")]
        ptype: Option<String>,
        #[arg(long, default_value_t = 15)]
        limit: usize,
    },
    /// Frontmatter query, e.g. --where "type=capability and target_maturity>baseline_maturity"
    Query {
        #[arg(long = "where")]
        r#where: String,
        /// Fields to show, comma-separated
        #[arg(long, value_delimiter = ',')]
        fields: Vec<String>,
        #[arg(long, default_value = "table")]
        format: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        include_archive: bool,
    },
    /// Persona review packets and findings
    Review {
        #[command(subcommand)]
        cmd: ReviewCmd,
    },
    /// Rename a page and rewrite every link to it
    Rename { old: String, new: String },
    /// Move a project/area (its folder) into archive/
    Archive { page: String },
    /// Concatenate pages with a bibliography, honouring confidentiality
    Export {
        targets: Vec<String>,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        include_confidential: bool,
    },
    /// Traceability tables along typed links
    Trace {
        /// Chain: type,field,type[,field,type...]
        #[arg(long, value_delimiter = ',')]
        chain: Vec<String>,
        /// A trace declared by a pack
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "md")]
        format: String,
    },
    /// Append to log.md, rotate, or show recent entries
    Log {
        #[command(subcommand)]
        cmd: LogCmd,
    },
    /// Pack management
    Pack {
        #[command(subcommand)]
        cmd: PackCmd,
    },
    /// The commonplace as a typed graph: paths, neighbourhoods, ontology, exports (Cypher/GraphML/Mermaid/DOT/JSON)
    Graph {
        #[command(subcommand)]
        cmd: GraphCmd,
    },
}

#[derive(Subcommand)]
enum GraphCmd {
    /// Node/edge counts, components, hubs, isolated pages
    Stats {
        #[arg(long)]
        mentions: bool,
    },
    /// The ontology: node types and relation types with from→to pairs
    Schema {
        #[arg(long)]
        mentions: bool,
    },
    /// Everything reachable from a page within N hops
    Walk {
        page: String,
        #[arg(long, default_value_t = 2)]
        depth: usize,
        /// Only follow these relations (comma-separated), e.g. enabled_by,realised_by
        #[arg(long, value_delimiter = ',')]
        rel: Vec<String>,
        /// out | in | both
        #[arg(long, default_value = "both")]
        direction: String,
        #[arg(long)]
        mentions: bool,
    },
    /// Shortest paths between two pages
    Path {
        from: String,
        to: String,
        #[arg(long, default_value_t = 4)]
        max_depth: usize,
        #[arg(long, value_delimiter = ',')]
        rel: Vec<String>,
        #[arg(long, default_value = "both")]
        direction: String,
        #[arg(long, default_value_t = 5)]
        limit: usize,
        #[arg(long)]
        mentions: bool,
    },
    /// Pages related to a page, with why (links, co-citation, shared neighbours, tags, random walk)
    Related {
        page: String,
        #[arg(long, default_value_t = 15)]
        limit: usize,
        /// Ignore plain wikilinks; use typed relations and citations only
        #[arg(long)]
        typed_only: bool,
    },
    /// Most central pages (PageRank, degree, betweenness)
    Hubs {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        typed_only: bool,
    },
    /// Clusters of pages (Louvain), with their hub page and evidence count
    Communities {
        #[arg(long, default_value_t = 3)]
        min_size: usize,
        #[arg(long)]
        typed_only: bool,
    },
    /// What is missing: wanted pages, orphans, dead ends, uncited sources, clusters without a hub, bridges
    Gaps {
        #[arg(long)]
        typed_only: bool,
    },
    /// Unlinked mentions of other pages' titles and aliases, with the line to link on
    SuggestLinks {
        page: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Page pairs worth checking for contradiction or staleness (shared sources, similar titles)
    Conflicts {
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },
    /// Export the graph: cypher | graphml | mermaid | dot | json
    Export {
        #[arg(long, default_value = "cypher")]
        format: String,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        mentions: bool,
        /// Mermaid only: restrict to the neighbourhood of this page
        #[arg(long)]
        around: Option<String>,
        #[arg(long, default_value_t = 2)]
        depth: usize,
    },
}

#[derive(Subcommand)]
enum SourceCmd {
    /// Re-hash originals and text layers
    Verify,
    /// Register a hand-written or externally converted text layer
    AttachText {
        id: String,
        #[arg(long, default_value = "manual")]
        converter: String,
        #[arg(long, default_value = "medium")]
        fidelity: String,
    },
    /// List sources with status
    List,
    /// Rewrite relative web links in a source's text layer and note to absolute URLs (against its origin)
    Relink {
        id: Option<String>,
        /// Every source in the manifest
        #[arg(long)]
        all: bool,
    },
    /// Show a source's note, or its text layer span
    Show {
        id: String,
        /// Print the text layer instead of the note
        #[arg(long)]
        text: bool,
        /// Locator, e.g. p.12, para 26, §3.2, L100-140
        #[arg(long)]
        at: Option<String>,
    },
}

#[derive(Subcommand)]
enum CiteCmd {
    /// Verify every footnote citation against the source text layers
    Verify {
        #[arg(long)]
        changed: bool,
        paths: Vec<String>,
    },
    /// Find where a quote occurs in a source (to build a locator)
    Find {
        source: String,
        quote: String,
    },
}

#[derive(Subcommand)]
enum LinksCmd {
    /// Broken links with suggestions
    Check,
    /// Pages linking to a page
    Backlinks { page: String },
}

#[derive(Subcommand)]
enum ReviewCmd {
    /// Build a packet (persona brief + pages + cited spans) for a fresh-context reviewer
    Prepare {
        persona: String,
        targets: Vec<String>,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Record a reviewer's JSON findings into the pages and reviews/
    Record {
        persona: String,
        findings: PathBuf,
        #[arg(long)]
        scope: String,
        #[arg(long)]
        model: Option<String>,
    },
}

#[derive(Subcommand)]
enum LogCmd {
    /// Append an entry: commonplace log add ingest "Title" --note "Created: ..." --note "Updated: ..."
    Add {
        op: String,
        title: String,
        #[arg(long = "note")]
        notes: Vec<String>,
    },
    /// Move old entries to archive/log-<year>.md
    Rotate {
        #[arg(long, default_value_t = 150)]
        keep: usize,
    },
    /// Show the last N entry headings
    Show {
        #[arg(short, default_value_t = 10)]
        n: usize,
    },
}

#[derive(Subcommand)]
enum PackCmd {
    /// Known packs, which are enabled, and the legal pack's jurisdiction profiles
    List,
    /// Enable and install a pack into this commonplace
    Install {
        name: String,
        /// Jurisdiction profile for the legal pack: za, au, en (or a code you add)
        #[arg(long)]
        jurisdiction: Option<String>,
    },
}

#[derive(Args)]
struct Empty {}

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

fn find_root(cli_root: &Option<PathBuf>) -> Result<PathBuf> {
    match cli_root {
        Some(r) => Ok(r.canonicalize().unwrap_or(r.clone())),
        None => config::find_root(&std::env::current_dir()?),
    }
}

fn open(cli_root: &Option<PathBuf>) -> Result<Vault> {
    Vault::open(&find_root(cli_root)?)
}

fn load_packs(v: &Vault) -> Result<Vec<pack::Pack>> {
    pack::load_packs(&v.packs_dir(), &v.config.packs, &v.config.jurisdiction)
}

fn skill_dir() -> Option<PathBuf> {
    std::env::var_os("COMMONPLACE_SKILL_DIR").or_else(|| std::env::var_os("BRAIN_SKILL_DIR")).map(PathBuf::from).or_else(|| std::env::var_os("CLAUDE_SKILL_DIR").map(PathBuf::from)).filter(|p| p.is_dir())
}

fn print_json<T: serde::Serialize>(v: &T) {
    println!("{}", serde_json::to_string_pretty(v).unwrap());
}

fn paths_set(paths: &[String], changed: bool, root: &Path) -> Result<Option<HashSet<String>>> {
    let mut set: HashSet<String> = paths.iter().map(|p| p.trim_start_matches("./").to_string()).collect();
    if changed {
        set.extend(ops::changed_files(root)?);
        if set.is_empty() {
            // nothing changed: report nothing rather than everything
            return Ok(Some(HashSet::new()));
        }
    }
    Ok(if set.is_empty() { None } else { Some(set) })
}

fn run(cli: Cli) -> Result<i32> {
    let json = cli.json;
    match cli.cmd {
        Cmd::Init { path, name, purpose, pack, jurisdiction, force } => {
            let root = path.unwrap_or(std::env::current_dir()?);
            let rep = scaffold::init(&root, &name, &purpose, &pack, &jurisdiction, force)?;
            // generate an index so the commonplace lints clean from the start
            let v = Vault::open(&root)?;
            let _ = index::generate(&v);
            if json {
                print_json(&rep);
            } else {
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
                    let j = config::Config::load(&root).map(|c| c.jurisdiction).unwrap_or_default();
                    if j.is_empty() {
                        println!("  legal pack: no jurisdiction set (generic citation patterns). Set one: `commonplace pack install legal --jurisdiction {}`", scaffold::known_jurisdictions("legal").join("|"));
                    } else {
                        println!("  legal pack: jurisdiction {j}");
                    }
                }
                println!("Next: read AGENTS.md; `git init` is strongly recommended.");
            }
            Ok(0)
        }
        Cmd::Doctor => {
            let root = find_root(&cli.root).ok();
            let conv = sources::available_converters();
            let git = sources::which("git");
            let info = serde_json::json!({
                "version": commonplace_core::VERSION,
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "root": root.as_ref().map(|r| r.display().to_string()),
                "git": git,
                "git_repo": root.as_ref().map(|r| r.join(".git").exists()).unwrap_or(false),
                "converters": conv,
                "skill_dir": skill_dir().map(|p| p.display().to_string()),
                "packs_enabled": root.as_ref().and_then(|r| config::Config::load(r).ok()).map(|c| c.packs).unwrap_or_default(),
                "packs_known": scaffold::known_packs(),
                "jurisdiction": root.as_ref().and_then(|r| config::Config::load(r).ok()).map(|c| c.jurisdiction).unwrap_or_default(),
            });
            if json {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                println!("commonplace {} on {}/{}", commonplace_core::VERSION, std::env::consts::OS, std::env::consts::ARCH);
                println!("root:       {}", info["root"].as_str().unwrap_or("(none found: run `commonplace init` or pass --root)"));
                println!("git:        {}{}", if git { "available" } else { "missing" }, if info["git_repo"].as_bool().unwrap_or(false) { ", commonplace is a repo" } else { "" });
                println!("converters: {}", if info["converters"].as_array().map(|a| a.is_empty()).unwrap_or(true) { "none (PDF/DOCX will need manual conversion)".to_string() } else { info["converters"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect::<Vec<_>>().join(", ") });
                println!("packs:      enabled {:?}; known {:?}", info["packs_enabled"], info["packs_known"]);
                if info["packs_enabled"].as_array().map(|a| a.iter().any(|p| p == "legal")).unwrap_or(false) {
                    let j = info["jurisdiction"].as_str().unwrap_or("");
                    println!("legal:      jurisdiction {}", if j.is_empty() { "NOT SET (generic patterns; `commonplace pack install legal --jurisdiction za|au|en`)".to_string() } else { j.to_string() });
                }
                println!("skill dir:  {}", info["skill_dir"].as_str().unwrap_or("(not set; pass COMMONPLACE_SKILL_DIR for persona lookup)"));
            }
            Ok(0)
        }
        Cmd::Status => {
            let v = open(&cli.root)?;
            let s = ops::status(&v)?;
            if json {
                print_json(&s);
            } else {
                println!("{} — {} pages, {} sources ({} unconverted, {} not yet summarised), {} citations", s.name, s.pages, s.sources_total, s.sources_unconverted, s.sources_unsummarised, s.citations_total);
                println!("active projects: {}", if s.active_projects.is_empty() { "none".into() } else { s.active_projects.join(", ") });
                if !s.projects_without_next_action.is_empty() {
                    println!("  without next action: {}", s.projects_without_next_action.join(", "));
                }
                println!("inbox: {} · open questions: {} · open contradictions: {} · last journal: {}", s.inbox_items, s.open_questions, s.open_contradictions, s.journal_last.as_deref().unwrap_or("none"));
                println!("by type: {}", s.by_type.iter().map(|(k, n)| format!("{k} {n}")).collect::<Vec<_>>().join(", "));
                println!("packs: {:?} · last lint: {}", s.packs, s.last_lint.as_deref().unwrap_or("never"));
            }
            Ok(0)
        }
        Cmd::Ingest { files, title, date, kind, origin, project, confidentiality, authority, converter, r#move, force } => {
            let root = find_root(&cli.root)?;
            let cfg = config::Config::load(&root)?;
            if files.is_empty() {
                return Err(anyhow!("give at least one file"));
            }
            let mut reports = vec![];
            for f in &files {
                let opts = sources::IngestOptions { title: if files.len() == 1 { title.clone() } else { None }, date: date.clone(), kind: kind.clone(), origin: origin.clone(), project: project.clone(), confidentiality: confidentiality.clone(), authority: authority.clone(), converter: converter.clone(), move_file: r#move, force };
                let rep = sources::ingest(&root, &cfg, f, &opts)?;
                if !json {
                    if let Some(d) = &rep.duplicate_of {
                        println!("{}: already ingested as [[{d}]]", f.display());
                    } else {
                        println!("ingested [[{}]]\n  raw:  {}\n  text: {}\n  note: {}\n  converter: {} (fidelity {}{})", rep.id, rep.raw, rep.text.as_deref().unwrap_or("(none — needs conversion)"), rep.note, rep.converter, rep.fidelity, rep.pages.map(|p| format!(", {p} pages")).unwrap_or_default());
                    }
                    for w in &rep.warnings {
                        println!("  warning: {w}");
                    }
                }
                reports.push(rep);
            }
            if json {
                print_json(&reports);
            } else {
                println!("Next: read the text layer, write the note's TL;DR / summary / key quotes, set status: summarised, then integrate into concept and project pages.");
            }
            Ok(0)
        }
        Cmd::Source { cmd } => {
            let root = find_root(&cli.root)?;
            match cmd {
                SourceCmd::Verify => {
                    let res = sources::verify_integrity(&root)?;
                    let bad = res.iter().any(|r| r.raw_ok == Some(false) || r.raw_ok.is_none());
                    if json {
                        print_json(&res);
                    } else {
                        for r in &res {
                            println!("{} {}", if r.message == "ok" { "ok   " } else { "FAIL " }, format!("{}: {}", r.id, r.message));
                        }
                        println!("{} sources checked", res.len());
                    }
                    Ok(if bad { 3 } else { 0 })
                }
                SourceCmd::AttachText { id, converter, fidelity } => {
                    let rel = sources::attach_text(&root, &id, &converter, &fidelity)?;
                    if json {
                        print_json(&serde_json::json!({"id": id, "text": rel}));
                    } else {
                        println!("attached {rel} to [[{id}]] (converter {converter}, fidelity {fidelity})");
                    }
                    Ok(0)
                }
                SourceCmd::List => {
                    let v = Vault::open(&root)?;
                    let rows: Vec<serde_json::Value> = v.pages_of_type("source").map(|p| serde_json::json!({"id": p.stem, "status": p.get("status").unwrap_or_default(), "kind": p.get("kind").unwrap_or_default(), "fidelity": p.get("fidelity").unwrap_or_default(), "confidentiality": p.get("confidentiality").unwrap_or_default(), "title": p.title})).collect();
                    if json {
                        print_json(&rows);
                    } else {
                        for r in &rows {
                            println!("{:<12} {:<10} {:<8} {}", r["status"].as_str().unwrap_or(""), r["kind"].as_str().unwrap_or(""), r["fidelity"].as_str().unwrap_or(""), r["id"].as_str().unwrap_or(""));
                        }
                    }
                    Ok(0)
                }
                SourceCmd::Relink { id, all } => {
                    let ids: Vec<String> = if all {
                        let mut v: Vec<String> = commonplace_core::manifest::Manifest::load(&root)?.sources.keys().cloned().collect();
                        v.sort();
                        v
                    } else {
                        vec![id.ok_or_else(|| anyhow!("give a source id or --all"))?]
                    };
                    let mut reps = vec![];
                    for id in ids {
                        reps.push(sources::relink(&root, &id)?);
                    }
                    if json {
                        print_json(&reps);
                    } else {
                        for r in &reps {
                            match &r.skipped {
                                Some(s) => println!("skip  {}: {s}", r.id),
                                None if r.links_rewritten == 0 => println!("clean {}", r.id),
                                None => println!("ok    {}: {} links rewritten in {}", r.id, r.links_rewritten, r.files_changed.join(", ")),
                            }
                        }
                        let total: usize = reps.iter().map(|r| r.links_rewritten).sum();
                        println!("{} sources, {} links rewritten; run `commonplace cite verify` to confirm quotes still match", reps.len(), total);
                    }
                    Ok(0)
                }
                SourceCmd::Show { id, text, at } => {
                    if text || at.is_some() {
                        let tl = sources::load_text_layer(&root, &id)?.ok_or_else(|| anyhow!("no text layer for '{id}'"))?;
                        match at {
                            Some(loc) => {
                                let l = cite::parse_locator(&loc);
                                let sp = cite::span(&tl.text, &l).ok_or_else(|| anyhow!("locator '{loc}' not found in the text layer"))?;
                                for (ln, line) in sp {
                                    println!("{:>5} | {}", ln, line);
                                }
                            }
                            None => print!("{}", tl.text),
                        }
                    } else {
                        let p = root.join(sources::NOTES_DIR).join(format!("{id}.md"));
                        print!("{}", std::fs::read_to_string(&p).map_err(|_| anyhow!("no source note at {}", p.display()))?);
                    }
                    Ok(0)
                }
            }
        }
        Cmd::New { r#type, title, folder, set } => {
            let root = find_root(&cli.root)?;
            let cfg = config::Config::load(&root)?;
            let extra: Vec<(String, String)> = set.iter().filter_map(|s| s.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))).collect();
            let rep = scaffold::new_page(&root, &cfg, &r#type, &title, folder.as_deref(), &extra)?;
            if json {
                print_json(&rep);
            } else {
                println!("created {} ({})", rep.path, rep.ptype);
            }
            Ok(0)
        }
        Cmd::Lint { changed, rule, no_cite, fail_on, paths } => {
            let v = open(&cli.root)?;
            let packs = load_packs(&v)?;
            let only = paths_set(&paths, changed, &v.root)?;
            if changed && only.as_ref().map(|s| s.is_empty()).unwrap_or(false) {
                if json {
                    print_json(&lint::LintReport::default());
                } else {
                    println!("nothing changed since the last commit/lint");
                }
                return Ok(0);
            }
            let opts = lint::LintOptions { only, verify_citations: !no_cite, rules: if rule.is_empty() { None } else { Some(rule) } };
            let rep = lint::lint(&v, &packs, &opts);
            let mut m = commonplace_core::manifest::Manifest::load(&v.root)?;
            m.last_lint = Some(commonplace_core::fsutil::now_iso());
            m.save(&v.root)?;
            if json {
                print_json(&rep);
            } else {
                println!("lint: {} pages — {} errors, {} warnings, {} info", rep.pages_checked, rep.errors, rep.warnings, rep.infos);
                for f in &rep.findings {
                    println!("  [{:<7}] {:<9} {}{}: {}", format!("{:?}", f.severity).to_lowercase(), f.rule, f.path, f.line.map(|l| format!(":{l}")).unwrap_or_default(), f.message);
                }
            }
            let fail = match fail_on.as_str() {
                "warning" | "warn" => rep.errors + rep.warnings > 0,
                _ => rep.errors > 0,
            };
            Ok(if fail { 1 } else { 0 })
        }
        Cmd::Cite { cmd } => {
            let v = open(&cli.root)?;
            match cmd {
                CiteCmd::Verify { changed, paths } => {
                    let only = paths_set(&paths, changed, &v.root)?;
                    let res = lint::verify_all_citations(&v, only.as_ref());
                    let fails = res.iter().filter(|r| matches!(r.verdict, cite::Verdict::QuoteNotFound | cite::Verdict::QuoteNotInSpan | cite::Verdict::SourceMissing)).count();
                    if json {
                        print_json(&res);
                    } else {
                        for r in &res {
                            let tag = match r.verdict {
                                cite::Verdict::Pass => "PASS ",
                                cite::Verdict::QuoteNotFound | cite::Verdict::QuoteNotInSpan | cite::Verdict::SourceMissing => "FAIL ",
                                _ => "WARN ",
                            };
                            println!("{tag} {}:{} [^{}] → [[{}]] {} — {}", r.page, r.line, r.label, r.source, r.locator, r.message);
                        }
                        println!("{} citations, {} failed", res.len(), fails);
                    }
                    Ok(if fails > 0 { 1 } else { 0 })
                }
                CiteCmd::Find { source, quote } => {
                    let tl = sources::load_text_layer(&v.root, &source)?.ok_or_else(|| anyhow!("no text layer for '{source}'"))?;
                    let lines: Vec<(usize, &str)> = tl.text.lines().enumerate().map(|(i, l)| (i + 1, l)).collect();
                    let hay = cite::Haystack::new(&lines);
                    match hay.verify(&quote) {
                        Some(l) => {
                            // derive page / para context
                            let mut page = None;
                            let mut para = None;
                            for (ln, line) in &lines {
                                if *ln > l {
                                    break;
                                }
                                if let Some(c) = regex::Regex::new(r"<!--\s*page:\s*(\d+)\s*-->").unwrap().captures(line) {
                                    page = Some(c[1].to_string());
                                }
                                if let Some(c) = regex::Regex::new(r"^(?:<!--\s*para:\s*(\d+)\s*-->|\[(\d+)\]\s)").unwrap().captures(line) {
                                    para = c.get(1).or(c.get(2)).map(|m| m.as_str().to_string());
                                }
                            }
                            let loc = if let Some(p) = &para { format!("para {p}") } else if let Some(p) = &page { format!("p.{p}") } else { tl.locate(l) };
                            if json {
                                print_json(&serde_json::json!({"found": true, "line": l, "where": tl.locate(l), "page": page, "para": para, "locator": loc}));
                            } else {
                                println!("found at {} → use locator: {loc}\n[^n]: [[{source}]] {loc} \"{quote}\"", tl.locate(l));
                            }
                            Ok(0)
                        }
                        None => {
                            let near = hay.nearest_lines(&quote, 5);
                            if json {
                                print_json(&serde_json::json!({"found": false, "nearest_lines": near}));
                            } else {
                                println!("not found verbatim. Nearest lines: {}", near.iter().map(|l| format!("L{l}")).collect::<Vec<_>>().join(", "));
                                for l in near.iter().take(3) {
                                    if let Some((_, line)) = lines.get(l - 1) {
                                        println!("  L{l}: {}", line.trim());
                                    }
                                }
                            }
                            Ok(1)
                        }
                    }
                }
            }
        }
        Cmd::Links { cmd } => {
            let v = open(&cli.root)?;
            match cmd {
                LinksCmd::Check => {
                    let issues = ops::broken_links(&v);
                    if json {
                        print_json(&issues);
                    } else {
                        for i in &issues {
                            println!("{}{}: [[{}]]{}", i.page, i.line.map(|l| format!(":{l}")).unwrap_or_default(), i.target, if i.suggestions.is_empty() { String::new() } else { format!(" → did you mean {}?", i.suggestions.iter().map(|s| format!("[[{s}]]")).collect::<Vec<_>>().join(", ")) });
                        }
                        println!("{} broken links", issues.len());
                    }
                    Ok(if issues.is_empty() { 0 } else { 1 })
                }
                LinksCmd::Backlinks { page } => {
                    let b = ops::backlinks_of(&v, &page)?;
                    if json {
                        print_json(&b);
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
        Cmd::Dedupe { threshold } => {
            let v = open(&cli.root)?;
            let d = ops::dedupe(&v, threshold);
            if json {
                print_json(&d);
            } else {
                for c in &d {
                    println!("{:.2}  {}  ~  {}  ({})", c.similarity, c.a, c.b, c.reason);
                }
                println!("{} candidate pairs (judge them; merge with `commonplace rename` + edits, never automatically)", d.len());
            }
            Ok(0)
        }
        Cmd::Index => {
            let v = open(&cli.root)?;
            let rep = index::generate(&v)?;
            let mut m = commonplace_core::manifest::Manifest::load(&v.root)?;
            m.last_index = Some(commonplace_core::fsutil::now_iso());
            m.save(&v.root)?;
            if json {
                print_json(&rep);
            } else {
                println!("index.md regenerated ({} lines); hubs: {}; page lists refreshed in {} pages", rep.index_lines, if rep.hubs.is_empty() { "none".into() } else { rep.hubs.join(", ") }, rep.mocs_updated.len());
            }
            Ok(0)
        }
        Cmd::Search { query, scope, include_archive, text, ptype, limit } => {
            let v = open(&cli.root)?;
            let q = query.join(" ");
            let hits = search::search(&v, &q, &search::SearchOptions { scope, include_archive, include_text_layer: text, ptype, limit });
            if json {
                print_json(&hits);
            } else {
                for h in &hits {
                    println!("{:>6.2}  {} [{}]{}\n        {}", h.score, h.rel, h.ptype, h.line.map(|l| format!(" L{l}")).unwrap_or_default(), h.snippet);
                }
                if hits.is_empty() {
                    println!("no hits");
                }
            }
            Ok(0)
        }
        Cmd::Query { r#where, fields, format, scope, include_archive } => {
            let v = open(&cli.root)?;
            let expr = commonplace_core::query::parse(&r#where)?;
            let pages: Vec<&commonplace_core::vault::Page> = v.pages.iter().filter(|p| (include_archive || !p.is_archived()) && scope.as_ref().map(|s| p.rel.starts_with(s.trim_end_matches('/'))).unwrap_or(true) && commonplace_core::query::eval(&expr, p)).collect();
            let fields: Vec<String> = if fields.is_empty() { vec!["type".into(), "status".into(), "description".into()] } else { fields };
            if json || format == "json" {
                let rows: Vec<serde_json::Value> = pages.iter().map(|p| {
                    let mut m = serde_json::Map::new();
                    m.insert("page".into(), serde_json::Value::String(p.stem.clone()));
                    m.insert("path".into(), serde_json::Value::String(p.rel.clone()));
                    for f in &fields {
                        let val = commonplace_core::query::field_value(p, f).map(|x| commonplace_core::frontmatter::scalar_to_string(&x)).unwrap_or_default();
                        m.insert(f.clone(), serde_json::Value::String(val));
                    }
                    serde_json::Value::Object(m)
                }).collect();
                print_json(&rows);
            } else if format == "list" {
                for p in &pages {
                    println!("- [[{}]]", p.stem);
                }
            } else {
                println!("| page | {} |", fields.join(" | "));
                println!("|---|{}", "---|".repeat(fields.len()));
                for p in &pages {
                    let cells: Vec<String> = fields.iter().map(|f| commonplace_core::query::field_value(p, f).map(|x| commonplace_core::frontmatter::scalar_to_string(&x)).unwrap_or_default().replace('|', "\\|")).collect();
                    println!("| [[{}]] | {} |", p.stem, cells.join(" | "));
                }
                eprintln!("{} pages", pages.len());
            }
            Ok(0)
        }
        Cmd::Review { cmd } => {
            let v = open(&cli.root)?;
            match cmd {
                ReviewCmd::Prepare { persona, targets, out } => {
                    let text = scaffold::persona_text(&v.root, skill_dir().as_deref(), &v.config, &persona)?;
                    let packet = ops::review_packet(&v, &text, &persona, &targets, v.config.review.context_lines)?;
                    let out = out.unwrap_or_else(|| commonplace_core::vault::tool_dir(&v.root).join("reviews").join(format!("{} {} {} packet.md", commonplace_core::fsutil::today(), persona, commonplace_core::fsutil::slug(&targets.join(" "), 40))));
                    commonplace_core::fsutil::write_atomic(&out, packet.as_bytes())?;
                    if json {
                        print_json(&serde_json::json!({"packet": out.display().to_string(), "persona": persona, "pages": targets}));
                    } else {
                        println!("packet written to {}\nGive it to a fresh-context subagent (ideally another model). It must return a JSON array of findings; then run:\n  commonplace review record {persona} <findings.json> --scope \"{}\"", out.display(), targets.join(","));
                    }
                    Ok(0)
                }
                ReviewCmd::Record { persona, findings, scope, model } => {
                    let fj = std::fs::read_to_string(&findings)?;
                    let rep = ops::review_record(&v, &persona, &fj, &scope, model.as_deref())?;
                    if json {
                        print_json(&rep);
                    } else {
                        println!("recorded {} finding(s) ({} blockers) on {} page(s); see reviews/", rep.findings, rep.blockers, rep.pages_updated.len());
                    }
                    Ok(if rep.blockers > 0 { 1 } else { 0 })
                }
            }
        }
        Cmd::Rename { old, new } => {
            let v = open(&cli.root)?;
            let rep = ops::rename(&v, &old, &new)?;
            if json {
                print_json(&rep);
            } else {
                println!("renamed {} → {} ({} links rewritten in {} files; old name kept as alias)", rep.from, rep.to, rep.links_rewritten, rep.files_updated.len());
            }
            Ok(0)
        }
        Cmd::Archive { page } => {
            let v = open(&cli.root)?;
            let moved = ops::archive(&v, &page)?;
            if json {
                print_json(&moved);
            } else {
                println!("archived {} file(s). Promote any reusable concepts to resources/ and run `commonplace index`.", moved.len());
            }
            Ok(0)
        }
        Cmd::Export { targets, out, include_confidential } => {
            let v = open(&cli.root)?;
            let text = ops::export(&v, &targets, include_confidential)?;
            match out {
                Some(o) => {
                    commonplace_core::fsutil::write_atomic(&o, text.as_bytes())?;
                    if !json {
                        println!("wrote {}", o.display());
                    }
                }
                None => print!("{text}"),
            }
            Ok(0)
        }
        Cmd::Trace { chain, name, format } => {
            let v = open(&cli.root)?;
            let packs = load_packs(&v)?;
            let chain: Vec<String> = match name {
                Some(n) => ops::pack_trace(&packs, &n).ok_or_else(|| anyhow!("no pack trace named '{n}' (available: {})", packs.iter().flat_map(|p| p.traces.iter().map(|t| t.name.clone())).collect::<Vec<_>>().join(", ")))?.chain.clone(),
                None => chain,
            };
            let (header, rows) = ops::trace(&v, &chain)?;
            if json || format == "json" {
                print_json(&serde_json::json!({"columns": header, "rows": rows.iter().map(|r| r.cells.clone()).collect::<Vec<_>>()}));
            } else if format == "csv" {
                println!("{}", header.join(","));
                for r in rows {
                    println!("{}", r.cells.join(","));
                }
            } else {
                println!("| {} |", header.join(" | "));
                println!("|{}", "---|".repeat(header.len()));
                let gaps = rows.iter().filter(|r| r.cells.contains(&"—".to_string())).count();
                for r in &rows {
                    println!("| {} |", r.cells.iter().map(|c| if c == "—" { c.clone() } else { format!("[[{c}]]") }).collect::<Vec<_>>().join(" | "));
                }
                eprintln!("{} rows, {} with gaps", rows.len(), gaps);
            }
            Ok(0)
        }
        Cmd::Log { cmd } => {
            let root = find_root(&cli.root)?;
            match cmd {
                LogCmd::Add { op, title, notes } => {
                    ops::log_append(&root, &op, &title, &notes)?;
                    if !json {
                        println!("logged: [{}] {} | {}", commonplace_core::fsutil::today(), op, title);
                    }
                    Ok(0)
                }
                LogCmd::Rotate { keep } => {
                    let n = ops::log_rotate(&root, keep)?;
                    if !json {
                        println!("moved {n} entries to archive/");
                    }
                    Ok(0)
                }
                LogCmd::Show { n } => {
                    let t = std::fs::read_to_string(root.join("log.md")).unwrap_or_default();
                    let heads: Vec<&str> = t.lines().filter(|l| l.starts_with("## [")).collect();
                    for h in heads.iter().rev().take(n).rev() {
                        println!("{h}");
                    }
                    Ok(0)
                }
            }
        }
        Cmd::Graph { cmd } => {
            let v = open(&cli.root)?;
            match cmd {
                GraphCmd::Stats { mentions } => {
                    let g = commonplace_core::graph::build(&v, mentions);
                    let st = g.stats();
                    if json {
                        print_json(&st);
                    } else {
                        println!("{} nodes, {} edges, {} components (largest: {:?})", st.nodes, st.edges, st.components, st.largest_components);
                        println!("hubs: {}", st.hubs.iter().map(|(n, d)| format!("{n} ({d})")).collect::<Vec<_>>().join(", "));
                        if !st.isolated.is_empty() {
                            println!("isolated ({}): {}", st.isolated.len(), st.isolated.iter().take(20).cloned().collect::<Vec<_>>().join(", "));
                        }
                    }
                    Ok(0)
                }
                GraphCmd::Schema { mentions } => {
                    let g = commonplace_core::graph::build(&v, mentions);
                    let sc = g.schema();
                    if json {
                        print_json(&sc);
                    } else {
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
                    }
                    Ok(0)
                }
                GraphCmd::Walk { page, depth, rel, direction, mentions } => {
                    let g = commonplace_core::graph::build(&v, mentions);
                    if g.node(&page).is_none() {
                        return Err(anyhow!("no page named '{page}'"));
                    }
                    let rows = g.walk(&page, depth, &rel, &direction);
                    if json {
                        print_json(&rows);
                    } else {
                        for r in &rows {
                            println!("{} [[{}]] ({}) via {}{}", "  ".repeat(r.depth - 1), r.node, r.label, r.path, if r.description.is_empty() { String::new() } else { format!(" — {}", r.description) });
                        }
                        println!("{} nodes within {} hops", rows.len(), depth);
                    }
                    Ok(0)
                }
                GraphCmd::Path { from, to, max_depth, rel, direction, limit, mentions } => {
                    let g = commonplace_core::graph::build(&v, mentions);
                    for p in [&from, &to] {
                        if g.node(p).is_none() {
                            return Err(anyhow!("no page named '{p}'"));
                        }
                    }
                    let paths = g.paths(&from, &to, max_depth, &rel, &direction, limit);
                    if json {
                        print_json(&paths);
                    } else {
                        for p in &paths {
                            println!("{}", p.iter().map(|x| if x.starts_with('-') || x.starts_with('<') { x.clone() } else { format!("[[{x}]]") }).collect::<Vec<_>>().join(" "));
                        }
                        if paths.is_empty() {
                            println!("no path within {max_depth} hops{}", if mentions { String::new() } else { " (try --mentions to include plain wikilinks)".into() });
                        }
                    }
                    Ok(if paths.is_empty() { 1 } else { 0 })
                }
                GraphCmd::Related { page, limit, typed_only } => {
                    let g = commonplace_core::graph::build(&v, !typed_only);
                    let rows = commonplace_core::analysis::related(&g, &v, &page, limit).ok_or_else(|| anyhow!("no page named '{page}'"))?;
                    if json {
                        print_json(&rows);
                    } else {
                        for r in &rows {
                            println!("{:>5.2}  [[{}]] ({})", r.score, r.node, r.label);
                            for why in &r.reasons {
                                println!("         {why}");
                            }
                        }
                        println!("{} related pages", rows.len());
                    }
                    Ok(0)
                }
                GraphCmd::Hubs { limit, typed_only } => {
                    let g = commonplace_core::graph::build(&v, !typed_only);
                    let rows = commonplace_core::analysis::hubs(&g, limit);
                    if json {
                        print_json(&rows);
                    } else {
                        println!("{:<8} {:<6} {:<8} page", "pagerank", "degree", "between");
                        for r in &rows {
                            println!("{:<8.4} {:<6} {:<8} [[{}]] ({})", r.pagerank, r.degree, r.betweenness, r.node, r.label);
                        }
                    }
                    Ok(0)
                }
                GraphCmd::Communities { min_size, typed_only } => {
                    let g = commonplace_core::graph::build(&v, !typed_only);
                    let rows = commonplace_core::analysis::communities(&g, min_size);
                    if json {
                        print_json(&rows);
                    } else {
                        for c in &rows {
                            println!("community {} ({} pages; {}; {} sources cited; hub: {})", c.id, c.size, c.types.iter().map(|(t, n)| format!("{n} {t}")).collect::<Vec<_>>().join(", "), c.sources_cited, c.hub.clone().unwrap_or_else(|| "NONE".into()));
                            println!("    {}", c.members.iter().map(|m| format!("[[{m}]]")).collect::<Vec<_>>().join(", "));
                        }
                        println!("{} communities of {}+ pages", rows.len(), min_size);
                    }
                    Ok(0)
                }
                GraphCmd::Gaps { typed_only } => {
                    let g = commonplace_core::graph::build(&v, !typed_only);
                    let gp = commonplace_core::analysis::gaps(&g, &v);
                    if json {
                        print_json(&gp);
                    } else {
                        let list = |title: &str, items: &[String]| {
                            if !items.is_empty() {
                                println!("{title} ({}):", items.len());
                                for i in items.iter().take(30) {
                                    println!("  [[{i}]]");
                                }
                            }
                        };
                        if !gp.wanted.is_empty() {
                            println!("Wanted pages (linked but missing) ({}):", gp.wanted.len());
                            for w in gp.wanted.iter().take(30) {
                                println!("  [[{}]] wanted by {} page{}: {}{}", w.target, w.referrers.len(), if w.referrers.len() == 1 { "" } else { "s" }, w.referrers.iter().take(4).cloned().collect::<Vec<_>>().join(", "), w.suggestion.as_ref().map(|s| format!("  (did you mean [[{s}]]?)")).unwrap_or_default());
                            }
                        }
                        list("Orphans (no inbound links)", &gp.orphans);
                        list("Dead ends (no outbound links)", &gp.dead_ends);
                        list("Sources no page cites", &gp.uncited_sources);
                        if !gp.communities_without_hub.is_empty() {
                            println!("Clusters without a topic/project/area page ({}):", gp.communities_without_hub.len());
                            for c in &gp.communities_without_hub {
                                println!("  {} pages: {}", c.size, c.members.iter().map(|m| format!("[[{m}]]")).collect::<Vec<_>>().join(", "));
                            }
                        }
                        if !gp.single_source_communities.is_empty() {
                            println!("Clusters resting on one source or none ({}):", gp.single_source_communities.len());
                            for c in &gp.single_source_communities {
                                println!("  {} pages, {} source(s): {}", c.size, c.sources_cited, c.members.iter().take(5).map(|m| format!("[[{m}]]")).collect::<Vec<_>>().join(", "));
                            }
                        }
                        if !gp.bridges.is_empty() {
                            println!("Bridges (pages that connect clusters; review first):");
                            for b in &gp.bridges {
                                println!("  [[{}]] betweenness {}", b.node, b.betweenness);
                            }
                        }
                        let total = gp.wanted.len() + gp.orphans.len() + gp.dead_ends.len() + gp.uncited_sources.len() + gp.communities_without_hub.len() + gp.single_source_communities.len();
                        println!("{total} gap items");
                    }
                    Ok(0)
                }
                GraphCmd::SuggestLinks { page, limit } => {
                    let g = commonplace_core::graph::build(&v, true);
                    let rows = commonplace_core::analysis::suggest_links(&g, &v, page.as_deref(), limit);
                    if json {
                        print_json(&rows);
                    } else {
                        for r in &rows {
                            println!("{}:{}  \"{}\" → [[{}]]", r.path, r.line, r.matched, r.target);
                            println!("    {}", r.context);
                        }
                        println!("{} unlinked mentions", rows.len());
                    }
                    Ok(0)
                }
                GraphCmd::Conflicts { limit } => {
                    let rows = commonplace_core::analysis::conflicts(&v, limit);
                    if json {
                        print_json(&rows);
                    } else {
                        for r in &rows {
                            println!("{:>5.2}  [[{}]] vs [[{}]] ({})", r.score, r.a, r.b, r.ptype);
                            println!("         {}", r.reasons.join("; "));
                        }
                        println!("{} candidate pairs (the tool nominates; you judge)", rows.len());
                    }
                    Ok(0)
                }
                GraphCmd::Export { format, out, mentions, around, depth } => {
                    let g = commonplace_core::graph::build(&v, mentions);
                    let text = match format.as_str() {
                        "cypher" => g.to_cypher(),
                        "graphml" => g.to_graphml(),
                        "dot" => g.to_dot(),
                        "json" => serde_json::to_string_pretty(&g)?,
                        "mermaid" => {
                            let ids = around.as_ref().map(|a| {
                                let mut set: HashSet<String> = g.walk(a, depth, &[], "both").into_iter().map(|r| r.node.to_lowercase()).collect();
                                set.insert(a.to_lowercase());
                                set
                            });
                            g.to_mermaid(ids.as_ref())
                        }
                        other => return Err(anyhow!("unknown format '{other}' (cypher|graphml|mermaid|dot|json)")),
                    };
                    match out {
                        Some(o) => {
                            commonplace_core::fsutil::write_atomic(&o, text.as_bytes())?;
                            if !json {
                                println!("wrote {} ({} nodes, {} edges)", o.display(), g.nodes.len(), g.edges.len());
                            }
                        }
                        None => print!("{text}"),
                    }
                    Ok(0)
                }
            }
        }
        Cmd::Pack { cmd } => match cmd {
            PackCmd::List => {
                let root = find_root(&cli.root).ok();
                let enabled = root.as_ref().and_then(|r| config::Config::load(r).ok()).map(|c| c.packs).unwrap_or_default();
                let jur = root.as_ref().and_then(|r| config::Config::load(r).ok()).map(|c| c.jurisdiction).unwrap_or_default();
                for p in scaffold::known_packs() {
                    let extra = if p == "legal" { format!("  (jurisdictions: {}; current: {})", scaffold::known_jurisdictions("legal").join(", "), if jur.is_empty() { "generic" } else { jur.as_str() }) } else { String::new() };
                    println!("{} {}{}", if enabled.contains(&p) { "[x]" } else { "[ ]" }, p, extra);
                }
                Ok(0)
            }
            PackCmd::Install { name, jurisdiction } => {
                let root = find_root(&cli.root)?;
                let mut cfg = config::Config::load(&root)?;
                let (name, alias_jur) = scaffold::normalise_pack(&name);
                if !scaffold::known_packs().contains(&name) {
                    return Err(anyhow!("unknown pack '{name}'; known: {}", scaffold::known_packs().join(", ")));
                }
                scaffold::install_pack(&root, &name)?;
                if !cfg.packs.contains(&name) {
                    cfg.packs.push(name.clone());
                }
                if let Some(j) = jurisdiction.as_deref().or(alias_jur) {
                    let known = scaffold::known_jurisdictions("legal");
                    if !known.iter().any(|k| k == j) && !commonplace_core::vault::tool_dir(&root).join("packs/legal/jurisdictions").join(format!("{j}.yaml")).is_file() {
                        return Err(anyhow!("unknown jurisdiction '{j}'; shipped profiles: {} (or add .commonplace/packs/legal/jurisdictions/{j}.yaml)", known.join(", ")));
                    }
                    cfg.jurisdiction = j.to_string();
                }
                cfg.save(&root)?;
                // prove the pack loads with this jurisdiction
                pack::load_packs(&commonplace_core::vault::tool_dir(&root).join("packs"), std::slice::from_ref(&name), &cfg.jurisdiction)?;
                if !json {
                    println!("pack '{name}' installed and enabled; read its workflow in the skill's packs/{name}/ folder");
                    if name == "legal" {
                        if cfg.jurisdiction.is_empty() {
                            println!("no jurisdiction set: generic citation patterns apply. Set one with `commonplace pack install legal --jurisdiction {}`", scaffold::known_jurisdictions("legal").join("|"));
                        } else {
                            println!("jurisdiction: {} (profile .commonplace/packs/legal/jurisdictions/{}.yaml)", cfg.jurisdiction, cfg.jurisdiction);
                        }
                    }
                }
                Ok(0)
            }
        },
    }
}
