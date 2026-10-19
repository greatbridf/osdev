# 00 — Operating contract

Part of the GBOS/Eonix `fix-and-pr` skill.

A standardized, end-to-end workflow for reproducing kernel issues, finding the
root cause, implementing and verifying a fix, and opening a pull request with
subagents.

The workflow is deliberately **evidence-gated**. Do not advance to the next
phase because a plausible fix exists; advance only when the previous phase has
produced verifiable evidence.

---

## 0. Operating contract

### 0.1 Roles

| Role | Owner | Responsibility |
| --- | --- | --- |
| Supervisor | Parent agent | Owns issue interpretation, phase gates, final diff, commits, and PR. Resolves ambiguity and approves publication. |
| Scout | `scout` subagent (read-only) | Fetches issue context, maps code paths, collects logs, proposes hypotheses. |
| Worker | `worker` subagent (writer) | Implements one bounded change in one worktree. |
| Reviewer | `reviewer` subagent (read-only, fresh context) | Independently challenges the reproduction script, diff, or root cause. |
| Verifier | `worker` or `reviewer` with a test-only mandate | Runs the reproduction and builds; does not edit kernel code. |
| Publisher | `worker` subagent | Pushes the branch and creates the PR. Does not change code. |

The supervisor may perform any step directly when that is cheaper than
delegation. Subagents are used to obtain isolation, independent verification,
and parallelism, not as ceremony.

### 0.2 Mandatory credential gate

Before any other workflow action, check `GH_TOKEN` without printing it:

```bash
test -n "${GH_TOKEN:-}"
```

- If it is set and non-empty, continue.
- If it is unset or empty, stop and ask the user for the token. Prefer asking
  them to export it in their shell:

  ```text
  GH_TOKEN is not set. Please provide a GitHub token by exporting it:
    export GH_TOKEN='...'
  Tell me when it is set; I will not continue until then.
  ```

- Do not inspect the repository, fetch issues, build, spawn subagents, or begin
  any phase until the user confirms it is available and the check succeeds.
- Never echo, log, commit, or put `GH_TOKEN` in a prompt. Do not write it to a
  file.

### 0.3 Non-negotiable rules

1. **One writer per worktree.** Never let two subagents edit the same checkout
   concurrently.
2. **No fix without reproduction.** If the issue cannot be reproduced, record
   why and stop; do not implement a speculative fix.
3. **No PR without post-fix verification.** The same reproduction that failed
   before the fix must pass after it.
4. **Separate evidence from authority.** A subagent report is evidence. The
   supervisor decides whether it is sufficient.
5. **Never leak credentials.** `GH_TOKEN` is read from the environment only.
   Never print it, put it in a URL, a prompt, a commit, a log, or a file.
6. **Never use `git add -A` or `git add .`.** Explicitly add intended files;
   otherwise an untracked secret or log can enter the commit.
7. **Minimal scope.** No unrelated refactors, formatting sweeps, or speculative
   improvements in a bug-fix PR.
8. **Stop on ambiguity.** Ask before changing behavior that is not clearly
   required by the issue, or when the target base branch is unclear.

### 0.4 Phase gates

| Phase | Gate to leave the phase |
| --- | --- |
| 0. Intake and environment | `GH_TOKEN` is present; repository builds, target architecture runs, issue details captured. |
| 1. Reproduction | A headless script exists and has been independently verified to distinguish bug from fixed behavior. |
| 2. Analysis | Root cause is stated with evidence; the fix boundary is identified. |
| 3. Fix and commits | Minimal fix is implemented; commits are clean and build independently. |
| 4. Verification and review | Reproduction passes in fixed mode, baseline passes in bug mode, builds/tests pass, independent review has no blocking findings. |
| 5. PR | Branch is pushed, PR is created with the correct base/head and verification evidence, and the local diff matches the PR diff. |

### 0.5 Persistent artifacts

Use issue-specific paths so later subagents can resume without relying on chat
history:

```bash
ISSUE=123
SLUG=short-issue-slug
ART="build/work/issue-${ISSUE}"
mkdir -p "$ART"
```

```text
build/work/issue-<id>/
├── issue.json              # issue metadata and comments, if fetched
├── analysis.md             # observations, hypotheses, root cause
├── plan.md                 # fix plan and test matrix
├── script-review.md        # independent reproduction-script verdict
├── before.log              # --expect bug run on the unmodified tree
├── after.log               # --expect fixed run on the fixed tree
└── pr.md                   # final PR title/body and URL
```

Reproduction scripts belong in `script/repro/`, because they may be reused by
later subagents and optionally committed as regression tests:

```text
script/repro/issue-<id>-<slug>.exp
```

`build/` is ignored by Git, but `make clean` deletes it. Copy important logs
out before a clean rebuild, or keep the canonical logs in a temporary directory
such as `/tmp/gbos-issue-<id>/`.

---

Next: [01 — Environment and issue intake](01-environment-and-intake.md)
