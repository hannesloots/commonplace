# Changelog

## Unreleased

- Skills layer, after Google's WikiSkill paper: raw sources and use records,
  the wiki that distils them (playbooks, error book), and agent skills
  compiled from the wiki. A skill is `skills/<name>/SKILL.md` (an Agent
  Skill, self-contained) plus `PURPOSE.md` (links to the wiki pages behind
  it). New commands `commonplace skill new | list | diff | accept | reject |
  used | sync`. An edit is a proposal until `accept`. Accept and reject
  both append the diff, reason and evidence to `skill-impact.md`, which is
  never rolled back. Only accepted versions are kept (in
  `.commonplace/skills/`) and installed (`.claude/skills`, `.agents/skills`,
  configurable under `[skills]`; `sync --user` installs under the home
  folder). Installs never overwrite a skill folder commonplace did not write.
- New error code `skill_not_found`. New lint rules `SKILL-001`, `SKILL-002`
  and `SKILL-004` to `SKILL-007` (see `references/cli.md`).
- A skill's `PURPOSE.md` is its wiki page (`type: skill`, named after the
  skill): it appears in backlinks, the graph and a Skills group in the index,
  its broken links are `LINK-001`, and `rename` rewrites its links in both the
  source and the accepted copy, so a rename never shows up as a proposal.
- `init` creates `skills/` and `skill-impact.md`; `init --force` adds the
  Skills operation to AGENTS.md. Page discovery skips `.claude/`, `.agents/`
  and, inside skill folders (those with a `SKILL.md` or an accepted copy),
  everything but `PURPOSE.md`. Other folders under `skills/` stay ordinary
  notes.
- `skill accept` refuses a `SKILL.md` agents cannot load (the SKILL-001
  checks) and an install folder holding a skill commonplace did not write,
  before anything is recorded. Every `skill` command rejects a name that is
  not a valid skill name, so a name can never reach outside `skills/`.
  `skill new` refuses a name an existing page already has. Reasons, notes
  and descriptions are kept on one line.
- Development: a devcontainer (`.devcontainer/`, Rust toolchain, musl
  target, Claude Code; rootless Podman, no capabilities).
- Eval cases moved from `skills/commonplace/evals/` to `evals/`, where
  `claude plugin eval` reads them (it refuses a folder inside `skills/`).
  They no longer ship inside the skill.
- The skill's references: new `references/skills.md`, with the loop
  (record use → distil into the wiki → propose one change → user accepts or
  rejects).

## 0.6.1 — 2026-11-09

- The plugin installs in GitHub Copilot CLI and OpenAI Codex as well as
  Claude Code. A root `plugin.json` (Agent Plugins format) sits next to
  `.claude-plugin/plugin.json`, and the shelf marketplace has a Codex list
  (`.agents/plugins/marketplace.json`) that the release keeps in step.
- `SKILL.md` tells agents that do not set `CLAUDE_SKILL_DIR` how to find the
  launcher, and to use `scripts/commonplace.ps1` in Windows PowerShell.

## 0.6.0 — 2026-11-05

- Errors have stable codes. A failed command (exit 2) prints
  `error[<code>]: <message>` and a `hint:` line on stderr; with `--json` it
  prints `{"error": {"code", "message", "hint", "suggestions", "path"}}` on
  stdout, so an agent can branch on the code. A missing page suggests the
  closest titles. Command-line usage errors with `--json` use code `usage`.
  The codes are listed in `references/cli.md`.
- `review record` checks every page in scope before writing. Before, a page
  with broken frontmatter stopped it after the review page and some stamps
  were already written; the error now names the page. A missing findings
  file is an `io` error that names the path.
- An unreadable `.commonplace/manifest.json` made `lint` fail with exit 2
  after checking, so the Stop hook (which blocks on exit 1) let it pass.
  `lint` now reports it as `SRC-005` (error), skips the raw-file check that
  would flag every source, and leaves the manifest untouched.
- `lint --changed` (and the Stop hook that uses it) skipped changed files
  whose names had non-ASCII characters (`Café.md`), because git escaped
  them. It now reads git's NUL-separated output, which is never escaped.
- `lint` reports a pack rule whose regex does not compile (`PACK-001`,
  error, on the pack's `pack.yaml`). Before, such a rule silently never
  fired. Pack regexes are now compiled once per run, not once per page.
- `graph walk` / `graph path`: `--direction` accepts only `out`, `in` or
  `both`; anything else is a `usage` error. Before, a typo silently meant
  `both`.
- `query --help` showed its field-comparison example in backticks
  (`` `target_maturity>baseline_maturity` ``); typed as shown, the query
  silently matched nothing. The help no longer shows backticks, and the
  query parser now ignores them.
- A YAML syntax error in a page's frontmatter is a `frontmatter` error that
  names the page (`path`), not an unlocated `parse` error.
- `lint --changed` (and `lint <paths>`) no longer hides findings on
  non-markdown files such as `SRC-005` on the manifest or `PACK-001` on a
  `pack.yaml`; and in a vault without git it no longer fails outright on an
  unreadable manifest.
- `COMMONPLACE_NOW` (`YYYY-MM-DD` or RFC 3339) pins the clock for every date
  the tool writes or compares, for repeatable runs and evals.
- `--root` at a folder without `commonplace.toml` now fails with `no_root`
  instead of an I/O error.
- `export` renumbered footnotes one label at a time, so a page that defined
  `[^2]` before `[^1]` ended up with both references as `[^2]`. Footnotes
  are now renumbered in one pass.
- `doctor` printed the enabled packs as `Array [String("ea"), ...]`; it now
  prints `["ea", "legal"]`, like `status`.
- `cite find` no longer crashes on a line locator of 0.
- `lint --changed` now includes files ending in upper-case `.MD`, as the
  rest of the tool does.
- Internal: Rust edition 2024; the workspace denies clippy pedantic and
  `unwrap`/`expect`/`panic` lints; long functions split; CI checks
  formatting, clippy, tests, docs, licences and advisories (cargo-deny) and
  the minimum Rust version (1.88).

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
