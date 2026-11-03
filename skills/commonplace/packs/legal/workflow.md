# Legal pack: workflow

A matter is a project. Everything in it is privileged by default. The commonplace
grounds you in the matter's documents and in verified authorities, so that
nothing fictitious reaches a court. Courts in several jurisdictions have
sanctioned practitioners for AI-fabricated citations; the rules below exist so
that cannot happen from this commonplace.

**Professional duty statement.** The commonplace is a research aid. The practitioner
verifies every authority and every quotation against the primary source before
filing or advising. Nothing here is legal advice.

## Contents

0. Jurisdiction
1. Confidentiality and where to run
2. Open a matter
3. Documents: evidence and chronology
4. Issues and case theory
5. Authorities: the verification gate
6. Arguments and drafts
7. Lint rules and traces
8. Citation formats and court codes

## 0. Jurisdiction

The pack is jurisdiction-neutral; a profile supplies the country specifics.
`commonplace.toml` names it (`jurisdiction = "za"`), and the profile lives at
`.commonplace/packs/legal/jurisdictions/<code>.yaml`. Shipped: `za` (South Africa),
`au` (Australia), `en` (England and Wales). Read the profile before working in
a matter: it gives the citation patterns the lint gates use, the court codes,
the hierarchy, the primary sources, the citator, the regulator, the privacy
law and the drafting conventions.

- Not set: `commonplace doctor` says so and generic patterns apply (any neutral-style
  citation still needs a verified page, but court codes and report series are
  unknown). Ask the practitioner which jurisdiction, then
  `$COMMONPLACE pack install legal --jurisdiction <code>`.
- Another country: copy `generic.yaml` to `<code>.yaml`, fill it from the
  practitioner's knowledge and the courts' own citation guides, set
  `jurisdiction`, run `$COMMONPLACE lint` to prove the patterns parse. Propose the
  profile back to the skill's maintainer so others get it.
- Foreign authority (a case from another system) is persuasive at most; mark
  `hierarchy_rank: 5` and say so in the authority page.

## 1. Confidentiality and where to run

Run matters locally (Claude Code or Cowork on the practitioner's machine), in
a commonplace folder outside synced drives, one commonplace per client or practice. Client
documents: `--confidentiality privileged`. Judgments and statutes: `public`.
Never export or paste privileged pages without the practitioner's instruction;
`commonplace export` withholds them by default. Minimise personal information under
the profile's privacy law: describe, don't copy, identity numbers and medical
details unless needed.

## 2. Open a matter

```bash
$COMMONPLACE new matter "<Short name v Short name>" --folder projects/<matter-slug> \
  --set 'court=...' --set 'case_number=...' --set 'client_role=applicant' --set 'stage=pleadings'
mkdir -p projects/<matter-slug>/{parties,chronology,issues,evidence,authorities,arguments,drafts}
```

Then `party` pages for each party, witness and representative.

## 3. Documents: evidence and chronology

Ingest every document in the matter (pleadings, affidavits, annexures,
correspondence, expert reports, transcripts) with `--confidentiality privileged
--kind document`. Scanned bundles need OCR (Docling/Marker) or a manual text
layer; keep page markers so pinpoints are `p.N` of the bundle.

- `evidence` page per exhibit: what it is, what it proves (cited pinpoints),
  weaknesses, admissibility. `exhibit_ref` is the bundle reference.
- `chronology-event` page per material event, `date`, cited to the exhibit or
  affidavit paragraph (LEG-005). The chronology table:
  `$COMMONPLACE query --where "type=chronology-event" --fields date,description --format table` (sort by date).

## 4. Issues and case theory

`issue` pages: the question, elements to prove (table: element, evidence,
authority, status), our position, theirs. The matter page carries the case
theory in one paragraph and links the issues.

## 5. Authorities: the verification gate

For each case, statute or regulation you intend to rely on:

1. Fetch the primary text from a source the profile names (`primary_sources`).
   Save it. Ingest with `--confidentiality public --kind judgment|statute
   --origin <url>`.
2. `$COMMONPLACE new authority "<Case name>" --folder projects/<matter>/authorities`.
   Fill `neutral_citation` and `report_citation` in the profile's forms
   (`citation_examples`), `court` (one of the profile's codes), `judge`,
   `date`, `source_url`, `hierarchy_rank` (profile's `hierarchy`, 1 = apex).
3. Write the ratio with pinpoints as citations:
   `[^1]: [[2025-01-08 Case v Case]] para 26 "..."`. Run `$COMMONPLACE cite verify`.
4. Check subsequent treatment with the profile's `treatment_tool`; set
   `treatment`.
5. Only now set `verified: true` and `verified_on`. If secondary sources
   disagree on a citation (it happens), set `confidence: contested` and resolve
   on the primary text before verifying.

LEG-001/LEG-002 make any citation in `drafts/` without a matching verified
authority page a lint **error**. Do not disable them.

## 6. Arguments and drafts

`argument` pages: proposition, `addresses` issues, `supported_by` authorities
and evidence, anticipated counter-argument. Drafts (`draft` type: heads,
skeleton, submissions, opinion, memo, letter, affidavit, witness statement)
cite only verified authorities with pinpoints, in the profile's drafting
conventions. Before anything goes to the practitioner for filing:

```bash
$COMMONPLACE lint projects/<matter>/drafts        # LEG-001/002 must be clean
$COMMONPLACE cite verify projects/<matter>/drafts
$COMMONPLACE review prepare citation-auditor projects/<matter>/drafts
$COMMONPLACE review prepare opposing-counsel projects/<matter>/drafts projects/<matter>/arguments
$COMMONPLACE review prepare senior-counsel projects/<matter>/drafts
```

## 7. Lint rules and traces

LEG-001 neutral citation in a draft without verified authority (error) ·
LEG-002 law-report citation without verified authority (error) · LEG-003
authority without neutral citation (error) · LEG-004 treatment not recorded
(warning) · LEG-005 chronology event without citation (error) · LEG-006
argument without support (warning) · LEG-007 references to secondary AI
citation tools (error) · LEG-008 matter page not privileged/confidential.
The patterns behind LEG-001, LEG-002 and LEG-007 and the court enum come from
the jurisdiction profile. Traces: `issue-to-authority`, `argument-support`.

## 8. Citation formats and court codes

In the profile: `citation_examples` (neutral, report, statute, section,
pinpoint), `courts`, `hierarchy`. Give both the neutral and the reported
citation where a report exists. Pinpoint to numbered paragraphs where the
court uses them; otherwise `p.` with the page marker of the ingested text.
