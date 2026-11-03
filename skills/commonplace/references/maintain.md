# Maintain: lint, distill, weekly review, archive

A commonplace degrades in predictable ways: near-duplicate pages, orphans, dead
links, changelog-vs-reference drift, stale "currently", unbounded logs,
contradictions nobody resolved. The tool catches the mechanical ones; you
handle the semantic ones; a weekly rhythm keeps both small.

## After every writing session

```bash
$COMMONPLACE cite verify --changed
$COMMONPLACE index
$COMMONPLACE lint --changed          # fix every error; mention warnings
$COMMONPLACE log add <op> "<title>" --note "..."
git add -A && git commit -m "<op>: <title>"
```

## Weekly (or "clean up", "health check")

1. `$COMMONPLACE status`: inbox items, unsummarised sources, projects without next
   actions, open questions and contradictions.
2. `$COMMONPLACE lint` on the whole commonplace. Fix errors, triage warnings, note infos.
3. Graph pass (the tool nominates, you decide):
   - `$COMMONPLACE graph gaps`: wanted pages (linked but missing, ranked by how many
     pages want them), orphans, dead ends, sources no page cites, clusters
     with no topic/project/area page (write the map page), clusters resting
     on one source (thin evidence), bridges (review these first: an error
     there spreads).
   - `$COMMONPLACE graph suggest-links`: unlinked mentions with the line to link
     on. Add the `[[link]]` where the sentence really is about that page;
     skip incidental matches.
   - `$COMMONPLACE graph conflicts`: same-type page pairs sharing sources or
     subject. Read both; record a `contradictions` entry or supersede one.
4. Semantic pass the tool cannot do:
   - stale claims a newer source supersedes (search for the topic, compare dates);
   - concepts mentioned three or more times without a page (`graph gaps` shows
     the linked ones; grep the term for unlinked ones);
   - pages that should merge (`$COMMONPLACE dedupe`) or split (SIZE-001, two ideas in one title);
   - reference sections that still describe a superseded state while a log entry says otherwise;
   - gaps worth one targeted search; file as open questions if not done now.
5. Distill: process `inbox/` (ingest, file, or delete) and the week's `journal/`
   pages into memory, project logs, concepts and playbooks (references/memory.md).
6. Projects: every active project has a next action, or becomes `paused`, or
   is archived. Areas: anything past its `review_cadence` gets a look.
7. `overview.md`: rewrite the current state paragraph; it is read before every query.
8. `$COMMONPLACE log rotate` when `log.md` passes ~150 entries. `$COMMONPLACE index`.
9. Log a `lint` entry listing what changed and what remains open.

## Merging duplicates

Never automatic. Choose the survivor (better title, more sources). Move unique
content and citations across. Add the loser's name to the survivor's `aliases`.
`$COMMONPLACE rename "<Loser>" "<Survivor>"` fails if the survivor exists, so instead:
replace links by hand or with `$COMMONPLACE links backlinks "<Loser>"` to find them,
then set the loser `status: superseded`, `superseded_by: "[[Survivor]]"` and
leave a one-line body pointing across (keeps old links alive). Log it.

## Renaming

`$COMMONPLACE rename "<Old>" "<New>"` rewrites every link and keeps the old name as
an alias. Rename concept pages when the claim sharpens; that is progress, not churn.

## Archiving

`$COMMONPLACE archive "<Project>"` moves the folder under `archive/` and marks it
archived. Before archiving: promote reusable concepts to `resources/`, make
sure decisions are recorded, and confirm sources stay (they are global).
Archive is excluded from search unless `--include-archive`.

## Error book

When a mistake happens twice, or once expensively, add an entry: symptom, root
cause, constraint, detection. Prefer turning it into a lint rule in the dev
repo (`[lint] retired_terms`, a pack rule) over a prose rule nobody re-reads.

## Upgrading the schema

`commonplace init <root> --force` refreshes AGENTS.md standard sections (Local rules
kept), templates and packs. Then lint and fix. Tell the user what changed.
