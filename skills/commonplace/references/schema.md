# Schema: layout, types, frontmatter, citations

The canonical schema lives in each commonplace as `AGENTS.md` (written by `commonplace init`;
`CLAUDE.md` imports it). This file explains the design so you can apply it well
and upgrade older commonplaces. When they differ, the commonplace's AGENTS.md wins.

## Contents

- Layout and the evidence chain
- Page types and required frontmatter
- Status, confidence, confidentiality vocabularies
- Citations and locators
- Text layers for PDFs: pages and sections
- Supersession and contradictions
- Index tiers
- Open Knowledge Format compatibility
- Upgrading a commonplace or migrating an llm-wiki vault

## Layout and the evidence chain

```
commonplace/
├── AGENTS.md  CLAUDE.md  commonplace.toml  TAGS.md
├── index.md (generated)  overview.md  log.md  error-book.md  open-questions.md
├── inbox/  journal/  questions/  reviews/  indexes/ (generated)
├── sources/raw/   <id>.<ext>            immutable originals, sha256 in .commonplace/manifest.json
├── sources/text/  <id>.text.md | <id>/  faithful markdown, anchors, never edited
├── sources/notes/ <id>.md               TL;DR, summary, key quotes (summary ladder)
├── projects/<slug>/<Title>.md + pages   PARA: project (has an end)
├── areas/<slug>/<Title>.md + pages      PARA: area (standard to maintain)
├── resources/{concepts,entities,topics,syntheses,playbooks}/   PARA: resource
├── memory/ Profile.md Preferences.md Glossary.md people/ decisions/
├── archive/                             PARA: archive
└── .commonplace/ manifest.json templates/ packs/
```

Why sources are global: one judgment, brochure or standard serves many
projects. Why text layers are immutable: citations point into them by page,
paragraph, section or line; editing would silently invalidate every citation.
Why notes are separate from text: the note is yours (summary ladder, L1–L3),
the text is the source's (L0).

Source id = `YYYY-MM-DD Short Title` (publication date). The same id names the
raw file, the text layer (`.text.md` suffix keeps Obsidian links unambiguous)
and the note. Citations link `[[id]]`, which resolves to the note.

## Page types and required frontmatter

Every page: `type`. Synthesis pages (concept, entity, synthesis, playbook,
decision, and any pack type with `cited: true`) also need `tags`, `status`,
`confidence`, `description`, `created`, `updated`, `sources`.

| type | extra required | notes |
|---|---|---|
| source | title, kind, raw, status, confidentiality, authority | `text` empty until converted; `summary_level` 0–4 |
| concept | — | title is the claim; one idea per page |
| entity | kind | organization, product, tool, case, place, event, work, other |
| topic | scope | map of content; `scope` is a folder or `topic/tag`; list generated |
| synthesis | question | a saved answer |
| playbook | trigger | procedure that worked; `times_used`, `last_used` |
| project | outcome, status(active/paused/done/archived) | `due`, `next_actions`, `waiting_for`, `packs` |
| area | status(active/dormant/archived) | `standard`, `review_cadence` |
| decision | status(proposed/accepted/rejected/superseded), date | ADR sections |
| person | — | `relationship`, `role`; dated facts |
| journal | date | one per day |
| capture | created, status(new/processed) | fleeting; delete or file within days |
| question | status(open/answered/parked) | what would resolve it |
| review | persona, date, scope | written by `commonplace review record` |

`description` is the index line (L0): one claim-style sentence written for
someone deciding whether to open the page. `aliases` carry other names so
links resolve. `related` and `up` are link lists. `confidentiality` defaults
to personal.

## Vocabularies

- status (synthesis): `seed` thin · `growing` useful, incomplete · `mature` well sourced, stable · `superseded`
- status (source): `registered` → `converted` → `summarised` → `integrated` → `superseded`
- confidence: `high` · `medium` · `low` · `contested` (sources disagree; see contradictions)
- authority: `primary` · `secondary` · `tertiary`
- fidelity (text layer): `high` (copy/pandoc/html) · `medium` (pdftotext, LLM conversion) · `low` (scan, OCR) · `none`
- confidentiality: `public` < `personal` < `confidential` < `privileged`; a page may not quote a source ranked above itself (PRIV-001)

## Citations and locators

```
Sentence with a sourced fact.[^3]

[^3]: [[2025-01-08 Mavundla v MEC]] para 26 "counsel lazily omitted to verify the research" — optional comment
```

| locator | matches | notes |
|---|---|---|
| `p.12`, `pp.12-14` | between `<!-- page:12 -->` markers | PDFs via pdftotext get these automatically |
| `para 26`, `¶26`, `[26]`, `at para 26` | from a `<!-- para:26 -->` marker or a line starting `[26] ` to the next | judgments, standards with numbered paragraphs |
| `§3.2`, `s 3.2` | heading whose text starts with `3.2` to the next heading of the same level | section-split text layers use `section` frontmatter |
| `L120`, `L120-128` | lines of the text layer | stable because the layer is immutable |
| `00:14:32`, free text | whole file | unverifiable span; verifier warns CITE-005 but still checks the quote |
| none | whole file | allowed, discouraged |

Quote rules: 3–80 words, verbatim, `...` allowed to join two segments in order.
Normalisation applied on both sides: NFKC, lowercase, straight quotes and
hyphens, soft hyphens removed, punctuation ignored, whitespace collapsed,
end-of-line hyphenation joined. Numbers, names and dates must match exactly.

## Text layers for PDFs: pages and sections

Long PDFs (standards, textbooks, judgments, course books) must be citable by
page and section. Two layouts are supported:

1. **Single file** `sources/text/<id>.text.md` with `<!-- page:N -->` before each
   page's text (pdftotext does this). Add `<!-- para:N -->` for numbered paragraphs
   when the source numbers them and the line does not already start with `[N]`.
2. **Section split** `sources/text/<id>/01 Introduction.md`, `02 Scope.md`, …
   each with frontmatter:
   ```yaml
   ---
   type: source-text
   source: "<id>"
   section: "3.2"          # id used by §3.2 locators (or the heading number)
   title: "Maturity"
   page_from: 14
   page_to: 18
   ---
   <!-- page:14 -->
   ...
   ```
   The verifier concatenates sections in file order, inserts `<!-- section:3.2 -->`
   and a `<!-- page:14 -->` marker if the body has none, so `p.`, `para`, `§`
   locators all work across the split. Register the folder with
   `commonplace source attach-text "<id>"`. Keep each section under ~2,000 lines.

Use the split layout when you convert with Docling/Marker or by reading the PDF
yourself: one section per heading, page markers at every page boundary. This is
what makes a 300-page standard usable without loading it whole.

## Supersession and contradictions

Never delete. When a newer source replaces a claim: keep the old sentence with
its citation, add the new one, set the page's `confidence: contested` until
resolved, and add

```yaml
contradictions:
  - with: "[[Other Page or Source]]"
    severity: soft | scope | hard
    status: open | resolved
    note: "why; what resolved it"
```

`hard` open contradictions are lint errors; `soft`/`scope` are warnings. A
whole page replaced by another gets `status: superseded` + `superseded_by`.

## Index tiers

`index.md` is L1: an orientation block you write (kept between
`<!-- commonplace:keep -->` markers) plus generated group lists of `[[page]] — description`.
Groups above `[index] hub_threshold` (default 60) move to `indexes/Index - <Group>.md`
hubs; Sources and Archive always get hubs once they pass half the threshold.
Project, area and topic pages carry a generated `<!-- commonplace:pages -->` list, so a
project page is its own map of content. Keep `index.md` under ~200 lines; the
tool reports its size.

## Open Knowledge Format compatibility

Google's OKF v0.2 needs only `type` in frontmatter and reserves `index.md` and
`log.md`. This schema is a superset: `title`, `description`, `tags`, `sources`,
`status` map directly; `reviews` entries map to OKF `verified` events;
`superseded_by`/`updated` map to `stale_after`/`status: deprecated` on export.
Citations here are stricter than OKF (verbatim quote within a located span).

## Upgrading a commonplace or migrating an llm-wiki vault

- Upgrade: `commonplace init <root> --force` refreshes AGENTS.md's standard sections
  (Local rules kept), templates and packs. Then `commonplace lint` and fix.
- Migrating a Karpathy-style `raw/` + `wiki/` vault: ingest each `raw/` file
  (`commonplace ingest raw/x.pdf --title ... --date ...`), move `wiki/sources/*` notes'
  content into the generated `sources/notes/` pages, move entities/concepts/
  syntheses into `resources/`, add `description` and `sources` to each, convert
  `(see [[Source]])` references into footnote citations with quotes, then
  `commonplace index` and `commonplace lint` until clean. Propose the plan before moving files.
