# `commonplace` command reference

Launcher: `scripts/commonplace` (POSIX) or `scripts/commonplace.ps1` (Windows). It picks
`bin/commonplace-<target>` for this OS/CPU, sets `COMMONPLACE_SKILL_DIR`, and execs. Add
`--json` to any command for machine-readable output. `--root <path>` overrides
root discovery (otherwise the tool walks up from the cwd to find `commonplace.toml`).

Exit codes: `0` ok · `1` findings at error level (lint, cite, links, review
blockers) · `2` usage error · `3` integrity violation (a raw original or text
layer changed since ingest: stop and tell the user).

## Contents

- Setup: init, doctor, status, pack
- Sources: ingest, source verify/attach-text/list/show
- Writing: new, rename, archive
- Checks: lint, cite verify/find, links, dedupe
- Navigation: index, search, query, trace
- Graph: stats, schema, walk, path, related, hubs, communities, gaps, suggest-links, conflicts, export
- Reviews and output: review prepare/record, export, log
- Manual mode

## Setup

| command | does |
|---|---|
| `commonplace init [path] --name N --purpose P [--pack a,b] [--force]` | scaffold folders, AGENTS.md, CLAUDE.md, TAGS.md, overview, log, error book, templates (`.commonplace/templates`), packs (`.commonplace/packs`), index. `--force` refreshes standard sections of AGENTS.md and keeps Local rules. Never overwrites pages. |
| `commonplace doctor` | version, OS/arch, root, git, available converters (docling, marker, markitdown, pandoc, pdftotext), packs |
| `commonplace status` | counts by type, active projects (and those without next actions), inbox items, unconverted/unsummarised sources, citations, open questions and contradictions |
| `commonplace pack list` / `commonplace pack install <name> [--jurisdiction <code>]` | packs: ea, legal, compare, study (`togaf` and `sa-legal` are accepted as aliases). `list` shows the legal pack's jurisdiction profiles (za, au, en) and the current one; `install legal --jurisdiction za` sets it. `sa-legal` still works as an alias for `legal` + `za` |

## Sources

| command | does |
|---|---|
| `commonplace ingest <file>... [--title T] [--date YYYY-MM-DD] [--kind K] [--origin URL] [--project P] [--confidentiality C] [--authority A] [--converter X] [--move] [--force]` | assigns id `YYYY-MM-DD Title`, copies to `sources/raw/` (read-only), sha256 into `.commonplace/manifest.json`, converts to `sources/text/<id>.text.md` (md/txt copy, html via built-in converter, pdf via pdftotext with `<!-- page:N -->` markers, docx via pandoc/markitdown), writes `sources/notes/<id>.md` from the template. Refuses byte-identical duplicates. If no converter can handle the file, the note is `status: registered` and the report says `needs_conversion`: convert it yourself and `attach-text`. |
| `commonplace source attach-text <id> [--converter X] [--fidelity high\|medium\|low]` | register a text layer you wrote or converted externally (`sources/text/<id>.text.md` or a folder `sources/text/<id>/NN Section.md`), hash it, update the note |
| `commonplace source verify` | re-hash every original and text layer; exit 3 on tampering |
| `commonplace source relink <id> \| --all` | rewrite relative web links (`/faq.htm`, `../img.svg`) in a source's text layer and note to absolute URLs against its recorded origin, then re-hash; line numbers do not move. Use after upgrading the tool on sources ingested from HTML earlier, or when Obsidian shows unresolved links under `sources/`. Run `commonplace cite verify` afterwards |
| `commonplace source list` | sources with status, kind, fidelity |
| `commonplace source show <id> [--text] [--at <locator>]` | print the note, the text layer, or just the span a locator names (`p.12`, `para 26`, `§3.2`, `L100-140`) |

Kinds: document, judgment, statute, article, paper, book, brochure, slides,
dataset, web, email, transcript, image, note, other. Authority: primary (the
thing itself), secondary (reporting on it), tertiary (aggregators). Confidentiality:
public, personal (default), confidential, privileged.

## Writing

| command | does |
|---|---|
| `commonplace new <type> "<Title>" [--folder F] [--set k=v ...]` | create a page from its template in the default folder for its type (or `--folder`); checks the title is unique across the commonplace; `--set tags="[topic/law]"` sets list fields |
| `commonplace rename "<Old>" "<New>"` | rename the file and rewrite every `[[link]]`; keeps the old name as an alias |
| `commonplace archive "<Project or Area>"` | move its folder to `archive/` and mark it archived |

Core types: concept, entity, synthesis, topic, playbook, project, area,
decision, person, journal, capture, question. Pack types come from enabled packs.

## Checks

| command | does |
|---|---|
| `commonplace lint [--changed] [--rule PREFIX,...] [--no-cite] [--fail-on error\|warning] [paths...]` | all deterministic rules (below) plus pack rules; `--changed` limits reporting to files changed since the last commit (git) or last lint (mtime) |
| `commonplace cite verify [--changed] [paths...]` | every footnote citation: source exists, span found, quote verbatim inside the span (after normalising quotes, dashes, whitespace, case, hyphenation; `...` joins segments) |
| `commonplace cite find "<source id>" "<quote>"` | where a quote occurs and the locator to use |
| `commonplace links check` / `commonplace links backlinks "<Page>"` | broken links with suggestions / inbound links |
| `commonplace dedupe [--threshold 0.86]` | candidate duplicate pages by name/alias similarity and shared sources; never merges automatically |

Rule families: FM (frontmatter), TAG, LINK (001 broken, 002 ambiguous, 003
orphan, 004 wrong target type), SRC (raw/text/manifest), CITE (001 unknown
source, 002 cites a wiki page, 003 quote not found, 004 quote outside span, 005
locator unknown, 006 no quote, 007/008 quote length, 009 no text layer, 010/011
footnote ref/def mismatch, 012 unsourced page, 013 sources list broken), DUP,
CON (contradictions), SUP (supersession), STALE, SIZE, TERM (retired terms),
SEC (secrets), PRIV (confidentiality flow), PROJ, DATE, LOG, IDX, EMPTY, INBOX,
and pack rules (TOG-, LEG-, CMP-, STU-). Silence a rule in `commonplace.toml`
`[lint] disabled_rules`.

## Navigation

| command | does |
|---|---|
| `commonplace index` | regenerate `index.md` (orientation block kept), hubs in `indexes/` for large groups, and the `<!-- commonplace:pages -->` lists inside project, area and topic pages |
| `commonplace search <terms> [--scope folder] [--type T] [--text] [--include-archive] [--limit N]` | BM25 over titles, aliases, tags, descriptions, bodies; `--text` adds source text layers |
| `commonplace query --where "<expr>" [--fields a,b] [--format table\|list\|json] [--scope F]` | frontmatter query: `type=capability and target_maturity>baseline_maturity`, `tags has topic/law`, `status!=superseded`, `verified exists`, `folder~projects/x`; `@field` compares two fields |
| `commonplace trace --chain type,field,type[,field,type] \| --name <pack trace> [--format md\|csv\|json]` | traceability table along typed link fields, gaps shown as `—` |

## Graph

The commonplace is a typed graph: nodes are pages (label = `type`), edges come from
frontmatter link fields (relation = field name, `sources` → `cites`), inline
`field:: [[Page]]` lines, citations (`cites`) and plain wikilinks (`mentions`).
Everything is computed in memory on each run (milliseconds at thousands of
pages); nothing to install. Index, log, review and question pages are not nodes.
`--mentions` includes plain wikilinks on the traversal commands; the analysis
commands include them by default (`--typed-only` to exclude).

| command | does |
|---|---|
| `commonplace graph stats [--mentions]` | node/edge counts, components, top hubs by degree, isolated pages |
| `commonplace graph schema [--mentions]` | the ontology: node types with counts, relation types with from→to type pairs |
| `commonplace graph walk "<page>" [--depth 2] [--rel a,b] [--direction out\|in\|both] [--mentions]` | everything reachable within N hops, with the path |
| `commonplace graph path "<A>" "<B>" [--max-depth 4] [--rel a,b] [--direction D] [--limit 5] [--mentions]` | shortest chains between two pages, e.g. use case → capability → building block; exit 1 if none |
| `commonplace graph related "<page>" [--limit 15] [--typed-only]` | ranked neighbours with the reason: direct links (by relation), co-citation (shared sources), Adamic-Adar shared neighbours, shared tags, personalised-PageRank proximity. Use it to widen a thin answer or find what to link |
| `commonplace graph hubs [--limit 20] [--typed-only]` | most central pages: PageRank, degree, betweenness. High betweenness = a bridge whose errors spread; review first |
| `commonplace graph communities [--min-size 3] [--typed-only]` | Louvain clusters with top members, type mix, the first topic/project/area page in the cluster (`NONE` = no map page yet) and distinct sources cited |
| `commonplace graph gaps [--typed-only]` | what is missing: wanted pages (linked but not existing, ranked by distinct referrers, with a did-you-mean), orphans (knowledge pages with no inbound link), dead ends (no outbound), sources no page cites, clusters without a map page, clusters resting on ≤1 source, bridges |
| `commonplace graph suggest-links ["<page>"] [--limit 100]` | unlinked mentions: another page's title or alias in prose (code, comments, existing links, footnotes and headings ignored), with file:line and the sentence. Multi-word names match case-insensitively; a single word only when it has a capital (proper noun or acronym) and matches exactly |
| `commonplace graph conflicts [--limit 30]` | same-type page pairs sharing sources or subject (title similarity, description overlap, tags) with no recorded contradiction or supersession between them; dates shown. The tool nominates, you read both and judge |
| `commonplace graph export [--format cypher\|graphml\|mermaid\|dot\|json] [--out file] [--mentions] [--around "<page>" --depth 2]` | the graph for Neo4j (`cypher-shell -f`), Gephi/yEd (GraphML), Obsidian/Markdown (Mermaid, `--around` for a neighbourhood), Graphviz, or JSON |

## Reviews and output

| command | does |
|---|---|
| `commonplace review prepare <persona> <pages or folders>... [--out file]` | packet: persona brief + each page + every cited source span with the deterministic verdict and context lines; written to `reviews/` |
| `commonplace review record <persona> <findings.json> --scope "<pages>" [--model m]` | writes `reviews/<date> <persona> <scope>.md` and stamps `reviews:` on each page; exit 1 if any blocker |
| `commonplace export <pages or folders>... [--out file] [--include-confidential]` | concatenate with renumbered footnotes and a Sources list; pages and quotes from confidential/privileged sources are withheld unless included |
| `commonplace log add <op> "<title>" [--note "..."]...` / `commonplace log show [-n 10]` / `commonplace log rotate [--keep 150]` | append-only log in the `## [date] op \| title` grammar; rotate old entries to `archive/log-<year>.md` |

Persona lookup order: `.commonplace/packs/<pack>/personas/<name>.md`, then the skill's
`references/personas/<name>.md` and `packs/<pack>/personas/<name>.md`
(via `COMMONPLACE_SKILL_DIR`), then `reviews/personas/<name>.md` in the commonplace.

## JSON shapes (most used)

- `lint --json`: `{errors, warnings, infos, pages_checked, findings: [{severity, rule, path, line, message}]}`
- `cite verify --json`: `[{page, line, label, source, locator, verdict: pass|quote_not_found|quote_not_in_span|span_not_found|no_quote|source_missing|text_layer_missing|quote_too_short|quote_too_long, found_line, nearest_lines, message}]`
- `ingest --json`: `[{id, raw, raw_sha256, text, note, converter, fidelity, pages, needs_conversion, duplicate_of, warnings}]`
- `search --json`: `[{rel, title, type, score, snippet, line}]`

## Manual mode (binary unavailable)

Follow the same rules by hand and say so in your report:

1. Ingest: copy the original to `sources/raw/<id>.<ext>`; compute `sha256sum`
   and record it in `.commonplace/manifest.json` (`sources.<id>.raw_sha256`); write
   `sources/text/<id>.text.md` with frontmatter `type: source-text`, `source`,
   `raw`, `raw_sha256`, `converter: llm`, `fidelity: medium` and `<!-- page:N -->`
   markers; write the note from `.commonplace/templates/source.md`.
2. Citations: for each `[^n]: [[id]] loc "quote"`, `grep -n -i -F "<quote>" "sources/text/<id>.text.md"`
   after straightening quotes and dashes, and confirm the line is inside the
   page or paragraph the locator names.
3. Links: for each `[[X]]`, `find . -iname "X.md"` or grep `aliases:`.
4. Index: rebuild `index.md` by listing pages per folder with their `description`.
5. Lint: check the required frontmatter per type (see AGENTS.md §4), tags in
   TAGS.md, orphans (grep for inbound links), open `hard` contradictions.
