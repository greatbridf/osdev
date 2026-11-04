# 05 — Failure handling

Part of the `create-pr` skill.

---

| Situation | Action |
| --- | --- |
| `GH_TOKEN` missing | Stop immediately. Ask the user to export it. Do not inspect remotes, push, or spawn subagents until the check succeeds. |
| Token invalid or under-scoped | Stop. Report the exact API error, without printing the token. Ask the user for a token with `repo` (or `contents: write` + `pull_requests: write`). |
| Base branch ambiguous | Ask. Do not default to `master` when the change targets a feature branch. |
| Branch contains unrelated or `wip` commits | Stop. Cherry-picking or rebasing is an authoring decision; return to the authoring workflow. |
| Commit written with agent help is missing `Assisted-by: LLM` | Stop. Messages are fixed by the author (amend/rebase) and verification is re-run afterwards; this skill does not rewrite history. |
| Commit is human-authored and has no `Assisted-by: LLM` | Expected. Leave it alone; the tag is a disclosure of agent authorship, not a requirement for every commit. |
| No verification evidence available | Ask the user whether to publish anyway with an explicit note, or return to authoring. Never fabricate evidence bullets. |
| Worktree dirty | Stop. Uncommitted changes belong to authoring. Do not `git add` or `git stash` here. |
| Push rejected (non-fast-forward) | Stop and report. Force-pushing is an authoring decision with a required verification re-run. |
| Push accepted but PR creation fails | Report the pushed head SHA and the API error. Retry only after the cause is known; do not push again with different refs. |
| PR created with wrong base/head | Ask before editing or closing. Fix the metadata if the branch content is correct, otherwise recreate. |
| PR diff differs from local diff | Stop. Do not add commits to "fix" the PR. Investigate whether the branch, base, or remote is wrong. |
| Secrets or build artifacts found in the diff | Do not publish. Return to authoring; the commit must be rewritten and the secret rotated if it was exposed. |
| CI fails | Report the failing check and log. Do not disable or weaken tests, and do not bypass required checks. |
| Subagent infrastructure failure | Report the run state, pushed refs, and any PR URL. Do not silently switch execution modes or retry blindly. |
| No subagent can be launched | Stop and report. Do not draft the body, push, or create the PR inline; the gate evidence would be self-certified. |

---

Back to [README.md](README.md).
