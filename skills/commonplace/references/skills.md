# Skills: the layer that makes the agent better at this user's work

A commonplace keeps three layers, after WikiSkill (Tang et al., Google and
Virginia Tech, 2026), which builds on Karpathy's LLM wiki:

| layer | here | changes how |
|---|---|---|
| raw | `sources/` (immutable) and the use records in `skill-impact.md` | appended, never edited |
| wiki | the commonplace: playbooks (what worked), `error-book.md` (what failed and the constraint it taught), concepts | you maintain it; never rolled back |
| skills | `skills/<name>/SKILL.md` + `PURPOSE.md` | one gated change at a time |

The paper's findings that shape the rules below: skills improved by an agent
that could read the accumulated wiki scored far higher than without it (63.7%
vs 48.7%); letting the agent read the wiki *while doing a task* made skills
worse; and keeping rejected changes on record stops the same bad idea from
coming back.

## Contents

- A skill's two files
- The loop after a task
- The gate
- Retiring and installing
- What not to do

## A skill's two files

`SKILL.md` is a standard Agent Skill: frontmatter `name` (the folder name) and
`description` (what it does and when to use it, in the words a request would
use; agents choose skills by it), then the procedure. It must stand alone:
copy in the facts and constraints it needs, no `[[links]]` (SKILL-005). Write
it as instructions to an agent: when to use, steps, checks, pitfalls. Extra
files (`references/`, `scripts/`) are allowed and installed with it.

`PURPOSE.md` is the skill's page in the wiki (`type: skill`, named after the
skill, so other pages link it as `[[weekly-review]]`); it is not installed for
agents. It links the playbooks, error-book entries and concepts each step came
from, so those pages show the skill in their backlinks and the graph, and
`rename` keeps the links current. Lint warns when a linked page is superseded
(SKILL-004): that is the signal to revise the skill.

## The loop after a task

Run it when a task used a skill, or repeated work a skill could have done.

1. **Record the use.** `commonplace skill used <name> --outcome ok|failed --note "<what happened>"`.
2. **Distil into the wiki.** A failure: root cause, then an error-book entry
   (symptom, root cause, constraint, how detected) or a fix to the playbook. A
   success worth repeating: update or create the playbook. The wiki changes
   whatever the gate later decides.
3. **Propose one change.** Read `skill-impact.md` for that skill first and do not
   re-propose a rejected idea unless new evidence answers the rejection reason.
   Then either `commonplace skill new <name> --description "..."` or edit
   `skills/<name>/`. One atomic change: one new skill, or one edit to one
   skill. Update `PURPOSE.md` to link what motivated it.
4. **Gate it** (below).

No change is a valid outcome. Most uses should end at step 2.

## The gate

The paper gates on a validation score; a commonplace has none, so the user is
the gate. Show `commonplace skill diff <name>`, say in two lines what it fixes
and which pages support it, and ask. Then:

```
commonplace skill accept <name> --reason "<what it fixes>" --evidence "<page>" --evidence "<page>"
commonplace skill reject <name> --reason "<why not>"
```

Both append the diff, reason and evidence to `skill-impact.md` and a line to
`log.md`. Accept stores the version in `.commonplace/skills/<name>/` and
installs it; reject restores the last accepted version (or removes a skill that
was never accepted; its text stays in the record). If the user has said a kind
of change needs no approval, record that under Local rules in AGENTS.md and
still record the decision.

## Retiring and installing

- Retire: delete `skills/<name>/`, then `skill accept <name> --reason "..."`;
  installed copies are removed.
- Install targets: `[skills] targets` in `commonplace.toml` (default
  `.claude/skills` for Claude Code, `.agents/skills` for Codex, Copilot CLI and
  other hosts), inside the commonplace, so skills load when an agent works
  here. `commonplace skill sync --user <name>` also installs a skill under the home
  folder for every project.
- `skill sync` reinstalls everything accepted (after a clone, or when lint
  reports SKILL-007) and lists pending proposals.

## What not to do

- Never edit `skill-impact.md`, `.commonplace/skills/` or an installed copy by
  hand; the record is what makes the loop learn.
- Never accept on the user's behalf or bundle several ideas into one proposal.
- Never point a skill at the wiki ("see the playbook"); copy what it needs.
- Never put secrets, personal data or privileged material in a skill: installed
  skills are read by any agent working in that folder, or everywhere with `--user`.
