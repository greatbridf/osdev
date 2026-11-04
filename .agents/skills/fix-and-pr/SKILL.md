---
name: fix-and-pr
description: Reproduce, fix, verify, and commit GBOS/Eonix kernel issues, running every phase in a subagent. Publishing is the create-pr skill.
---

# GBOS / Eonix Fix and Commit

Takes a kernel issue to a verified fix committed on a local branch. Publishing
is [create-pr](../create-pr/SKILL.md)'s job.

Every phase runs in a subagent. The supervisor writes briefs, checks the reports
they produce, and owns the commits. It does not read kernel code, write scripts,
edit sources, run builds, or review its own work.

## Credential gate

Before anything else, including repo inspection, builds, and subagents:

```bash
test -n "${GH_TOKEN:-}"
```

If unset, stop and ask for `export GH_TOKEN='...'`. Never print, log, commit, or
write the token, or put it in a prompt.

## Use when

- An issue, bug report, stack trace, or failing test needs reproducing and fixing.
- A headless QEMU reproducer is needed.
- Commits must be prepared for a verified fix.

Pushing or opening a PR → [create-pr](../create-pr/SKILL.md).

## Start

```bash
ISSUE=<id>; SLUG=<slug>; BASE=<base-branch>
ART="build/work/issue-${ISSUE}"; mkdir -p "$ART/reports"
```

Read [references/00-operating-contract.md](references/00-operating-contract.md),
then the document for the current phase.

## Phase gates

| Phase | Executed by | Gate to pass |
| --- | --- | --- |
| 0 Intake | `scout` | Clean checkout, build works, target boots, issue captured ([01](references/01-environment-and-intake.md)) |
| 1 Reproduction | `worker`, verified by `reviewer` | Script distinguishes bug from fixed ([02](references/02-reproduction.md)) |
| 2 Analysis | `scout`, challenged by `reviewer` | Root cause with evidence, fix boundary ([03](references/03-analysis.md)) |
| 3 Fix | `worker`, plan checked by `reviewer`, supervisor commits | Minimal fix, every commit builds alone ([04](references/04-fix-and-commits.md), [05](references/05-commit-conventions.md)) |
| 4 Verification | verifier, then fresh `reviewer` | Bug and fixed runs pass, builds pass, no blocking findings ([06](references/06-verification.md)) |
| 5 Handoff | supervisor | Clean branch, base agreed, evidence recorded |

A gate passes on the report the named subagent writes. No report, no gate.

## Rules

1. Delegate every phase; never run one inline. If no subagent can be launched,
   stop and report.
2. One writer per worktree.
3. No reproduction, no fix. No post-fix verification, no handoff.
4. `git add` explicit paths only.
5. Never leak `GH_TOKEN`.
6. Minimal diff: no refactors, formatting sweeps, or dependency bumps.
7. Author and review with different agents.
8. `Assisted-by: LLM` on agent-written commits, immediately before
   `Signed-off-by`. Human-written commits stay untouched.
9. No pushes, PRs, or merges from this skill.
10. Ambiguity → ask.

## Reproducer contract

`script/repro/issue-<id>-<slug>.exp` with `--expect bug|fixed --arch --mode
--smp --timeout --log --help`. Exit `0` match, `1` mismatch, `2` infrastructure,
`3` usage, `130` interrupted. Headless via `make test-run`, unique QEMU name,
never `killall qemu-system-*`. Details in [02](references/02-reproduction.md).

## Report

Issue id, base/head, root cause, changed files, before/after results with log
paths, build and test exit codes, review verdict, commit hashes, remaining risks.
End by handing off to `create-pr`.
