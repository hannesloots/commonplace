# EA pack: workflow

Enterprise architecture work in the commonplace follows capability-based planning
through the usual stages (vision, business architecture, information and
technology architecture, gaps and roadmap). The stages align with the ADM
phases A–F of the TOGAF® standard, so a practitioner certified in it will find
the pages where they expect them; the pack itself is framework-neutral. TOGAF
is a registered trademark of The Open Group; this pack is not affiliated with
or endorsed by it.

The commonplace is the architecture repository: `resources/` holds the reference
library (standards, series guides and your organisation's principles, as
licensed sources), `projects/<engagement>/architecture/` is the architecture
landscape for that engagement, decisions are the governance log, and
`requirement` pages are the requirements repository.

Store any standard you rely on as a licensed source in `sources/raw` and cite
it; never paraphrase a standard from memory.

## Contents

1. Set up an engagement
2. Vision
3. Business architecture (use cases → value streams → capabilities)
4. Information and technology architecture: building blocks
5. Gaps, work packages, roadmap
6. Generated tables
7. Lint rules and traces
8. Deliverables

## 1. Set up an engagement

```bash
$COMMONPLACE new project "<Engagement>" --set 'packs=[ea]' --set 'outcome=Architecture Vision and Target Architecture approved by <board>'
mkdir -p projects/<engagement>/architecture/{motivation,business,requirements,building-blocks,gaps,roadmap}
```

Ingest the inputs first: strategy documents, current-state inventories,
interview notes (confidential), vendor documents, the standards and guides the
organisation follows. Every architecture claim cites one of them.

## 2. Vision

Pages: `driver`, `goal` (driven_by), `stakeholder` (concerns), `principle`,
scope statement in the project page. Then `$COMMONPLACE new architecture-vision
"Architecture Vision - <Engagement>" --folder projects/<engagement>/architecture`
(type `deliverable`, stage vision). Fill it last, from the pages.

## 3. Business architecture

Work from needs to capabilities:

1. **Use cases** (`use-case`: actor, trigger, outcome, main flow) from
   interviews and process documents. Cite.
2. **Value streams** (`value-stream`) where the engagement spans end-to-end
   value: stages with entrance/exit criteria.
3. **Capabilities** (`capability`): noun phrases, level 1–3, `parent`,
   `baseline_maturity` and `target_maturity` (0–5, state the scale you use and
   cite it), `heat`. Build the map top-down from a reference model if one is
   licensed, else bottom-up from the use cases.
4. Link: each use case `enabled_by` one or more capabilities (EA-001 enforces
   it); each value stream stage names its enabling capabilities.
5. Capability map: `$COMMONPLACE query --where "type=capability" --fields level,parent,baseline_maturity,target_maturity,heat`.

## 4. Information and technology architecture: building blocks

1. **ABBs** (`abb`, domain business/data/application/technology, `state`
   baseline/target/both): what function is needed, vendor-neutral. Each
   capability with a gap gets `realised_by` ABBs (EA-003).
2. **SBBs** (`sbb`): candidate or in-use products/services, `implements` ABBs
   (EA-004), lifecycle. Cite vendor documents for claimed features; mark
   vendor claims `authority: secondary` unless tested.
3. **Requirements and principles**: every ABB `satisfies` at least one (EA-005).

## 5. Gaps, work packages, roadmap

1. `gap` pages per capability or ABB where baseline ≠ target: `subject`,
   `gap_type` (new, enhance, retire, replace, carry-forward).
2. `work-package` pages that `closes` gaps, with `horizon` now/next/later and
   `depends_on`.
3. Roadmap deliverable from `$COMMONPLACE trace --name gap-to-roadmap` and the
   work-package table.

## 6. Generated tables

```bash
$COMMONPLACE trace --name use-case-to-capability      # coverage, gaps as —
$COMMONPLACE trace --name capability-to-sbb           # realisation chain
$COMMONPLACE trace --name requirement-coverage
$COMMONPLACE query --where "type=capability and target_maturity>baseline_maturity" --fields baseline_maturity,target_maturity,realised_by
$COMMONPLACE graph walk "<Capability>" --depth 2 --rel realised_by,implemented_by
$COMMONPLACE graph export --format mermaid --around "<Capability>" --depth 2   # diagram for the deliverable
```

Paste tables into deliverables; they are regenerated, never hand-edited.

## 7. Lint rules and traces

EA-001 use case without capability (error) · EA-002 target maturity below
baseline (warning) · EA-003 capability without ABB (warning) · EA-004 SBB
without ABB (error) · EA-005 ABB without requirement/principle (warning) ·
EA-006 gap without work package (warning) · EA-007 deliverable missing
standard sections (warning). (Rule ids were TOG-00n before 0.4.0; both
spellings are accepted in `disabled_rules`.)

## 8. Deliverables

Architecture Vision, Target Architecture and Roadmap templates carry the
standard sections. Before release: `cite verify`, personas
`citation-auditor`, `chief-architect`, `steering-committee`, `reader`; then
`$COMMONPLACE export`. Record approval as a `decision` page.
