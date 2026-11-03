---
type: error-book
updated: {{date}}
---

# Error book

Standing constraints learned from mistakes. Read this before any writing
session; every entry is a rule you follow. Add an entry when the same class of
mistake happens twice, or once if it was costly. Each entry: symptom, root
cause, the constraint, and how it is detected.

## Constraints

### E-001 A quote that "sounds right" is not a quote
- **Symptom:** `commonplace cite verify` fails because the quoted words were reconstructed from memory.
- **Root cause:** citing while synthesising instead of copying from `sources/text/`.
- **Constraint:** copy quotes from the text layer; run `commonplace cite find` to get the locator; never edit a quote to make it pass.
- **Detected by:** CITE-003 / CITE-004.

### E-002 A new page for something that already had one
- **Symptom:** two pages for one concept under different names.
- **Root cause:** skipping the search step before `commonplace new`.
- **Constraint:** `commonplace search "<name>"` and read aliases before creating anything; extend, don't create.
- **Detected by:** DUP-001, `commonplace dedupe`.
