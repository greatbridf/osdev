# Fix-and-Commit references

Phase documents for the `fix-and-pr` skill. Read
[00 — Operating contract](00-operating-contract.md) first, then the current
phase.

| Phase | Document | Read it when |
| --- | --- | --- |
| 0 Intake | [01](01-environment-and-intake.md) | Preflight, build, branch, capturing the issue |
| 1 Reproduction | [02](02-reproduction.md) | Writing and verifying a headless reproducer |
| 2 Analysis | [03](03-analysis.md) | Collecting evidence and proving the root cause |
| 3 Fix | [04](04-fix-and-commits.md) | Planning and implementing the change |
| 3 Commits | [05](05-commit-conventions.md) | Message, trailers, staging |
| 4 Verification | [06](06-verification.md) | Matrix, independent diff review, done checklist |
| 5 Handoff | [create-pr](../../create-pr/SKILL.md) | Publishing the branch |

Each phase runs in a subagent named by the gate table in the skill. Its report
under `$ART/reports/` is the gate evidence; a phase run inline has not happened.

[00](00-operating-contract.md) also holds the roles, brief format, artifact
layout, and stop conditions.
