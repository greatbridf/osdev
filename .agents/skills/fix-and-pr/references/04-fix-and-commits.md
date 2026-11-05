# 04 — Fix

Phase 3. A `worker` writes the fix; the supervisor commits.

## Plan

`plan.md`: root cause, files and functions to change, explicit non-goals,
invariants to preserve, failure paths to keep, commit split, verification matrix,
risks.

| Case | Arch | MODE | SMP | Command | Expected |
| --- | --- | --- | --- | --- | --- |
| Baseline | x86_64 | release | 4 | `--expect bug` | 0 |
| Fixed | x86_64 | release | 4 | `--expect fixed` | 0 |
| Build | x86_64 | release | 4 | `make build` | 0 |
| Smoke | x86_64 | release | 4 | `script/test.sh` | 0 |

A fresh `reviewer` closes the plan gate and writes
`$ART/reports/03-plan-reviewer.md`: blocking issues first, then suggestions,
judged against the root cause and repository conventions. Missing lock,
lifetime, refcount, or error paths, missing architecture variants, and steps
broader than the cause are blocking.

## Worker brief

- Objective: implement `plan.md` for issue #<id>.
- cwd, branch `fix/<id>-<slug>`, base commit.
- Authority: only the named files or subsystem. No unrelated modules, no
  reformatting, no dependency changes, no surviving debug prints.
- No commit unless the supervisor asks.
- Read the code and `analysis.md` first, change minimally, keep error paths and
  SMP/preemption safety, run `cargo fmt --all -- --check` if the repo expects
  rustfmt.
- Build release and debug; add a second architecture when the change is shared.
- Reproduce: `--expect fixed` exits 0, and `--expect bug` still exits 0 on the
  baseline commit.
- Report changed files, commands, exit codes, and log paths.
- Stop if the plan is wrong or a second bug appears.

## Refine

```bash
git diff --stat; git diff; git diff --check
```

Ask: is every line necessary, are lock order and ownership intact, is it
race-free at the reported `SMP`, are arch cases handled, is user input
validated, does it change behavior the issue does not require, is leftover
instrumentation gone?

Split when it reads better: `prep:` (behavior-preserving), `fix:`, `test:`. Every
commit must build alone. Rebase only on a private branch:

```bash
git rebase -i "github/${BASE}"
git diff "github/${BASE}"...HEAD
```

---

Next: [05 — Commits](05-commit-conventions.md)
