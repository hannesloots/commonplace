# Review: persona reviews that catch what lint cannot

`commonplace lint` and `commonplace cite verify` prove structure: links resolve, quotes
exist inside their spans, frontmatter is complete. They cannot judge whether a
quote supports a sentence, whether a conclusion overreaches, whether two pages
are one concept, or whether the deliverable is usable. Personas do that, in a
fresh context, with evidence for every finding.

## Procedure

1. Choose personas. Minimum before anything leaves the commonplace: `citation-auditor`
   and `skeptic`. Add `librarian` after big ingests, `freshness-checker` for
   anything year-bound or legal, `reader` for deliverables, and the pack expert
   (`chief-architect`, `opposing-counsel`, `broker`, `examiner`).
2. Build the packet: `$COMMONPLACE review prepare <persona> <pages or folder>`. It
   contains the persona brief, each page, and every cited source span with the
   deterministic verdict and context lines, plus the required output format.
3. Run it in a subagent with a clean context. Pass the packet path, not the
   content, and ask for only the JSON array back. Prefer a different model
   family or at least a different model (`model:` on the Agent tool) for the
   skeptic; self-review under-catches. The subagent must not edit files.
4. Save the JSON to `reviews/<date> <persona> findings.json` and record:
   `$COMMONPLACE review record <persona> <findings.json> --scope "<pages>" --model <m>`.
   This writes a review page and stamps `reviews:` on each page.
5. Resolve every finding in the review page: **fix** (edit, re-verify),
   **waive** (one-line reason), or **open question** (file it). Blockers must
   be fixed or explicitly waived by the user.
6. If the same class of finding recurs, add a constraint to `error-book.md`.

## Bias controls

- Binary checklist items, not scores. "Does the quote entail the sentence: yes/partial/no."
- Every finding carries an `evidence` quote from the page or the source span; findings without evidence are discarded.
- For comparisons and rankings, run the review on both orderings and keep only findings that appear in both.
- The reviewer never sees the author's reasoning, only the pages and sources.
- Reviewers are told what they are not checking (lint already did it) so they spend attention on judgement.

## Finding format

```json
{"severity": "blocker|major|minor|note", "page": "resources/concepts/X.md",
 "location": "## Details, sentence starting 'Counsel must'", "claim": "...",
 "problem": "...", "evidence": "verbatim from page or source", "recommendation": "..."}
```

Severity: `blocker` = wrong or unsupported in a way that would mislead; `major`
= needs a fix before use; `minor` = improve when touched; `note` = information.

## When the binary is unavailable

Assemble the packet by hand: persona file + page text + for each citation the
40 lines around the quote in the text layer. Record findings by adding a
`reviews:` entry to the page frontmatter and writing the review page in `reviews/`.
