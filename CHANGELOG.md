# Changelog

## 0.5.2 — 2026-11-03

- First release from the published history: author and copyright are
  "Commonplace contributors"; the marketplace is `shelf`
  (hannesloots/shelf, install `commonplace@shelf`).

## 0.5.1 — 2026-11-03

- Releases publish the built plugin (with binaries) into the marketplace
  repository hannesloots/shelf, so a marketplace install gets the
  tool and not only the skill. The marketplace manifest moved there; this
  repository keeps only the plugin manifest.

## 0.5.0 — 2026-11-03

- Renamed from `brain` to `commonplace` (after the commonplace book: a
  personal book of sourced excerpts kept so they can be cited later). The
  binary, skill, plugin, repository (github.com/hannesloots/commonplace),
  config file (`commonplace.toml`), tool folder (`.commonplace/`), generated
  block markers (`<!-- commonplace:… -->`) and env vars
  (`COMMONPLACE_SKILL_DIR`, `COMMONPLACE_BIN`) carry the new name.
- Compatibility: a root with `brain.toml` or `.brain/` keeps using them;
  `<!-- brain:… -->` markers are recognised and rewritten on the next
  `index`; `BRAIN_SKILL_DIR` and `BRAIN_BIN` are still read. Nothing in an
  existing vault needs to change.

## 0.4.1 — 2026-11-03

- Release workflow: the Intel macOS binary is cross-compiled on the current
  macOS runner; GitHub retired the `macos-13` image the matrix used, which
  left every release run queued forever on that one job.

## 0.4.0 — 2026-11-03

- The `togaf` pack is now `ea` (enterprise architecture). The pack was never
  affiliated with The Open Group; the name, the persona (`togaf-architect`,
  now `chief-architect`) and the ADM phase letters in the workflow could read
  as if it were. The workflow is now organised by stage (vision, business,
  information and technology, gaps and roadmap) with one line noting the
  alignment with the TOGAF® ADM and the trademark. Rule ids TOG-00n are
  EA-00n; both spellings work in `disabled_rules`. `togaf` remains an alias
  (`brain init --pack togaf` works, and a brain whose `.brain/packs/togaf/`
  predates the rename keeps loading it). Run `brain pack install ea` to
  upgrade such a brain.

## 0.3.0 — 2026-11-03

- The `sa-legal` pack is now `legal` plus a jurisdiction profile. Country
  specifics (neutral and report citation patterns, court codes, hierarchy,
  primary sources, citator, regulator, privacy law, drafting conventions)
  live in `jurisdictions/<code>.yaml`; shipped: `za`, `au`, `en`, plus
  `generic`. `brain.toml` gains `jurisdiction`; `brain init --jurisdiction`
  and `brain pack install legal --jurisdiction` set it; `brain doctor` and
  `brain pack list` show it. `sa-legal` in an existing brain.toml is read as
  `legal` with `za`. The `sa-advocate` persona is now `senior-counsel`,
  reading the profile first; the authority template's `saflii_url` is
  `source_url`.

## 0.2.1 — 2026-11-03

- Release workflow: build attestation is skipped on private repositories
  (GitHub does not offer it there), so a private repo can publish releases.

## 0.2.0 — 2026-11-03

- Graph analysis, in memory, no new dependencies: `brain graph related`
  (links, co-citation, Adamic-Adar, tags, personalised PageRank, each with
  the reason), `hubs` (PageRank, degree, betweenness), `communities`
  (Louvain, with hub page and evidence count), `gaps` (wanted pages ranked
  by referrers, orphans, dead ends, uncited sources, clusters without a map
  page, single-source clusters, bridges), `suggest-links` (unlinked mentions
  with the line to link on) and `conflicts` (same-type pairs sharing sources
  or subject, for contradiction review). Graph commands are now documented
  in the skill and wired into the weekly maintenance pass.

- HTML ingest drops site chrome (scripts, styles, `nav`, `footer`) and
  rewrites relative links and images to absolute URLs against `--origin`, so
  Obsidian no longer shows hundreds of unresolved links under `sources/`.
- `brain source relink <id> | --all` repairs sources ingested before this
  change (rewrites links in place, re-hashes the text layer; locators unchanged).
- `brain init` writes `.obsidian/app.json` (if absent) that hides
  `sources/raw/` and `sources/text/` from search, graph and unresolved-link
  counts; links into them still open.

## 0.1.0 — 2026-11-03

First working version.

- `brain` CLI (Rust, static musl): init, doctor, status, ingest, source
  verify/attach-text/list/show, new, rename, archive, lint (core + pack rules),
  cite verify/find, links check/backlinks, dedupe, index, search (BM25),
  query (frontmatter expressions), trace, graph (stats, schema, walk, path,
  export cypher/graphml/mermaid/dot/json), review prepare/record, export, log,
  pack list/install.
- Skill `brain`: SKILL.md, references (cli, schema, ingest, research, review,
  maintain, memory, express), seven personas, four packs (togaf, sa-legal,
  compare, study) with templates, declarative lint rules, traces and expert
  personas.
- Plugin wrapper: manifest, marketplace, Stop hook running lint and cite verify
  inside a brain, `bin/brain` on PATH.
- Fixture brain and release workflow (five targets, attestations, cloud and
  desktop skill bundles, plugin zip).
