# Research: plan → gather → ingest → synthesise → verify → review → express

This is the deep-research loop, filed into the commonplace. It mirrors how strong
research agents work (a lead plans and writes the plan down; workers fetch and
summarise into files, not chat; citations are a separate pass; a rubric judges
the result) with one difference: everything lands in the commonplace, so the next
question starts from compiled knowledge instead of from scratch.

## Contents

1. Plan
2. Effort budget and subagents
3. Gather and ingest
4. Synthesise
5. Gap pass
6. Verify and review
7. Express
8. Subagent brief template
9. Quality rubric

## 1. Plan

Create or open the project (`$COMMONPLACE new project "<Title>"`) and write
`projects/<slug>/Research Plan.md` (`$COMMONPLACE new synthesis "Research Plan: <Title>" --folder projects/<slug>`):

- **Question tree**: the main question, 3–7 sub-questions, each with what
  would count as an answer.
- **What the commonplace already knows**: `$COMMONPLACE search` and the topic pages;
  list relevant existing pages so you extend rather than duplicate.
- **Expected sources**: primary first (standards, regulators, courts, vendor
  documents, papers), then secondary. Name them if you can.
- **Stopping rule**: e.g. "two independent primary sources agree on each
  sub-question, or the disagreement is recorded as a contradiction".
- **Effort budget**: see §2.
- **Confidentiality**: anything client or personal involved?

## 2. Effort budget and subagents

| question shape | do |
|---|---|
| single fact, one source | yourself; 3–10 tool calls |
| comparison of a few options, or a survey of a bounded field | 2–4 subagents, one per option or sub-question |
| broad, open, multi-domain | more subagents, each with one sub-question and clear boundaries |

Fan-out costs many times the tokens of a single agent. Use it when sub-questions
are independent and breadth matters, not for depth on one source.

Each subagent gets the brief in §8. It writes sources into the commonplace itself
(`ingest`, text layer, note) and returns only: the source ids it created, a
200-word summary per sub-question, the two or three strongest quotes with
locators, and open issues. Never ask a subagent to return whole documents.

## 3. Gather and ingest

For each source worth keeping, follow references/ingest.md (register, convert
with page anchors, spot-check, note, integrate). Rank what you keep by
`authority`. Discard, don't ingest, sources that only restate others.

## 4. Synthesise

Write or extend concept pages (one idea each) and a synthesis page that answers
the main question. Every sourced sentence cites. Where sources disagree, keep
both, cite both, set `contested`, add a `contradictions` entry. Say what is
unsourced. State confidence per sub-question.

Prefer tables for comparisons, with every cell traceable (the compare pack
generates matrices from option frontmatter).

## 5. Gap pass

Find the thinnest claim: the one with a single source, a secondary source, an
old source, or a quote that only partially supports it. Do one targeted search
for it. Either strengthen it or lower its confidence. Then stop; the stopping
rule in the plan decides, not the urge to keep reading.

## 6. Verify and review

```bash
$COMMONPLACE cite verify --changed            # must pass
$COMMONPLACE review prepare citation-auditor projects/<slug> resources/concepts/<new pages>
$COMMONPLACE review prepare skeptic projects/<slug>
```

Run each packet in a fresh-context subagent (another model where possible).
Record with `$COMMONPLACE review record`. Fix blockers; waive with a reason; file the
rest as open questions. Then `index`, `lint --changed`, `log add research`.

## 7. Express

Produce the deliverable (references/express.md): a synthesis page, an exported
report (`$COMMONPLACE export`), a pack deliverable (Architecture Vision, comparison
matrix, heads outline). Include a caveats section: what was not verified, what
is secondary, what is old.

## 8. Subagent brief template

```
You are researching one sub-question for a commonplace (a cited markdown wiki).
Commonplace root: <path>. Tool: <path to scripts/commonplace>. Read AGENTS.md first.

Sub-question: <...>
What counts as an answer: <...>
Boundaries: do not research <...>; stop after <N> sources or <time>.
Source preference: primary (<examples>) over secondary; record authority.

For each source you keep:
1. Save the original (HTML/PDF) and run: $COMMONPLACE ingest <file> --title ... --date ... --origin <url> --authority ... --confidentiality public
2. If needs_conversion, write the text layer with <!-- page:N --> markers and attach it.
3. Write the source note (TL;DR, summary, 5–10 key quotes with locators), status: summarised.
Do NOT create concept pages or edit existing synthesis pages; the lead integrates.

Return ONLY:
- source ids created
- ≤200 words answering the sub-question, with [[source id]] locator after each claim
- the 3 strongest verbatim quotes with locators
- disagreements between sources
- what you could not find
```

## 9. Quality rubric (for the skeptic and for yourself)

- Factual accuracy: do claims match the cited spans?
- Citation accuracy: does each quote support the sentence it is attached to, in context?
- Completeness: are all sub-questions in the plan addressed or explicitly open?
- Source quality: primary over secondary; dates stated; authority recorded.
- Balance: counter-evidence sought and recorded; confidence justified.
- Efficiency: no duplicate pages; existing pages extended; index and lint clean.
