---
name: fix-and-pr
description: Reproduce, debug, fix, verify, and submit GBOS/Eonix kernel issues as pull requests. Use for kernel bug fixing, headless QEMU reproduction scripts, subagent-driven root-cause analysis, commit preparation, and credential-safe PR creation.
---

# GBOS / Eonix Fix and PR

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

## When this skill applies

- A GitHub issue, bug report, stack trace, or failing test must be reproduced
  and fixed.
- A QEMU-based kernel reproduction script is needed.
- The work needs subagent orchestration for analysis, implementation, review,
  or PR publication.
- The user asks to prepare commits or open a PR for a verified fix.

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
   document when entering each phase.

## Phase gates

| Phase | Required evidence | Reference document |
| --- | --- | --- |
| 0. Intake and environment | `GH_TOKEN` present; clean checkout, configured toolchain, successful build, target QEMU boot, issue details captured. | [01 — Environment and issue intake](references/01-environment-and-intake.md) |
| 1. Reproduction | Headless script independently verified to distinguish bug from fixed behavior. | [02 — Automated reproduction](references/02-reproduction.md) |
| 2. Analysis | Root cause stated with evidence; fix boundary and invariants recorded. | [03 — Root-cause analysis](references/03-analysis.md) |
| 3. Fix and commits | Minimal scoped fix; clean, independently buildable commits with repository-style messages. | [04 — Fix planning and implementation](references/04-fix-and-commits.md), [05 — Commit conventions](references/05-commit-conventions.md) |
| 4. Verification and review | Baseline `--expect bug` passes, fixed `--expect fixed` passes, builds/tests pass, independent review has no blockers. | [06 — Verification and review](references/06-verification.md) |
| 5. PR | Correct base/head, pushed branch, PR body with verification evidence, local and PR diffs match, no secrets. | [07 — PR submission](references/07-pr-submission.md) |

Do not advance because a plausible fix exists. Advance only when the gate
evidence exists.

## Critical rules

1. **One writer per worktree.** Never run concurrent writers in the same
   checkout.
2. **No reproduction, no fix.** Do not implement a speculative fix.
3. **No post-fix verification, no PR.** The same reproducer must pass in fixed
   mode after the fix.
4. **Never use `git add -A` or `git add .`.** Add explicit paths only.
5. **Never leak `GH_TOKEN`.** Do not print it, pass it in prompts, write it to
   disk, embed it in URLs, or commit it.
6. **Keep the fix minimal.** No unrelated refactors, formatting sweeps, or
   dependency updates.
7. **Separate authoring from review.** Use a fresh-context `reviewer` for the
   independent diff review.
8. **Stop on ambiguity.** Ask before changing unspecified behavior or choosing
   a PR base branch.

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

- [08 — Failure handling](references/08-failure-handling.md)
- [09 — Issue analysis template](references/09-analysis-template.md)
- [10 — Quick command reference](references/10-command-reference.md)
- [11 — Prompt hygiene](references/11-prompt-hygiene.md)

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
└── pr.md
```

`build/` is ignored by Git, but `make clean` deletes it. Preserve important logs
elsewhere before a clean rebuild.

## Subagent roles

| Role | Use |
| --- | --- |
| `scout` | Read-only issue intake, code mapping, log analysis, hypothesis generation. |
| `worker` | One bounded implementation or script-generation task in one worktree. |
| `reviewer` | Fresh-context challenge of the reproduction script, root cause, plan, or final diff. |
| Publisher `worker` | Push the verified branch and create the PR; no code changes. |

The supervisor retains phase-gate decisions, final diff ownership, and
publication authority.

## Output expectations

When reporting progress or completion, include:

- Issue id and chosen base/head branches.
- Root cause and changed files.
- Baseline and fixed reproduction results with log paths.
- Build/test commands and exit codes.
- Independent review verdict.
- Commit hashes and PR URL, if created.
- Remaining risks or blockers.

READ ALL REFERENCE DOCUMENTATIONS UNDER THIS SKILL'S references/ DIRECTORY BEFORE CONTINUING!
