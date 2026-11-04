# 07 — Failure handling and escalation

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

---

## 7. Failure handling and escalation

Applies while taking an issue from intake to committed fix. Push, PR, and API
failures are covered in
[create-pr's failure table](../../create-pr/references/05-failure-handling.md).

| Situation | Action |
| --- | --- |
| `GH_TOKEN` missing | Stop immediately. Ask the user to provide/export it. Do not start any phase or spawn subagents until it is set. |
| Cannot reproduce | Return to issue intake. Ask for arch, mode, SMP, image, and exact commands. Do not guess a fix. |
| Reproduction is flaky | Run multiple times, record pass rate, distinguish race from infrastructure, and test `SMP=1` vs `SMP=4`. |
| Build fails after a fix | Fix the build first; do not run the reproducer on a stale binary. |
| Fix makes the reproducer pass but root cause is unclear | Stop. A passing proxy is not a root cause. Return to Phase 2. |
| Reviewer finds a blocking issue | Return to Phase 3, correct it, and re-run the full Phase 4 matrix. |
| Fix requires a broad refactor | Split the work or ask for scope approval. Keep the bug-fix PR minimal. |
| Second independent bug appears | Record it, ask whether to expand scope, and do not silently include it. |
| Auth/push/PR failure | Stop and report the exact failure. Never print `GH_TOKEN` or switch to a tokenized URL. Push and PR failures are covered in [create-pr's failure table](../../create-pr/references/05-failure-handling.md). |
| Subagent infrastructure failure | Report the run state and partial diff. Do not silently switch execution modes. |
| No subagent can be launched | Stop and report that the workflow cannot proceed. Do not execute a phase inline; the gate evidence would be self-certified. |
| Base branch ambiguous | Ask. Do not default to `master` when the issue targets a feature branch. |
| User asks to publish before Phase 4 passes | Refuse and point at the missing gate evidence; publication requires verified commits, and the push belongs to the `create-pr` skill. |

---

Next: [08 — Issue analysis template](08-analysis-template.md)
