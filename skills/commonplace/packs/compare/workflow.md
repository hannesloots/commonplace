# Compare pack: workflow

For comparisons where getting a value wrong costs money: medical schemes,
insurance, vendors, products. Every cell in the comparison traces to the
provider's own document for the right year.

## Contents

1. Set up the comparison
2. Needs profile and criteria
3. Options: one page per option per year
4. Scenarios
5. The matrix
6. Recommendation and caveats
7. Lint rules
8. Medical schemes (South Africa) notes

## 1. Set up

```bash
$COMMONPLACE new project "<Comparison> <year>" --set 'packs=[compare]' --set 'outcome=Decide between options for <year>'
$COMMONPLACE new needs-profile "Needs - <who>" --folder projects/<slug>
$COMMONPLACE new comparison "<Comparison> <year>" --folder projects/<slug> --set 'year=<year>'
```

Ingest the primary documents first: each provider's current-year brochure,
benefit schedule and rules; the regulator's guidance (for SA medical schemes:
the Council for Medical Schemes circulars and the PMB list). Aggregator
articles are `authority: tertiary` and never the sole source for a value
(CMP-002).

## 2. Needs profile and criteria

The needs profile holds the situation (`confidentiality: confidential`):
members, conditions, expected usage, budget, must-haves, deal-breakers. From
it derive 5–10 criteria; list them in the comparison page's `criteria` field
with weights. Each criterion becomes a frontmatter field name on option pages
(snake_case, e.g. `monthly_contribution_main`, `chronic_cover`, `day_to_day`).

## 3. Options

One `option` page per option per year (`year` field; CMP-001 flags old years).
Put each criterion's value in frontmatter and, in the body, a line per value
with its citation to the brochure page:

```
- monthly_contribution_main: R 4,950[^1]
[^1]: [[2025-11-01 Scheme X Benefit Guide 2026]] p.12 "Main member R4 950"
```

Set `verification: brochure` when every value is cited to the provider's own
document, `regulator` when cross-checked, `provider-confirmed` after a call or
written confirmation. Record exclusions, waiting periods, late-joiner
penalties, network/DSP restrictions, co-payments in the body.

## 4. Scenarios

`scenario` pages describe concrete events (a hospital admission, a chronic
medicine month, two GP visits a month, a planned procedure) and cost them under
each option in a table, citing the benefit rule used (CMP-003). Probability
weight them in the comparison.

## 5. The matrix

```bash
$COMMONPLACE query --where "type=option and year=<year>" --fields provider,plan,monthly_contribution_main,hospital_cover,chronic_cover,day_to_day,verification
```

Paste into the comparison page. Cells from unverified options are marked
`(unverified)`. Add the scenario totals table.

## 6. Recommendation and caveats

Recommend against the needs profile, weighted criteria and scenarios; show the
runner-up and what would flip the decision. Caveats: values are for `year`,
benefits change annually, confirm with the provider; the standard disclaimer
line (CMP-004). Reviews: `citation-auditor`, `freshness-checker`, `broker`,
`sceptical-buyer`.

## 7. Lint rules

CMP-001 option year old (warning) · CMP-002 values not verified against
provider/regulator (warning) · CMP-003 scenario without cited rules (warning)
· CMP-004 missing disclaimer (info).

## 8. Medical schemes (South Africa) notes

- Primary sources: each scheme's benefit guide and rules for the benefit year;
  Council for Medical Schemes (CMS) circulars on contribution increases and
  the Prescribed Minimum Benefits (PMB) list. PMBs are a baseline every
  registered scheme must cover; cite the CMS page for the current count.
- Contribution changes apply from 1 January (some schemes defer); record the
  effective date on the option page.
- Criteria worth including: contribution per member type, hospital cover and
  rate (100%–300% of scheme rate), designated service providers and
  penalties, chronic cover (PMB CDL and additional), day-to-day/savings,
  co-payments and deductibles, late-joiner penalties, gap cover interaction,
  waiting periods.
