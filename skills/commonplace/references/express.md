# Express: deliverables that leave the commonplace

Everything that leaves the commonplace carries its evidence. A deliverable is a page
(or export) whose every factual sentence is cited, whose confidence is
stated, and that has passed the citation auditor and the skeptic.

## Shapes

| ask | produce |
|---|---|
| an answer | a `synthesis` page: Answer, Reasoning, Caveats, Related; cite |
| a report or memo | synthesis page(s) + `commonplace export <pages> --out report.md` (renumbers footnotes, appends Sources, withholds confidential material) |
| a comparison | compare pack: `comparison` page with the matrix from `commonplace query --where "type=option and year=YYYY" --fields ...` and scenario tables |
| architecture deliverable | ea pack: Architecture Vision / Target Architecture / Roadmap templates, tables from `commonplace trace --name ...` and `commonplace query` |
| legal draft | legal pack: `draft` page in the jurisdiction profile's drafting conventions; lint LEG-001/002 must pass; practitioner verifies before filing |
| study output | study pack: module page with concepts and playbooks, flashcards, certificate coverage table |

## Structure of a deliverable

1. The answer or recommendation first, in one paragraph.
2. The reasoning with citations, tables where there are comparisons.
3. Confidence per conclusion and why.
4. Caveats: unverified items, secondary sources, dates, contradictions left open.
5. Sources (export generates it).

Write for a reader who will not open the commonplace: define terms (link the
glossary), no internal shorthand, no page ids in prose (the footnotes carry them).

## Before release

```bash
$COMMONPLACE cite verify <pages>
$COMMONPLACE review prepare citation-auditor <pages>   # then skeptic, then reader
$COMMONPLACE lint <pages>
$COMMONPLACE export <pages> --out <file>               # check the Sources list reads well
```

Record the deliverable in the project page (Log and Decisions), log an
`edit`/`research` entry, commit. If the deliverable is reusable, promote the
synthesis to `resources/syntheses/` and link it from the topic page.

## Formatting for other tools

The export is plain markdown with footnotes; pandoc, Obsidian and most
renderers handle it. For Word, PDF or slides, use the respective skill on the
exported file. Keep the footnote definitions; they are the audit trail.
