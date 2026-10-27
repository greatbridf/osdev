# 08 — Failure handling and escalation

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

---

## 7. Failure handling and escalation

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
| Auth/push/PR failure | Stop and report the exact failure. Never print `GH_TOKEN` or switch to a tokenized URL. |
| Subagent infrastructure failure | Report the run state and partial diff. Do not silently switch execution modes. |
| Base branch ambiguous | Ask. Do not default to `master` when the issue targets a feature branch. |

---

Next: [09 — Issue analysis template](09-analysis-template.md)
