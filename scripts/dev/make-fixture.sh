#!/bin/sh
# Rebuild fixtures/demo deterministically (used by CI: it must lint clean).
set -e
cd "$(dirname "$0")/../.."
B="${COMMONPLACE_BIN:-target/debug/commonplace}"
rm -rf fixtures/demo fixtures/tmp && mkdir -p fixtures/tmp
cat > fixtures/tmp/judgment.md <<'EOT'
# Mavundla v MEC (test extract)

[25] The applicant's counsel relied on seven authorities.

[26] It emerged that counsel lazily omitted to verify the research produced by the tool, and that the cases did not exist.

[27] The conduct is referred to the Legal Practice Council.
EOT
cat > fixtures/tmp/guide.html <<'EOT'
<html><body><h1>Business Capabilities Guide</h1><p>Each capability enables one or more value stages. A capability map is one of the four core elements of Business Architecture.</p><h2>3.2 Maturity</h2><p>Maturity is assessed on a five point scale.</p></body></html>
EOT
$B init fixtures/demo --name "Demo Commonplace" --purpose "Fixture commonplace for tests: a legal concept and a small enterprise architecture slice." --pack ea,legal --jurisdiction za
R=fixtures/demo
$B --root $R ingest fixtures/tmp/judgment.md --title "Mavundla v MEC" --date 2025-01-08 --origin "https://www.saflii.org/za/cases/ZAKZPHC/2025/2.html" --confidentiality public --kind judgment
$B --root $R ingest fixtures/tmp/guide.html --title "TOGAF Business Capabilities Guide" --date 2022-04-25 --confidentiality public --kind document
$B --root $R new topic "Legal Research" --set 'description=Map of pages about legal research and AI' --set 'scope=topic/law' --set 'tags=[topic/law]'
$B --root $R new concept "Fabricated citations breach the duty not to mislead the court" --set 'tags=[topic/law]' --set 'sources=["[[2025-01-08 Mavundla v MEC]]"]' --set 'description=Courts refer counsel to the LPC for fake AI citations' --set 'status=growing' --set 'up=["[[Legal Research]]"]'
cat >> "$R/resources/concepts/Fabricated citations breach the duty not to mislead the court.md" <<'EOT'

The court found that counsel had not checked the authorities the tool produced.[^1] The matter was referred to the regulator.[^2]

[^1]: [[2025-01-08 Mavundla v MEC]] para 26 "counsel lazily omitted to verify the research produced by the tool"
[^2]: [[2025-01-08 Mavundla v MEC]] para 27 "referred to the Legal Practice Council"
EOT
$B --root $R new project "Demo Engagement" --set 'description=A small enterprise architecture slice' --set 'outcome=A target architecture for onboarding' --set 'next_actions=[Write the vision]' --set 'packs=[ea]'
$B --root $R new capability "Customer Onboarding" --folder projects/demo-engagement/architecture/business --set 'tags=[topic/general]' --set 'sources=["[[2022-04-25 TOGAF Business Capabilities Guide]]"]' --set 'description=Onboard new customers end to end' --set 'realised_by=["[[Onboarding Workflow Service]]"]'
$B --root $R new use-case "Open an account" --folder projects/demo-engagement/architecture/business --set 'tags=[topic/general]' --set 'enabled_by=["[[Customer Onboarding]]"]' --set 'description=A customer opens an account online' --set 'actor=Customer' --set 'sources=["[[2022-04-25 TOGAF Business Capabilities Guide]]"]'
$B --root $R new abb "Onboarding Workflow Service" --folder projects/demo-engagement/architecture/building-blocks --set 'tags=[topic/general]' --set 'description=Orchestrates onboarding steps' --set 'satisfies=["[[Straight-through onboarding]]"]' --set 'sources=["[[2022-04-25 TOGAF Business Capabilities Guide]]"]'
$B --root $R new requirement "Straight-through onboarding" --folder projects/demo-engagement/architecture/requirements --set 'tags=[topic/general]' --set 'description=Onboarding completes without manual steps' --set 'priority=must' --set 'sources=["[[2022-04-25 TOGAF Business Capabilities Guide]]"]'
# register the tag
sed -i 's/^- `topic\/general` — fallback when nothing more specific fits yet$/- `topic\/general` — fallback when nothing more specific fits yet\n- `topic\/law` — legal research, courts, professional duties/' $R/TAGS.md
# mark sources summarised with a description
for f in "$R/sources/notes/"*.md; do sed -i "s/^status: converted$/status: summarised/; s/^description: ''$/description: 'Fixture source'/" "$f"; done
$B --root $R index >/dev/null
$B --root $R lint --fail-on error
rm -rf fixtures/tmp
echo "fixture ok"
