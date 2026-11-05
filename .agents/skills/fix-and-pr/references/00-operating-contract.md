# 00 — Operating contract

Roles, briefs, artifacts, and stop conditions for every phase.

## Roles

| Role | Phases | May | Must not |
| --- | --- | --- | --- |
| supervisor | — | run preflight, write briefs, read reports, `git status`/`diff`/`log`/commit, decide gates | read kernel code, write scripts, edit sources, build, review own work, push |
| `scout` | 0, 2 | read, search, run read-only commands | edit anything, commit |
| `worker` | 1, 3 | edit only the files its brief names | touch other modules, commit unless told |
| `reviewer` | 1, 2, 3, 4 | read, run tests, report a verdict | edit code, approve work it wrote |
| verifier | 4 | build, run the matrix, write logs | edit kernel code |
| publisher | — | see [create-pr](../../create-pr/SKILL.md) | — |

The verifier is a `worker` or `reviewer` with a test-only brief. A reviewer may
edit a defective reproduction script only if its brief says so, and must report
every change.

## Briefs

Keep each brief under a page. Six parts:

1. **Objective** — one deliverable.
2. **Location** — cwd, ref, branch, base commit.
3. **Authority** — exact files or dirs writable, commands allowed.
4. **Inputs** — issue, logs, analysis, plan, or diff paths.
5. **Deliverable** — files to write plus `$ART/reports/<phase>-<role>.md`.
6. **Stop conditions** — ambiguity, auth failure, infrastructure failure, or a
   second independent bug.

The report carries the verdict, commands, exit codes, log paths, and open
questions. It is the gate evidence. Never put `GH_TOKEN` in a brief.

## Artifacts

```text
build/work/issue-<id>/
├── issue.json              # issue metadata and comments
├── analysis.md             # observations, hypotheses, root cause
├── plan.md                 # change boundary, verification matrix
├── script-review.md        # reproducer verdict
├── before.log              # --expect bug on the unmodified tree
├── after.log               # --expect fixed on the fixed tree
└── reports/                # one file per subagent run
    ├── 00-intake-scout.md
    ├── 01-repro-worker.md     01-repro-reviewer.md
    ├── 02-analysis-scout.md   02-analysis-reviewer.md
    ├── 03-plan-reviewer.md
    └── 04-verify-verifier.md  04-diff-reviewer.md
```

`build/` is git-ignored but `make clean` deletes it. Copy logs elsewhere before
a clean rebuild.

## Stop conditions

| Situation | Action |
| --- | --- |
| `GH_TOKEN` missing | Stop. Ask the user to export it. |
| No subagent can be launched | Stop and report; an inline phase would self-certify its gate. Preflight is the exception: the supervisor decides it. |
| Cannot reproduce | Record what was tried, ask for arch/MODE/SMP/image. Do not guess a fix. |
| Flaky reproduction | Repeat, record pass rate, compare `SMP=1` vs `SMP=4`. |
| Build fails after the fix | Fix the build; do not run the reproducer on a stale binary. |
| Test passes but root cause is unclear | Return to phase 2. A passing proxy is not a cause. |
| Reviewer finds a blocker | Return to phase 3, then re-run the whole phase 4 matrix. |
| Fix needs a broad refactor | Split it or get scope approval. |
| A second bug appears | Record it, ask, do not bundle it. |
| Auth, push, or API failure | Stop and report the error. Never print the token or retry with a tokenized URL. |
| Base branch ambiguous | Ask. Do not default to `master`. |

---

Next: [01 — Intake](01-environment-and-intake.md)
