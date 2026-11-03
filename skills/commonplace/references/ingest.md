# Ingest: from original to integrated knowledge

Ingest is where provenance is won or lost. The tool registers and hashes; you
convert when it cannot, then summarise and integrate. One source at a time
unless the user asks for batch mode.

## Contents

1. Register
2. Convert (decision tree) and PDFs by page and section
3. Spot-check
4. Write the source note (summary ladder)
5. Integrate (extend, don't create)
6. Close out
7. Special cases: web pages, email, images, media, course material

## 1. Register

```bash
$COMMONPLACE ingest <file> --title "<Short Title>" --date <YYYY-MM-DD published> \
  --origin "<URL or where it came from>" --kind <judgment|paper|brochure|...> \
  --authority primary --confidentiality <public|personal|confidential|privileged> \
  [--project "<Project Title>"]
```

Read the report: `text` present or `needs_conversion`; `fidelity`; `pages`;
warnings (duplicate, scanned PDF, missing page boundaries). For a URL, fetch
the page first, save the HTML or a clean markdown copy, then ingest the file
with `--origin <url>`; the tool converts HTML itself.

Never copy files into `sources/` by hand: an unregistered raw file has no hash
and lint reports SRC-003.

## 2. Convert

| file | tool does | if not available |
|---|---|---|
| .md .txt | copies (fidelity high) | — |
| .html | built-in conversion (high): drops scripts, styles, `nav` and `footer`; resolves relative links and images to absolute URLs against `--origin`, so always pass the origin for web pages | — |
| .pdf | pdftotext with `<!-- page:N -->` markers (medium); warns if text per page is tiny (scan) | convert yourself: read the PDF (Read tool), write the text layer by section with page markers; or run Docling/Marker if installed and add page markers |
| .docx .odt .rtf .epub | pandoc (high) or markitdown | read with the docx skill and write the text layer |
| .pptx .xlsx | markitdown | read and write a faithful table/markdown rendition |
| images, audio, video | nothing | describe/transcribe into the text layer, `fidelity: low`, say it is a transcription |

When you write the text layer yourself:
- Be faithful. No summarising, no "improving". Keep headings, numbered
  paragraphs, tables (as markdown tables), figure captions ("[Figure 3: ...]").
- Put `<!-- page:N -->` at every page boundary. For numbered paragraphs keep the
  `[26]` at line start (judgments) or add `<!-- para:26 -->`.
- For documents over ~40 pages or with a strong section structure, split per
  section: `sources/text/<id>/01 Introduction.md` … with `section`, `title`,
  `page_from`, `page_to` frontmatter (see references/schema.md). This is the
  user's established practice for standards and course books; it keeps each
  section loadable on its own while citations stay page-accurate.
- Then: `$COMMONPLACE source attach-text "<id>" --converter llm --fidelity medium`
  (low for scans or transcriptions).

Fidelity matters downstream: the citation auditor spot-checks `medium`/`low`
layers against the original before trusting a quote.

## 3. Spot-check

Open the original and the text layer side by side at three places: first page,
one table or numbered list, last page. Fix nothing in the text layer by hand;
if it is wrong, re-convert and `attach-text` again. Note the fidelity honestly.

## 4. Write the source note

`sources/notes/<id>.md` is the summary ladder (progressive summarisation,
done once, well):

- **TL;DR** (L1, ≤50 words): what it is and why it matters here.
- **Summary** (L2, ≤300 words): in your words, structure over prose.
- **Key quotes** (L3): 5–15 verbatim quotes with locators, chosen because you
  expect to cite them. Each on its own line: `> "…" — para 26`. These are the
  raw material for citations; copy them exactly from the text layer.
- **Structure**: the outline (tool pre-fills headings when it can) with page
  ranges, so later readers can jump.
- **Relevance and open questions**: which projects, what it settles, what it
  does not.

Fill frontmatter: `author`, `publisher`, `published`, `origin`, `kind`,
`authority`, `tags` (registered only), `projects`, `description` (one line),
`summary_level: 3`, `status: summarised`.

Discuss 3–5 takeaways with the user before integrating, unless in batch mode.
This is where they steer emphasis.

## 5. Integrate

A single source may touch 5–15 pages. For each candidate concept or entity:

1. `$COMMONPLACE search "<name>"`; check aliases; open the page if it exists.
2. Extend it: add the cited sentences, update `sources`, `updated`, maybe
   `confidence`. Record disagreements as contradictions, not overwrites.
3. Create only when it matters in two or more sources or is central to this
   one: `$COMMONPLACE new concept "<Claim-style title>" --set 'tags=[...]' --set 'sources=["[[<id>]]"]'`.
4. Link: from the relevant topic page, project page, and `related` of 3–8
   neighbours. Every new page needs at least one inbound link from a content page.
5. Update the project page's questions/log if this source answers something.
6. Set the note's `status: integrated`.

Citation discipline while writing: cite from the text layer, not from the note
(the note's quotes are copies; the text layer is the truth). When unsure of
the locator: `$COMMONPLACE cite find "<id>" "<quote>"`.

## 6. Close out

```bash
$COMMONPLACE cite verify --changed      # fix every FAIL by re-reading the source
$COMMONPLACE index
$COMMONPLACE lint --changed             # fix errors; report warnings
$COMMONPLACE log add ingest "<title>" --note "Created: [[A]], [[B]]" --note "Updated: [[C]]" --note "Contradictions: ..."
git add -A && git commit -m "ingest: <title>"
```

Report: pages created/updated, contradictions, questions raised, fidelity caveats.

## 7. Special cases

- **Web pages**: save a copy (HTML or clean markdown) at ingest time; pages
  change. `origin` is the URL, `retrieved` the date. Prefer primary sources
  (the regulator, the scheme, the court) over aggregators; set `authority`.
- **Email / chat exports**: `kind: email`, `confidentiality: confidential` by
  default; the text layer keeps headers (from, to, date) as a table.
- **Images, screenshots, scans**: describe what is visible, transcribe text,
  `fidelity: low`; keep the original. For scanned PDFs use OCR (Docling/Marker)
  when available.
- **Audio/video**: transcript as the text layer with `[hh:mm:ss]` markers every
  minute or paragraph; locators are timestamps.
- **Course material (study pack)**: ingest slides/transcripts/readings as
  sources; the module page is your distillation; each durable idea becomes or
  extends a concept page; each procedure becomes a playbook. See packs/study.
- **Legal material (legal pack)**: judgments from the jurisdiction profile's
  primary sources (SAFLII, AustLII, BAILII, ...) as HTML or PDF, kept public;
  client documents privileged. Authority pages are verified only after the
  judgment is ingested. See packs/legal.
- **Batch mode**: ingest many files, write notes for each, then one integration
  pass; checkpoint with the user every ~10 sources.
