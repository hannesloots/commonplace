# Design record: the commonplace skill

How the skill is structured and why. The evidence is in the research surveys this was built from (kept outside the repository)
(two surveys) and the four research passes run on 2026-11-03 (Karpathy's gist
and its comment thread, Google's Open Knowledge Format v0.2, the main
open-source implementations, the 2026 papers on LLM-compiled wikis and
citation faithfulness, second-brain methods, agent-memory systems, Claude
Code's skill/plugin/hook rules, and the Rust crate landscape).

## 1. What problem the skill solves

Research done in chat evaporates. RAG rediscovers knowledge on every question.
A wiki the model maintains compiles knowledge once and keeps it current, but
in practice it drifts: fabricated quotes, hallucinated link targets,
near-duplicate pages, orphans, indexes that overflow context, changelogs that
contradict reference sections. The skill is the wiki pattern plus the
discipline that practitioners converged on, with the mechanical parts in a
binary that does not forget.

## 2. Decisions

| decision | choice | why |
|---|---|---|
| Evidence chain | raw (immutable, hashed) → text layer (faithful, anchored, immutable) → note → synthesis | citations need a stable target; editing the text layer would silently invalidate them; hashes detect tampering (gist commenters) |
| Citation form | footnote `[[source]] locator "verbatim quote"` | Obsidian- and GitHub-rendered; deterministic check of existence within a span (the property Anthropic's Citations API has: cited text is extracted, not generated); quotes are what a reviewer and a court can check; `^[file:L1-L2]` line ranges alone are not readable and break on re-conversion |
| Locators | page markers, numbered paragraphs, section ids, lines; split text layers per section with page metadata | the user's established practice for PDFs; judgments cite paragraphs; standards cite sections; pages survive re-conversion better than lines |
| Deterministic vs judgement | Rust: hashes, links, quotes, frontmatter, index, search, graph. Personas: entailment, overreach, duplicates, freshness, readability | "models pretty much never check on their own, so it has to be a script" (gist thread); LLM judges have position/verbosity/self-preference biases and reject correct citations (CiteGuard); verifier strictness alone swings unsupported-citation rates 3→18% (Goo et al.), so name the verifier and keep the deterministic layer |
| Organisation | sources global; synthesis in PARA; memory separate | one source serves many projects; PARA scopes retrieval by actionability; archive excluded from search |
| Index | generated L1 index with a kept orientation block; hubs above a threshold; generated page lists inside project/area/topic pages | hand-maintained indexes break in the low hundreds of pages; capable agents skip a large index anyway (Cochran 2026), so keep it small and make topic pages the maps of content |
| Identity | filename is identity; aliases; `commonplace rename` rewrites links | synto's separate `entity_id` solves homonyms but doubles bookkeeping for the model; rename-with-rewrite plus aliases covers the same failure at lower cost |
| Contradictions | keep both, cite both, `contradictions` entry with severity, `contested` confidence; hard = lint error | llm-wiki-compiler `contradictedBy`, kfchou severity tokens; "Beyond Memory" argues for preserving superseded paths |
| Log | append-only `log.md` with the gist grammar, rotated by the tool; git for real history | unbounded logs confuse weaker models; git diff is the better audit trail |
| Packs | declarative `pack.yaml`: types, required fields, enums, typed links, rules with a small check vocabulary, traces, personas | "one CLAUDE.md doesn't survive the second domain" (mpt.solutions); domain rules must be editable without Rust; the binary stays domain-free |
| Legal jurisdictions | one `legal` pack whose pack.yaml carries `{{jurisdiction.*}}` placeholders, filled at load time from `jurisdictions/<code>.yaml` (citation patterns, court codes, sources, citator, regulator, privacy law, drafting conventions); `jurisdiction` in commonplace.toml; `sa-legal` kept as an alias | about a fifth of the original SA pack was country-specific and all of it was data; a profile is one file to add, the gates stay identical, and the binary still knows nothing about law |
| Graph | typed edges from frontmatter link fields, Dataview inline fields, citations, mentions; path/walk/schema; Cypher export | the ontology already exists in the frontmatter; multi-hop questions ("which use case depends on this standard?") need traversal; Neo4j for anyone who wants Cypher |
| Graph analysis | in-memory, hand-rolled: personalised PageRank, Adamic-Adar, Louvain, Brandes betweenness; `related`, `hubs`, `communities`, `gaps`, `suggest-links`, `conflicts`; every row carries its reason | the "Rust search and graph tooling" review (Nov 2026): the hard part is the analysis (orphans, wanted pages, link prediction, communities, bridges), not storage or Cypher; at wiki scale it runs in milliseconds in memory; Kuzu archived, CozoDB abandoned, DuckPGQ research-only; unlinked-mention scans are the highest-yield link signal (Obsidian Graph Analysis, llm-wiki-mcp); the tool nominates and the model judges, which keeps the deterministic layer honest |
| Memory | `memory/` pages with dated, superseded facts; playbooks as the procedures skills are compiled from (see Skills) | Graphiti bi-temporal facts, Mem0 invalidate-not-delete, WikiSkill's skill layer; Claude Code auto-memory stays for how-to-work-with-the-user |
| Skills | three layers: raw (sources, use records) → wiki (playbooks, error book) → skills (`skills/<name>/SKILL.md` + `PURPOSE.md`); one atomic proposal at a time, gated by the user; accepted versions kept in `.commonplace/skills/` and only those installed (`.claude/skills`, `.agents/skills`); every decision and use appended to `skill-impact.md`, never rolled back; skills may not link into the wiki | WikiSkill (Tang et al., Aug 2026): removing wiki access from the skill proposer dropped scores 63.7%→48.7%; letting the task agent read the wiki made skills worse; the impact record stops rejected edits recurring. No validation set here, so the user is the gate; baseline in the tool dir, not git, so reject works without a repository |
| Distribution | plugin (skill + hooks + bin) from a marketplace; skill folder also works standalone and zipped | a non-developer user installs once and updates with `/plugin update`; `${CLAUDE_PLUGIN_ROOT}` changes per version so the commonplace lives outside it; static musl binary, 4 MB, no runtime |
| Name | `commonplace`, after the commonplace book; `brain` (from "second brain") until 0.5.0 | "brain" was generic and unsearchable, leaned on a phrase Forte Labs owns, and said nothing about the differentiator; a commonplace book is exactly a personal collection of sourced excerpts kept for later citation, and nobody owns the word |
| Language | Rust, one static binary, no C deps, no network | runs unchanged on a non-developer's machine and in sandboxes; pure-Rust tree needs no toolchain for musl; PDF conversion stays external (Docling/Marker/MarkItDown remain better than any Rust crate) |
| Crates | yaml_serde (YAML org's maintained serde_yaml), regex-based markdown scanning, hand-rolled BM25, strsim, unicode-normalization, htmd, ignore, sha2, clap | small, maintained, pure Rust; tantivy adds 3.5 MB and a C dependency for no gain below ~50k pages |

## 3. What was deliberately left out

- Vector search, MCP servers, databases: unfaithful to plain files; revisit above ~500 pages if BM25 is not enough.
- A SQLite cache (FTS5, sqlite-vec) and local embeddings (model2vec), as the Nov 2026 search/graph review suggests for Phase 2: parsing the whole commonplace takes ~100 ms at 60 pages and scales linearly, so a cache buys nothing yet; rusqlite `bundled` adds a C build to the musl target; embeddings add a model download and a semantic signal Claude supplies itself when it reads `related` output. Revisit when a commonplace passes a few thousand pages or holds a large raw corpus (vendor docs), where hybrid BM25+vector with RRF and a `raw` collection would earn their place.
- PDF conversion in Rust: external tools or the model do it; the binary anchors and verifies.
- Automatic merging of duplicates: proposals only; a merge is a judgement.
- Scores from LLM judges: binary checklist items with evidence quotes instead.
- Luhmann ids, Johnny.Decimal numbering, full GTD: human ergonomics; next actions kept.

## 4. How the pieces fit

```
user ──asks──▶ Claude (skill: SKILL.md + references + packs)
                 │ writes notes, synthesis, memory; spawns persona reviewers
                 ▼
            commonplace/ (AGENTS.md schema, PARA, sources, memory)
                 ▲
                 │ init · ingest · new · lint · cite · index · search · query · trace · graph · review · export
            commonplace binary (crates/commonplace-core + commonplace-cli)
```

The skill never edits the binary; the binary never judges content. A commonplace
works without the skill (AGENTS.md is complete) and without the binary
(manual mode), at reduced safety.

## 5. Open items

- Test the plugin install and executable bits on a clean machine (Windows and
  macOS binaries come from CI; not yet exercised).
- Verify the Stop hook's input schema (`stop_hook_active`) against current docs
  before relying on the loop guard.
- Run the eval cases under `skills/commonplace/evals` after each change to SKILL.md.
- Pilot each pack on a real but low-risk case; turn recurring mistakes into
  lint rules.
- OKF export command (`commonplace export --format okf`) is specified but not built.
