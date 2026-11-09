---
name: commonplace
description: Builds and maintains a cited markdown research wiki and second brain ("commonplace") while doing research, studying, or working on projects. Keeps every original document, a faithful page-anchored markdown copy and a summary note; compiles concept, entity, project and memory pages with verbatim-quote citations that the bundled `commonplace` tool verifies; organises synthesis with PARA; runs lint and persona reviews. Use whenever the user wants to research a topic over time, ingest documents or PDFs, build or query a knowledge base / second brain / Obsidian vault, remember things across sessions, compare options (medical aid, insurance, vendors), ground work in a legal matter, do enterprise architecture (TOGAF-style capabilities, use cases, vision, target architecture), or study a course or certification, even if they do not say "wiki" or "commonplace".
---

# Commonplace

A commonplace is a folder of plain markdown that is the durable memory for research,
projects and the person it belongs to. You are its maintainer. The user curates
sources and asks questions; you read, write, link, cite and keep it clean. It
compounds: every query starts from what earlier queries filed.

Three layers, three operations (Karpathy's LLM-wiki, formalised):

| layer | folder | who writes it |
|---|---|---|
| originals, immutable | `sources/raw/` | the tool (`commonplace ingest`) |
| faithful markdown with page/section anchors | `sources/text/` | the tool or you, once, never edited after |
| notes and synthesis (PARA: projects, areas, resources, archive; plus memory/) | everything else | you |

Operations: **ingest** → **query** → **lint**, plus **research**, **review**,
**distill**. The `commonplace` binary does the bookkeeping that models do badly
(hashing, link resolution, verbatim-quote checks, index generation, search).
You do the judgement (summarising, integrating, spotting contradictions).

## First: find the tool and the commonplace

```bash
COMMONPLACE="${CLAUDE_SKILL_DIR:-<this skill dir>}/scripts/commonplace"   # launcher picks the right binary
$COMMONPLACE doctor                                                   # root, converters, packs
```

`<this skill dir>` is the folder that holds this `SKILL.md`; use its absolute path
when your agent does not set `CLAUDE_SKILL_DIR`. In Windows PowerShell (no `sh`),
run `& "<this skill dir>\scripts\commonplace.ps1" doctor` instead; every
`$COMMONPLACE` command below works the same way through it.

If `doctor` finds no root, ask where the commonplace should live (default `~/commonplace`)
and run `$COMMONPLACE init <path> --name "<Name>" --purpose "<one sentence>"
[--pack ea,legal,compare,study] [--jurisdiction za|au|en]`, then `git init` inside it. If the binary
cannot run on this machine, follow [references/cli.md § Manual mode](references/cli.md)
and say so in your report.

Then read, in the commonplace root: `AGENTS.md` (the schema; it overrides anything
here), `index.md`, `overview.md`, `error-book.md`, and the last entries of
`log.md` (`$COMMONPLACE log show`). Obey every constraint in the error book.

## Non-negotiables

1. Never modify `sources/raw/` or `sources/text/`. Re-convert instead of editing.
2. Every factual sentence from a source carries a footnote citation:
   `[^n]: [[<source id>]] <locator> "verbatim quote"`. Copy quotes from the text
   layer; `$COMMONPLACE cite find "<id>" "<quote>"` gives the locator. Never adjust a
   quote so it passes. A quote proves existence, not support; the Citation
   Auditor persona judges support.
3. Never write a `[[link]]` to a page you have not confirmed exists. Search
   (`$COMMONPLACE search`) before creating; extend, don't create. Create pages with
   `$COMMONPLACE new <type> "<Title>"` so frontmatter is right.
4. Never answer from memory when the commonplace or a source can answer. If you must,
   mark it `unsourced:` and set confidence low.
5. Never delete knowledge. Supersede (`status: superseded`, `superseded_by`),
   record contradictions, keep both sides cited.
6. Respect `confidentiality`. Privileged or confidential material never flows
   into a less confidential page, export, or chat summary without the user's say.
7. Finish every writing session: `cite verify --changed` → `index` →
   `lint --changed` → fix errors → `log add` → commit. Report lint status honestly.

## Workflow router

| the user wants | do | read |
|---|---|---|
| add a document, URL, PDF, course material; files in `inbox/` | Ingest | [references/ingest.md](references/ingest.md) |
| a question the commonplace might answer | Query (below) | — |
| "research X", "find out", "compare", "what are the options" | Research | [references/research.md](references/research.md) |
| write a report, vision, memo, comparison, heads of argument | Express | [references/express.md](references/express.md) |
| "review this", before anything leaves the commonplace | Review | [references/review.md](references/review.md), [references/personas/](references/personas/) |
| "clean up", "health check", weekly, after writing | Maintain | [references/maintain.md](references/maintain.md) |
| "what connects to X", "what is missing", "how do A and B relate", ontology, Neo4j-style questions | Graph (`$COMMONPLACE graph related/path/gaps/communities`) | [references/cli.md § Graph](references/cli.md) |
| remember a fact, preference, person, decision; daily notes; "what did we decide" | Memory | [references/memory.md](references/memory.md) |
| set up, upgrade or understand a commonplace's structure | Schema | [references/schema.md](references/schema.md) |
| any `commonplace` command, JSON output, exit codes, manual fallback | CLI | [references/cli.md](references/cli.md) |

## Query procedure

1. `index.md` → the relevant topic, project or area page → `$COMMONPLACE search "<terms>"`
   (add `--text` to search inside source text layers, `--include-archive` only when asked).
2. Read source notes first. Open `sources/text/` spans (`$COMMONPLACE source show "<id>" --at p.12`)
   for anything you will quote. For "how does X relate to Y" or to widen a
   thin answer, `$COMMONPLACE graph related "<page>"` and `$COMMONPLACE graph path "<A>" "<B>"`
   give the neighbours and the chain, each with the reason.
3. Answer with citations and links to the pages used. Distinguish what the commonplace
   says from what you add. State confidence.
4. If the answer took real synthesis, file it: `$COMMONPLACE new synthesis "<Question as title>"`,
   cite, link from the topic page, log it. The next query starts further ahead.

## Where things go

- Sources: `sources/{raw,text,notes}/`, global, by `YYYY-MM-DD Title` id. Never inside a project.
- Outcome-bound work with an end: `projects/<slug>/`. Standing responsibilities: `areas/<slug>/`.
- Reusable knowledge: `resources/{concepts,entities,topics,syntheses,playbooks}/`. One idea per concept page, claim-style title, 3–8 links.
- About the user: `memory/` (Profile, Preferences, Glossary, people/, decisions/). Daily notes: `journal/`.
- Inactive: `archive/` (`$COMMONPLACE archive "<Project>"`). Unresolved: `questions/` and `open-questions.md`.
- Procedures that worked, including what you learn from the user's courses: `resources/playbooks/`. They are how the commonplace makes both of you more capable next time.

## Packs

`commonplace.toml` lists enabled packs. Before working in a domain, read its workflow
and keep its lint rules green (they are mandatory, not advisory):

| pack | for | read |
|---|---|---|
| `ea` | enterprise architecture, capability-based planning (aligned with the TOGAF® ADM, framework-neutral): drivers → capabilities (baseline/target maturity) → ABB/SBB → gaps → roadmap; Architecture Vision and Target Architecture deliverables; `commonplace trace` matrices. `togaf` is accepted as an alias | [packs/ea/workflow.md](packs/ea/workflow.md) |
| `legal` | matters in any adversarial system: chronology, issues, evidence map, verified authorities, arguments, drafts. Hard gate: no citation in a draft without a verified authority page and the judgment in `sources/raw`. Country specifics (citation forms, courts, primary sources, regulator) come from a jurisdiction profile: `za`, `au`, `en` shipped, set with `--jurisdiction`; ask which one if `commonplace doctor` says none | [packs/legal/workflow.md](packs/legal/workflow.md) |
| `compare` | medical schemes, insurance, vendors: year-bound options with cited attribute values, needs profile, scenarios, generated matrix | [packs/compare/workflow.md](packs/compare/workflow.md) |
| `study` | courses and certifications: modules distilled into concepts and playbooks, flashcards, exam tracking | [packs/study/workflow.md](packs/study/workflow.md) |

Enable one later with `$COMMONPLACE pack install <name>`.

## Reviews

Deterministic checks (`lint`, `cite verify`) prove structure. Judgement needs a
fresh pair of eyes: `$COMMONPLACE review prepare <persona> <pages>` builds a packet;
give it to a subagent with a clean context (another model when possible, with
`model:` on the Agent call); record its JSON findings with `$COMMONPLACE review record`.
Personas: citation-auditor, skeptic, librarian, freshness-checker, reader, plus
pack experts (chief-architect, opposing-counsel, broker, examiner). Run the
citation auditor and skeptic before any deliverable leaves the commonplace.

## Research, briefly

Plan first (`projects/<slug>/Research Plan.md`: question tree, expected sources,
stopping rule, effort budget). Fetch and ingest; subagents write source notes
straight into the commonplace and return paths plus a short summary, not full text.
Synthesise with citations. Find the thinnest claim and do one targeted pass on
it. Run `cite verify`, then the citation auditor and skeptic. Then express.
Details and the brief template: [references/research.md](references/research.md).

## Reporting back

Say what was ingested, created and updated (page list), unresolved
contradictions, unverified citations, open questions, and the lint result.
Never say "verified" unless `cite verify` passed and the citation auditor ran.
Never attribute pages to a model or tool in the commonplace itself.
