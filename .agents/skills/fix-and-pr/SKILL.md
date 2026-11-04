---
name: fix-and-pr
description: Reproduce, debug, fix, verify, and commit GBOS/Eonix kernel issues by delegating every phase to a subagent (scout, worker, reviewer). Use for kernel bug fixing, headless QEMU reproduction scripts, root-cause analysis, and commit preparation. Publishing the result as a pull request is a separate skill: create-pr.
---

# GBOS / Eonix Fix and Commit

This skill takes a kernel issue from intake to a verified fix committed on a
local branch: reproduce, prove the root cause, implement, verify, commit.
Publishing that branch as a pull request belongs to the
[create-pr](../create-pr/SKILL.md) skill.

Every phase runs in a subagent. The supervisor writes the phase brief, checks the
returned report against the gate, and owns the commits and the final diff; it does
not read kernel code, write scripts, edit sources, or run builds itself.

## Mandatory credential gate

Run this before **any** other action: preflight, branch creation, issue
fetching, builds, analysis, or subagent launches.

1. Check `GH_TOKEN` without printing it:

   ```bash
   test -n "${GH_TOKEN:-}"
   ```

2. If the check succeeds, continue.
3. If `GH_TOKEN` is unset or empty, stop immediately. Ask the user for a GitHub
   token, preferably by exporting it:

   ```text
   GH_TOKEN is not set. Please provide a GitHub token by exporting it in your
   shell, for example:
     export GH_TOKEN='...'
   Tell me when it is set; I will not continue until then.
   ```

4. Do not run tools, inspect the repository, spawn subagents, or begin the
   workflow until the user confirms it is available and the check succeeds.
5. Never echo, log, commit, or put `GH_TOKEN` in a subagent prompt. Do not
   write it to a file.

The detailed protocol is split by phase under `references/`. Read
`references/README.md`, then `references/00-operating-contract.md`, then the
document for the current phase. Load only the documents needed for that phase.

If the task is not a GBOS/Eonix kernel issue, keep the same phase gates but
replace repository-specific paths, build commands, and remotes.

Commits written with an agent's help are AI-assisted and must say so: add
`Assisted-by: LLM` as the last trailer before `Signed-off-by`. Commits authored
by the maintainer alone carry no such tag. The `create-pr` skill applies the same
rule to commits and adds the PR body footer. See
[05 — Commit conventions](references/05-commit-conventions.md).

## When this skill applies

- A GitHub issue, bug report, stack trace, or failing test must be reproduced
  and fixed.
- A QEMU-based kernel reproduction script is needed.
- The work needs subagent orchestration for analysis, implementation, or
  review.
- The user asks to prepare commits for a verified fix.

For pushing a branch and opening a pull request, use
[create-pr](../create-pr/SKILL.md).

## Entry point

```bash
ISSUE=<issue-id>
SLUG=<short-slug>
BASE=<base-branch>
ART="build/work/issue-${ISSUE}"
mkdir -p "$ART"
```

Then:

1. Confirm the credential gate above passes.
2. Read `references/README.md` and `references/00-operating-contract.md`.
3. Run the repository preflight in `references/01-environment-and-intake.md`.
4. Capture the issue in `$ART/issue.json`.
5. Follow the phase gates below in order, loading the matching reference
   document when entering each phase. Phase 5 is the handoff to
   [create-pr](../create-pr/SKILL.md).

## Phase gates

| Phase | Executed by | Required evidence | Reference document |
| --- | --- | --- | --- |
| 0. Intake and environment | `scout` | `GH_TOKEN` present; clean checkout, configured toolchain, successful build, target QEMU boot, issue details captured. | [01 — Environment and issue intake](references/01-environment-and-intake.md) |
| 1. Reproduction | `worker` writes the script, fresh `reviewer` verifies it | Headless script independently verified to distinguish bug from fixed behavior. | [02 — Automated reproduction](references/02-reproduction.md) |
| 2. Analysis | `scout`, plus a `reviewer` challenge | Root cause stated with evidence; fix boundary and invariants recorded. | [03 — Root-cause analysis](references/03-analysis.md) |
| 3. Fix and commits | `worker` implements, `reviewer` critiques the plan, supervisor commits | Minimal scoped fix; clean, independently buildable commits with repository-style messages; every commit written with agent assistance carries an `Assisted-by: LLM` trailer. | [04 — Fix planning and implementation](references/04-fix-and-commits.md), [05 — Commit conventions](references/05-commit-conventions.md) |
| 4. Verification and review | verifier `worker`/`reviewer`, then a fresh `reviewer` | Baseline `--expect bug` passes, fixed `--expect fixed` passes, builds/tests pass, independent review has no blockers. | [06 — Verification and review](references/06-verification.md) |
| 5. Handoff | supervisor | A clean local branch with verified commits, and the base branch agreed with the user. Publishing that branch is the `create-pr` skill's job. | [create-pr](../create-pr/SKILL.md) |

Do not advance because a plausible fix exists. Advance only when the gate
evidence exists, and only when it came from the subagent named for that phase.

## Critical rules

1. **Delegate every phase.** Phases 0-4 are executed by the subagents named in
the gate table; the supervisor only writes briefs, checks reports, and commits.
If a subagent cannot be launched, stop and report that the workflow cannot run
instead of doing the work inline.
2. **One writer per worktree.** Never run concurrent writers in the same
   checkout.
3. **No reproduction, no fix.** Do not implement a speculative fix.
4. **No post-fix verification, no handoff.** The same reproducer must pass in
   fixed mode after the fix.
5. **Never use `git add -A` or `git add .`.** Add explicit paths only.
6. **Never leak `GH_TOKEN`.** Do not print it, pass it in prompts, write it to
   disk, embed it in URLs, or commit it.
7. **Keep the fix minimal.** No unrelated refactors, formatting sweeps, or
   dependency updates.
8. **Author and review with different agents.** The `reviewer` that clears a
gate never wrote the artifact it reviews.
9. **Tag AI-assisted work, and only that.** A commit written with agent help
   gets `Assisted-by: LLM` in its trailer block immediately before
   `Signed-off-by`. A commit the maintainer wrote alone is left untouched: do
   not add the tag to it, and do not hand off an agent-written commit that lacks
   it.
10. **Do not publish.** This skill does not push branches, open PRs, or merge.
    That is the `create-pr` skill's job.
11. **Stop on ambiguity.** Ask before changing unspecified behavior.

## Reproduction contract

Save the script at:

```text
script/repro/issue-<id>-<slug>.exp
```

It must support:

```text
--expect bug|fixed
--arch <arch>
--mode <mode>
--smp <n>
--timeout <seconds>
--log <path>
--help
```

Exit codes:

| Code | Meaning |
| --- | --- |
| `0` | Observed behavior matched `--expect` |
| `1` | Observed behavior contradicted `--expect` |
| `2` | Infrastructure failure |
| `3` | Usage error |
| `130` | Interrupted |

Use `make test-run` for headless execution. Give QEMU a unique name and kill
only that instance; never use global `killall qemu-system-*`.

Before trusting the script, have a fresh verifier confirm:

- `--expect bug` exits 0 on the unmodified tree.
- `--expect fixed` exits 1 on the unmodified tree (negative control).
- No QEMU process remains after any exit path.
- Logs contain no secrets and are bounded.

## Supporting references

- [07 — Failure handling](references/07-failure-handling.md)
- [08 — Issue analysis template](references/08-analysis-template.md)
- [09 — Quick command reference](references/09-command-reference.md)
- [10 — Prompt hygiene](references/10-prompt-hygiene.md)

## Artifacts

Keep issue state outside chat history:

```text
build/work/issue-<id>/
├── issue.json
├── analysis.md
├── plan.md
├── script-review.md
├── before.log
├── after.log
└── reports/            # one report per subagent run, one per phase gate
```

The PR record (`pr.md`) and the base/head decision belong to the `create-pr`
skill.

`build/` is ignored by Git, but `make clean` deletes it. Preserve important logs
elsewhere before a clean rebuild.

## Subagent roles

| Role | Phases | Use |
| --- | --- | --- |
| `scout` | 0, 2 | Read-only issue intake, code mapping, log analysis, hypothesis generation. |
| `worker` | 1, 3 | One bounded implementation or script-generation task in one worktree. |
| `reviewer` | 1, 2, 3, 4 | Fresh-context challenge of the reproduction script, root cause, plan, or final diff. |
| verifier (`worker` or `reviewer`, test-only) | 4 | Runs the reproduction matrix and builds; does not edit kernel code. |

Each brief ends with a report path under `$ART/reports/` that the subagent must
write, and that report is the gate evidence. The supervisor retains only
gate decisions, the commit, and final diff ownership; push and merge authority
sits with the `create-pr` skill.

## Output expectations

When reporting progress or completion, include:

- Issue id and chosen base/head branches.
- Root cause and changed files.
- Baseline and fixed reproduction results with log paths.
- Build/test commands and exit codes.
- Independent review verdict.
- Commit hashes and the local branch name to hand off.
- Remaining risks or blockers.

End the report with the handoff: run the `create-pr` skill with the base branch,
head branch, and issue id to publish.

READ ALL REFERENCE DOCUMENTATIONS UNDER THIS SKILL'S references/ DIRECTORY BEFORE CONTINUING!
