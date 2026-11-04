# Fix-and-Commit reference library

Detailed reference for the GBOS/Eonix `fix-and-pr` skill: intake, reproduction,
analysis, fix, commits, and verification. The workflow ends with verified commits
on a local branch, which the separate [create-pr](../../create-pr/SKILL.md) skill
publishes.

Start with [00 — Operating contract](00-operating-contract.md), then read the
document for the phase you are entering. Read the next phase's document before
crossing its gate.

## Phase documents

| Phase | Document | Read it when |
| --- | --- | --- |
| 0. Intake and environment | [01 — Environment and issue intake](01-environment-and-intake.md) | Setting up the checkout, build, QEMU, branch, and issue record. |
| 1. Reproduction | [02 — Automated reproduction](02-reproduction.md) | Writing or verifying a headless QEMU reproducer. |
| 2. Analysis | [03 — Root-cause analysis](03-analysis.md) | Collecting evidence, mapping code, and proving the root cause. |
| 3. Fix | [04 — Fix planning and implementation](04-fix-and-commits.md) | Planning the change, writing it, and refining the diff. |
| 3. Commits | [05 — Commit conventions](05-commit-conventions.md) | Splitting and writing commits. |
| 4. Verification | [06 — Verification and review](06-verification.md) | Running the matrix, independent review, and final acceptance. |
| 5. Handoff | [create-pr](../../create-pr/SKILL.md) | Pushing the branch, writing the PR body, and publishing. |

## Supporting documents

| Document | Purpose |
| --- | --- |
| [07 — Failure handling](07-failure-handling.md) | Stop conditions, escalation, and recovery choices. |
| [08 — Issue analysis template](08-analysis-template.md) | Recording observations, hypotheses, and root cause. |
| [09 — Quick command reference](09-command-reference.md) | Copy-paste commands for build, debug, reproduction, and commit hygiene. |
| [10 — Prompt hygiene](10-prompt-hygiene.md) | Required structure for subagent handoffs. |

## Reading rules

1. Confirm `GH_TOKEN` is set before doing anything else. If it is missing, stop
   and ask the user; do not read further or start a phase.
2. Read [00 — Operating contract](00-operating-contract.md) first. It contains
   the roles, hard rules, phase gates, and artifact layout.
3. Load only the current phase document and any supporting document it points
   to.
4. Before leaving a phase, read the next phase document and confirm its gate
   evidence exists.
5. Do not skip the reproduction or verification documents; they define the
   evidence that makes the fix trustworthy.
6. Stop at Phase 5. Pushing and opening the PR is the
   [create-pr](../../create-pr/SKILL.md) skill's job.
7. Execute each phase in its subagent and check the report it writes. Do not
   run a phase inline; a self-certified gate is not evidence.
