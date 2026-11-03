---
type: registry
updated: {{date}}
---

# Tag registry

Only tags under **Active** may be used. `commonplace lint` flags anything else. Keep
the list short: a tag earns its place by grouping pages that links alone do not.

## Rules

- Lowercase kebab-case, one namespace level: `namespace/name`.
- Namespaces: `topic/` (subject areas, at most 5 per page) and `todo/` (work flags).
- Single-valued facts (type, status, confidence, kind) are properties, not tags.

## Active

### topic/ — subject areas
- `topic/general` — fallback when nothing more specific fits yet

### todo/ — work flags (remove when done)
- `todo/verify` — contains a claim that needs checking or a better source
- `todo/expand` — thin page that deserves more detail
- `todo/merge` — may duplicate another page; say which in the body
- `todo/split` — covers too much; should become several pages
- `todo/source` — has claims with no citation
- `todo/review` — needs a persona review before use

## Proposed

Format: `- \`topic/name\` — definition | why existing tags don't fit | pages: [[A]], [[B]], [[C]]`

- (none)

## Retired

Old tag → replacement. When retiring, update every page that used it.

- (none)

## Adding a tag

1. Propose only when at least 3 existing or incoming pages would use it and no
   active tag fits.
2. Add it under **Proposed** with definition, reason and pages.
3. Mention it in your report. Do not apply it yet.
4. When the user approves, move it to **Active**, apply it, and log a `tags` entry.
