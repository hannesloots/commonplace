# Memory: the second-brain side

The commonplace is also the user's (and your) long-term memory: who they are, what
they prefer, who matters, what was decided, what they learned, what worked.
The research machinery (sources, citations) applies to external knowledge;
memory pages are about the user and are mostly first-hand, so they are dated
rather than cited.

## Contents

- What lives where
- Capture → journal → distill
- Facts: dated, superseded, never deleted
- Playbooks: how learning compounds
- Interplay with Claude Code's own memory
- Sensitivity

## What lives where

| | page | content |
|---|---|---|
| who they are | `memory/Profile.md` | roles, expertise, responsibilities, context |
| how they want work done | `memory/Preferences.md` | corrections and confirmed approaches, with the why |
| words as this commonplace uses them | `memory/Glossary.md` | term — meaning — defining page |
| people | `memory/people/<Name>.md` (`type: person`) | relationship, role, dated facts, interactions |
| decisions | `memory/decisions/<Title>.md` or inside a project (`type: decision`) | ADR: context, options, decision, consequences |
| days | `journal/YYYY-MM-DD.md` | happened, decided, learned, follow-ups |
| standing responsibilities | `areas/<slug>/` | the standard to maintain, current state, recurring questions |
| unresolved | `questions/` + `open-questions.md` | what would resolve it |

## Capture → journal → distill (CODE)

- **Capture** anything that resonates into `inbox/` (`commonplace new capture "<title>"`)
  or straight into today's journal page (`commonplace new journal`). Capture is
  cheap; do not organise at capture time.
- **Organise** weekly: each capture is ingested as a source, folded into a
  page, or deleted. Projects first (actionable), then areas, then resources.
- **Distill**: durable things move up: a fact about the user → Profile or
  Preferences; a person → people/; a decision → decisions/; an idea → concept;
  a procedure → playbook. Journal pages stay as the day's record.
- **Express**: when asked "what did we decide about X" or "how do I usually do
  Y", the answer comes from decisions and playbooks, with dates.

## Facts: dated, superseded, never deleted

Write facts as dated lines: `- 2026-11-03: prefers Rust for tools that others
will run unchanged (reason: no runtime to install)`. When a fact changes, add
the new dated line and mark the old one `(superseded 2026-12-01)`. The history
is the point: it lets you say "you used to prefer X; since December, Y".

Ask before recording anything sensitive (health, finances, other people's
legal matters). Mark such pages `confidentiality: confidential`.

## Playbooks: how learning compounds

A playbook is a procedure that worked, written so it can be followed next time:
trigger, preconditions, steps, checks, pitfalls. Sources of playbooks: a course
module (study pack), a project retrospective, a recurring request, a mistake
in the error book. Update `times_used`/`last_used` when applied; retire
playbooks that stop working with `status: superseded`. Playbooks are the
"skill layer": they make you more capable in this user's world, and they make
the user more capable because the procedure is written in their terms with
their sources.

## Interplay with Claude Code's own memory

Claude Code keeps a small auto-memory per project (`MEMORY.md` + topic files)
for how to work with the user. That stays what it is. The commonplace is the user's
knowledge and the long record. Rule of thumb: a preference about how to run
sessions → auto-memory and `memory/Preferences.md` (one line each); a fact
about the user's life, work, people, decisions, learning → the commonplace only.
When a commonplace is the working directory, its `CLAUDE.md` imports `AGENTS.md`,
so the schema loads automatically.

## Sensitivity

Memory pages default to `confidentiality: personal`. `commonplace export` withholds
confidential and privileged pages. Never paste memory pages into prompts for
third-party services. If the user shares the commonplace with someone else (a
another person's comparison, a colleague's project), keep their material in a
separate commonplace; commonplaces are cheap.
