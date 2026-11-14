---
type: skill-impact
---

# Skill impact

Every proposed change to a skill in `skills/`, accepted or rejected, with its
diff and the evidence behind it, and every recorded use. Written by
`commonplace skill accept`, `reject` and `used`; never edit or roll it back.
Read it before proposing a change, so an idea that was rejected is not
proposed again.

## [2026-11-03] accepted | verify-ai-citations
- reason: first version, from the Mavundla referral
- evidence: [[Fabricated citations breach the duty not to mislead the court]]
- files: PURPOSE.md, SKILL.md

```diff
--- /dev/null
+++ b/PURPOSE.md
@@ -0,0 +1,10 @@
+---
+type: skill
+title: "verify-ai-citations"
+---
+
+# Why Verify AI citations exists
+
+## Motivated by
+
+- [[Fabricated citations breach the duty not to mislead the court]]: why every step exists
--- /dev/null
+++ b/SKILL.md
@@ -0,0 +1,20 @@
+---
+name: verify-ai-citations
+description: Check every authority an AI tool produced before it goes into a court filing. Use when drafting or reviewing legal work that cites cases.
+---
+
+# Verify AI citations
+
+Courts treat an unverified fake citation as misleading the court, and refer
+counsel to the regulator. Never file an authority you have not opened.
+
+## Steps
+
+1. List every case, statute and quotation in the draft.
+2. Open each one in an official law report or court database.
+3. Check the citation, the paragraph and the quoted words match.
+4. Remove or replace anything you cannot find.
+
+## Checks
+
+Every authority in the draft has a source you opened, noted beside it.
```
