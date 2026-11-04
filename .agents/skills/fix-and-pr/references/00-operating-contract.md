# 00 — Operating contract

Part of the GBOS/Eonix `fix-and-pr` skill.

A standardized, end-to-end workflow for reproducing kernel issues, finding the
root cause, implementing and verifying a fix, and committing it to a clean local
branch. The separate [create-pr](../../create-pr/SKILL.md) skill publishes that
branch as a pull request.

The workflow is deliberately **evidence-gated**. Do not advance to the next
phase because a plausible fix exists; advance only when the previous phase has
produced verifiable evidence.

---

## 0. Operating contract

### 0.1 Roles

| Role | Owner | Executes | Responsibility |
| --- | --- | --- | --- |
| Supervisor | Parent agent | Gate checks, commits | Writes phase briefs, checks reports against the gates, owns commits and the final diff, resolves ambiguity. |
| Scout | `scout` subagent (read-only) | Phases 0, 2 | Fetches issue context, maps code paths, collects logs, proposes hypotheses. |
| Worker | `worker` subagent (writer) | Phases 1, 3 | Writes the reproducer and implements one bounded change in one worktree. |
| Reviewer | `reviewer` subagent (read-only, fresh context) | Phases 1, 2, 3, 4 | Independently challenges the reproduction script, root cause, plan, or diff. |
| Verifier | `worker` or `reviewer` with a test-only mandate | Phase 4 | Runs the reproduction and builds; does not edit kernel code. |

The publisher role lives in the [create-pr](../../create-pr/SKILL.md) skill.

The supervisor does not execute phases. Reading code, writing scripts, editing
sources, running builds and reproductions, and reviewing are delegated; the
supervisor's own tool use is limited to writing briefs, reading the reports those
briefs produce, and git operations at a gate (status, diff, log, commit). If a
subagent cannot be launched, stop and report that the workflow cannot proceed
rather than doing the work inline.

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

1. **Delegate execution.** Each phase runs in the subagent named for it; the
   supervisor briefs and gates. Never substitute inline supervisor work for a
   phase because it looks faster.
2. **One writer per worktree.** Never let two subagents edit the same checkout
   concurrently.
3. **No fix without reproduction.** If the issue cannot be reproduced, record
   why and stop; do not implement a speculative fix.
4. **No handoff without post-fix verification.** The same reproduction that
   failed before the fix must pass after it.
5. **Separate evidence from authority.** A subagent report is evidence. The
   supervisor decides whether it is sufficient.
6. **Never leak credentials.** `GH_TOKEN` is read from the environment only.
   Never print it, put it in a URL, a prompt, a commit, a log, or a file.
7. **Never use `git add -A` or `git add .`.** Explicitly add intended files;
   otherwise an untracked secret or log can enter the commit.
8. **Minimal scope.** No unrelated refactors, formatting sweeps, or speculative
   improvements in a bug-fix PR.
9. **Disclose AI assistance.** A commit produced with agent assistance carries
   an `Assisted-by: LLM` trailer in its trailer block, immediately before
   `Signed-off-by`. A commit the maintainer wrote alone carries nothing: the tag
   describes authorship, so never add it to human work. The `create-pr` skill
   applies the same test and adds the PR body footer. See
   [05 — Commit conventions](05-commit-conventions.md).
10. **Stop on ambiguity.** Ask before changing behavior that is not clearly
    required by the issue.
11. **Do not publish.** No pushes, no PRs, no merges from this skill.

### 0.4 Phase gates

| Phase | Executed by | Gate to leave the phase |
| --- | --- | --- |
| 0. Intake and environment | `scout` | `GH_TOKEN` is present; repository builds, target architecture runs, issue details captured. |
| 1. Reproduction | `worker`, verified by a fresh `reviewer` | A headless script exists and has been independently verified to distinguish bug from fixed behavior. |
| 2. Analysis | `scout`, challenged by a `reviewer` | Root cause is stated with evidence; the fix boundary is identified. |
| 3. Fix and commits | `worker` implements, `reviewer` critiques the plan, supervisor commits | Minimal fix is implemented; commits are clean, build independently, and those written with agent assistance carry the `Assisted-by: LLM` trailer. |
| 4. Verification and review | verifier `worker`/`reviewer`, then a fresh `reviewer` | Reproduction passes in fixed mode, baseline passes in bug mode, builds/tests pass, independent review has no blocking findings. |
| 5. Handoff | supervisor | Clean local branch, agent-authored commits carrying the `Assisted-by: LLM` trailer and human commits left as they are; the base branch agreed with the user; the verification evidence recorded for the PR body. Publishing happens in the [create-pr](../../create-pr/SKILL.md) skill. |

A gate is passed on evidence, not on the supervisor's own reading of the code: the
named subagent's report is what the supervisor checks.

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
└── reports/                # one report per subagent run
    ├── 00-intake-scout.md
    ├── 01-repro-worker.md
    ├── 01-repro-reviewer.md
    ├── 02-analysis-scout.md
    ├── 03-plan-reviewer.md
    ├── 04-verify-worker.md
    └── 04-diff-reviewer.md
```

Every brief ends with the report path its subagent must write. A gate is checked
against that report, so a phase without one has not been executed.

The [create-pr](../../create-pr/SKILL.md) skill adds `pr.md` here.

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
