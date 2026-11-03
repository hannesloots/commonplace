# commonplace

A cited markdown research wiki and second brain that Claude maintains while it
works, plus the Rust tool that keeps it honest.

- **Three layers.** Immutable originals (`sources/raw/`), a faithful markdown
  rendition with page and section anchors (`sources/text/`), and notes and
  synthesis that cite into it. Karpathy's LLM-wiki pattern with provenance.
- **Verifiable citations.** `[^1]: [[2025-01-08 Mavundla v MEC]] para 26 "verbatim quote"`.
  `commonplace cite verify` checks every quote exists inside the span the locator
  names. No quote, no claim.
- **PARA + memory.** `projects/`, `areas/`, `resources/`, `archive/` for
  synthesis; `memory/` for the person; `journal/` and `inbox/` for capture.
- **Packs.** Enterprise architecture (capability-based planning, aligned with
  the TOGAF® ADM), legal matters (jurisdiction
  profiles for South Africa, Australia, England and Wales; add your own as one
  YAML file), structured comparisons (medical schemes), and
  study/certification, each with types, templates, lint rules, traces and
  reviewer personas, as data.
- **Graph.** The commonplace is a typed graph: `commonplace graph path`, `walk`, `schema`,
  and export to Cypher (Neo4j), GraphML, Mermaid, DOT.
- **Reviews.** Lint proves structure; persona reviews (citation auditor,
  skeptic, librarian, freshness checker, reader, red team, pack experts) in a
  fresh context judge support and overreach.

## Layout of this repository

```
skills/commonplace/        the distributable skill (SKILL.md, references, templates, packs, scripts, bin)
crates/commonplace-core    Rust library: frontmatter, links, citations, lint, index, search, query, graph
crates/commonplace-cli     the `commonplace` binary
.claude-plugin/      plugin + marketplace manifests (install the repo as a Claude Code plugin)
hooks/               Stop hook that runs lint/cite verify inside a commonplace
fixtures/demo  a small commonplace that must lint clean (CI)
docs/                design record and research
```

## Install

**As a user who just wants it:** install from the marketplace, which carries
the built plugin with the binaries for every platform (this repository holds
only source; the release workflow publishes into the marketplace).

Claude Code:

```
/plugin marketplace add hannesloots/shelf
/plugin install commonplace@shelf
```

Claude Desktop / Cowork: Customize → Plugins → "+" next to Personal plugins →
Add marketplace → `hannesloots/shelf` → Install. Or upload
`commonplace-plugin.zip` from the [releases](https://github.com/hannesloots/commonplace/releases) page.

Then in any folder: "set up a commonplace for my research on X". The skill runs
`commonplace init`, and the folder gets `AGENTS.md` (the schema), `CLAUDE.md`, and
the tool-owned `.commonplace/`. Updates: `/plugin update commonplace`.

**As a developer:**

```
git clone https://github.com/hannesloots/commonplace && cd commonplace
cargo build --release --target x86_64-unknown-linux-musl
cp target/x86_64-unknown-linux-musl/release/commonplace skills/commonplace/bin/commonplace-x86_64-unknown-linux-musl
ln -s "$PWD/skills/commonplace" ~/.claude/skills/commonplace      # live-edit the skill
cargo test && sh scripts/dev/make-fixture.sh
```

Binaries for other targets come from the release workflow (`git tag v0.1.0 && git push --tags`).

## Using the tool directly

```
commonplace init ~/commonplace --name "My Commonplace" --purpose "..." --pack ea,study
commonplace ingest paper.pdf --title "..." --date 2026-01-01 --origin https://...
commonplace new concept "Claim-style title" --set 'tags=[topic/x]'
commonplace cite verify --changed · commonplace lint --changed · commonplace index · commonplace search "terms"
commonplace query --where "type=capability and target_maturity>baseline_maturity"
commonplace trace --name capability-to-sbb · commonplace graph path "A" "B" · commonplace graph export --format cypher
commonplace review prepare citation-auditor resources/concepts · commonplace review record ...
```

See `skills/commonplace/references/cli.md`.

## Design

See `docs/design.md` for the decisions and the evidence behind them.

## Licence

MIT.
