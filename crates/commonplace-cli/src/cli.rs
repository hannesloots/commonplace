//! The command-line interface definition.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "commonplace", version = commonplace_core::VERSION, about = "Deterministic tools for a cited markdown research wiki / second brain", long_about = None)]
pub(crate) struct Cli {
    /// Commonplace root (default: walk up from the current directory to find commonplace.toml)
    #[arg(long, global = true)]
    pub(crate) root: Option<PathBuf>,
    /// Machine-readable JSON output
    #[arg(long, global = true)]
    pub(crate) json: bool,
    #[command(subcommand)]
    pub(crate) cmd: Cmd,
}

#[derive(Subcommand)]
pub(crate) enum Cmd {
    /// Create a new commonplace (folders, AGENTS.md, templates, packs)
    Init(InitArgs),
    /// Report environment: root, converters, git, packs
    Doctor,
    /// Counts and pending work
    Status,
    /// Register a file as an immutable source, convert it, create its note
    Ingest(IngestArgs),
    /// Source-layer operations
    Source {
        #[command(subcommand)]
        cmd: SourceCmd,
    },
    /// Create a page from its template
    New(NewArgs),
    /// Deterministic hygiene checks (frontmatter, links, citations, packs)
    Lint(LintArgs),
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
    Search(SearchArgs),
    /// Frontmatter query, e.g. --where "type=capability and `target_maturity>baseline_maturity`"
    Query(QueryArgs),
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
    Export(ExportArgs),
    /// Traceability tables along typed links
    Trace(TraceArgs),
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

#[derive(Args)]
pub(crate) struct InitArgs {
    pub(crate) path: Option<PathBuf>,
    #[arg(long, default_value = "")]
    pub(crate) name: String,
    #[arg(long, default_value = "")]
    pub(crate) purpose: String,
    /// Comma-separated packs: ea, legal, compare, study
    #[arg(long, value_delimiter = ',')]
    pub(crate) pack: Vec<String>,
    /// Jurisdiction profile for the legal pack: za, au, en (or a code you add)
    #[arg(long, default_value = "")]
    pub(crate) jurisdiction: String,
    #[arg(long)]
    pub(crate) force: bool,
}

#[derive(Args)]
pub(crate) struct IngestArgs {
    pub(crate) files: Vec<PathBuf>,
    #[arg(long)]
    pub(crate) title: Option<String>,
    /// Publication date YYYY-MM-DD (default: today, or a date prefix in the title)
    #[arg(long)]
    pub(crate) date: Option<String>,
    #[arg(long)]
    pub(crate) kind: Option<String>,
    /// Where it came from (URL, "email from X", "scan")
    #[arg(long)]
    pub(crate) origin: Option<String>,
    #[arg(long)]
    pub(crate) project: Option<String>,
    /// public | personal | confidential | privileged
    #[arg(long)]
    pub(crate) confidentiality: Option<String>,
    /// primary | secondary | tertiary
    #[arg(long)]
    pub(crate) authority: Option<String>,
    /// Force a converter: pdftotext | pandoc | markitdown
    #[arg(long)]
    pub(crate) converter: Option<String>,
    /// Move the file instead of copying it
    #[arg(long)]
    pub(crate) r#move: bool,
    #[arg(long)]
    pub(crate) force: bool,
}

#[derive(Args)]
pub(crate) struct NewArgs {
    pub(crate) r#type: String,
    pub(crate) title: String,
    #[arg(long)]
    pub(crate) folder: Option<String>,
    /// Extra frontmatter: --set key=value (lists as "[a, b]")
    #[arg(long = "set", value_name = "KEY=VALUE")]
    pub(crate) set: Vec<String>,
}

#[derive(Args)]
pub(crate) struct LintArgs {
    /// Only report pages changed since the last commit / lint
    #[arg(long)]
    pub(crate) changed: bool,
    /// Only these rule prefixes (e.g. CITE, LINK-001)
    #[arg(long, value_delimiter = ',')]
    pub(crate) rule: Vec<String>,
    /// Skip citation verification (faster)
    #[arg(long)]
    pub(crate) no_cite: bool,
    /// Exit 1 at this severity or worse: error (default) | warning
    #[arg(long, default_value = "error")]
    pub(crate) fail_on: String,
    /// Restrict to these paths
    pub(crate) paths: Vec<String>,
}

#[derive(Args)]
pub(crate) struct SearchArgs {
    pub(crate) query: Vec<String>,
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) include_archive: bool,
    /// Also search source text layers
    #[arg(long)]
    pub(crate) text: bool,
    #[arg(long = "type")]
    pub(crate) ptype: Option<String>,
    #[arg(long, default_value_t = 15)]
    pub(crate) limit: usize,
}

#[derive(Args)]
pub(crate) struct QueryArgs {
    #[arg(long = "where")]
    pub(crate) r#where: String,
    /// Fields to show, comma-separated
    #[arg(long, value_delimiter = ',')]
    pub(crate) fields: Vec<String>,
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) include_archive: bool,
}

#[derive(Args)]
pub(crate) struct ExportArgs {
    pub(crate) targets: Vec<String>,
    #[arg(long)]
    pub(crate) out: Option<PathBuf>,
    #[arg(long)]
    pub(crate) include_confidential: bool,
}

#[derive(Args)]
pub(crate) struct TraceArgs {
    /// Chain: type,field,type[,field,type...]
    #[arg(long, value_delimiter = ',')]
    pub(crate) chain: Vec<String>,
    /// A trace declared by a pack
    #[arg(long)]
    pub(crate) name: Option<String>,
    #[arg(long, default_value = "md")]
    pub(crate) format: String,
}

#[derive(Subcommand)]
pub(crate) enum GraphCmd {
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
    Walk(WalkArgs),
    /// Shortest paths between two pages
    Path(PathArgs),
    /// Pages related to a page, with why (links, co-citation, shared neighbours, tags, random walk)
    Related {
        page: String,
        #[arg(long, default_value_t = 15)]
        limit: usize,
        /// Ignore plain wikilinks; use typed relations and citations only
        #[arg(long)]
        typed_only: bool,
    },
    /// Most central pages (`PageRank`, degree, betweenness)
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
    Export(GraphExportArgs),
}

#[derive(Args)]
pub(crate) struct WalkArgs {
    pub(crate) page: String,
    #[arg(long, default_value_t = 2)]
    pub(crate) depth: usize,
    /// Only follow these relations (comma-separated), e.g. `enabled_by,realised_by`
    #[arg(long, value_delimiter = ',')]
    pub(crate) rel: Vec<String>,
    /// out | in | both
    #[arg(long, default_value = "both")]
    pub(crate) direction: String,
    #[arg(long)]
    pub(crate) mentions: bool,
}

#[derive(Args)]
pub(crate) struct PathArgs {
    pub(crate) from: String,
    pub(crate) to: String,
    #[arg(long, default_value_t = 4)]
    pub(crate) max_depth: usize,
    #[arg(long, value_delimiter = ',')]
    pub(crate) rel: Vec<String>,
    #[arg(long, default_value = "both")]
    pub(crate) direction: String,
    #[arg(long, default_value_t = 5)]
    pub(crate) limit: usize,
    #[arg(long)]
    pub(crate) mentions: bool,
}

#[derive(Args)]
pub(crate) struct GraphExportArgs {
    #[arg(long, default_value = "cypher")]
    pub(crate) format: String,
    #[arg(long)]
    pub(crate) out: Option<PathBuf>,
    #[arg(long)]
    pub(crate) mentions: bool,
    /// Mermaid only: restrict to the neighbourhood of this page
    #[arg(long)]
    pub(crate) around: Option<String>,
    #[arg(long, default_value_t = 2)]
    pub(crate) depth: usize,
}

#[derive(Subcommand)]
pub(crate) enum SourceCmd {
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
pub(crate) enum CiteCmd {
    /// Verify every footnote citation against the source text layers
    Verify {
        #[arg(long)]
        changed: bool,
        paths: Vec<String>,
    },
    /// Find where a quote occurs in a source (to build a locator)
    Find { source: String, quote: String },
}

#[derive(Subcommand)]
pub(crate) enum LinksCmd {
    /// Broken links with suggestions
    Check,
    /// Pages linking to a page
    Backlinks { page: String },
}

#[derive(Subcommand)]
pub(crate) enum ReviewCmd {
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
pub(crate) enum LogCmd {
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
pub(crate) enum PackCmd {
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
