---
name: create-pr
description: Publish a verified branch as a pull request, delegating body drafting, review, and the push to subagents. Never changes code.
---

# Create a Pull Request

Turns a branch that is already written and verified into a validated PR:
preflight, title and body, credential-safe push, `gh pr create`, read-back
check. It does not fix anything; not ready → [fix-and-pr](../fix-and-pr/SKILL.md).

Preflight (phase 0) is the supervisor's own work with no subagent: it decides
whether the branch may be published at all. Phases 1-3 are delegated; the
supervisor still does not draft the body, push, or call the API.

## Credential gate

Before preflight, remotes, or subagents:

```bash
test -n "${GH_TOKEN:-}"
```

If unset, stop and ask for `export GH_TOKEN='...'`. Never print, log, commit, or
write the token, and never put it in a prompt or a remote URL.

## Use when

- A verified branch must be pushed and published.
- An existing PR needs its title, body, base, or footer corrected.
- Another workflow has finished verification and hands off.

## Start

```bash
ISSUE=<id-or-slug>; BASE=<base-branch>; HEAD=$(git rev-parse --abbrev-ref HEAD)
ART="build/work/issue-${ISSUE}"; mkdir -p "$ART/reports"
```

## Gates

| Phase | Executed by | Gate to pass |
| --- | --- | --- |
| 0 Preflight | supervisor (no subagent) | Clean tree, intended commits, base/head confirmed, trailers correct, evidence on record, no secrets ([01](references/01-preflight.md)) |
| 1 Body | `worker` drafts, `reviewer` checks | Title and body written from real evidence, every claim matching the diff ([02](references/02-body.md)) |
| 2 Publish | publisher `worker` | Branch pushed, PR created against the right base/head ([03](references/03-publish.md)) |
| 3 Validate | publisher `worker` | PR read back: base, head, SHA, body, diff match local ([03](references/03-publish.md)) |

## Rules

1. Publication only: no source edits, amends, rebases, or force-pushes here.
2. Delegate phases 1-3; never draft the body, push, or call the API inline. No
   subagent → stop and report. Preflight is the supervisor's own, by design.
3. No verification evidence, no PR. Never invent a verification bullet.
4. Never leak `GH_TOKEN`.
5. `Assisted-by: LLM` goes on agent-written commits (before `Signed-off-by`) and
   on the body as a footer; human-written commits stay untouched.
6. Ask when the base is ambiguous; never assume `master`.
7. No `build/`, logs, keys, or local paths in the diff.
8. Never merge, auto-merge, or close without being asked.
9. Link an existing issue that matches, as `Fixes #<id>` or `Refs #<id>`; never
   invent a number. No match → no link line, and say so in the body worker's
   report.

## Artifacts

```text
build/work/issue-<id>/
├── pr-title.txt   pr-body.md   pr.md
└── reports/       # 01-body-worker.md, 01-body-reviewer.md, 02-publish-worker.md
```

## Report

PR URL and number, repo, base, head and SHA, title, verification cited, CI
status, diff-match confirmation, remaining risks.

Stop conditions: missing token, ambiguous base, unrelated commits, missing
trailer on an agent-written commit, no evidence, dirty tree, rejected push, PR
diff mismatch, secrets in the diff, failing CI. Report and stop; never paper
over it here.
