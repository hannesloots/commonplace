# Study pack: workflow

Learning compounds when course material is distilled into concept pages and
playbooks that both the user and the agent can use later, and when recall is
tested. The course is an area (ongoing learning), the certification is a
target, and each module is a cited distillation.

## Contents

1. Set up a course
2. Ingest the material
3. Module pages: distil
4. Concepts and playbooks: what becomes capability
5. Flashcards and exams
6. Certification tracking
7. Lint rules and traces

## 1. Set up

```bash
mkdir -p areas/learning/<course-slug>/{modules,flashcards}
$COMMONPLACE new course "<Course Title>" --folder areas/learning/<course-slug> --set 'provider=...' --set 'status=in-progress'
$COMMONPLACE new certificate "<Certification>" --folder areas/learning --set 'issuer=...' --set 'status=target'
```

List learning outcomes on the course page (STU-002); modules are checked
against them.

## 2. Ingest the material

Slides, transcripts, readings, textbooks and official exam guides are sources:
`$COMMONPLACE ingest <file> --title "<Course> - <Module>" --date <date> --kind slides|transcript|book
--confidentiality personal --authority primary`. Textbooks and standards:
split the text layer by chapter/section with page markers (references/ingest.md)
so citations are page-accurate. Licensed material stays in the commonplace only.

## 3. Module pages: distil

`$COMMONPLACE new module "<NN Module Title>" --folder areas/learning/<course>/modules --set 'course=["[[<Course Title>]]"]' --set 'order=NN'`

Write **Key ideas** in your words with citations to the material; **Procedures
learned** as numbered steps; **Questions I could not answer** (file as
`question` pages if they matter). When done, `status: done` and link the
concept and playbook pages in `concepts` (STU-001).

## 4. Concepts and playbooks: what becomes capability

For each durable idea: search first; extend an existing concept page or create
one with a claim-style title, cited to the module's sources. For each
procedure (a configuration, a method, a checklist, an exam technique): a
`playbook` with trigger, preconditions, steps, checks, pitfalls. Link the
module → concepts/playbooks; link concepts to the topic page for the field.
This is the point of the pack: next time the user, or the agent working for
them, needs the procedure, it is already written in their terms with sources.

## 5. Flashcards and exams

`flashcard` pages: one question, one answer, `concept` link (STU-003), cited.
Generate 5–15 per module from the key ideas. Review sessions: the agent asks,
the user answers, the agent grades and updates `last_reviewed`, `difficulty`.
Mock exams and real attempts as `exam` pages: result, weak domains, playbook
updates.

## 6. Certification tracking

The certificate page's syllabus table maps each exam domain to the modules and
concepts covering it with a confidence. `$COMMONPLACE query --where "type=module and status!=done"`
shows what is left. Set `status: scheduled` with the date, then `passed`.

## 7. Lint rules and traces

STU-001 module without produced concepts/playbooks (warning) · STU-002 course
without learning outcomes (info) · STU-003 flashcard without concept (warning).
Trace `course-to-concepts`. Persona `examiner` for module and flashcard review.
