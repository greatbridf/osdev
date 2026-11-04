# 01 — Preflight

Run every check before any push. The supervisor does this; it needs no subagent.

## Credential

```bash
test -n "${GH_TOKEN:-}"   # never print the value
```

## Repo and range

```bash
git status --short --branch; git remote -v
git fetch --all --prune
git log --oneline "github/${BASE}..HEAD"
git diff --stat "github/${BASE}...HEAD"
git merge-base --is-ancestor "github/${BASE}" HEAD
```

- Intended checkout and head branch, clean worktree (ignored paths aside).
- Base is the branch this lands on; ambiguous → ask. Behind base → ask, do not
  rebase here.
- Only intended commits: no `wip`, `fixup!`, or unrelated carrying commits.
- Diff holds only files that belong to the change.

## Trailers

```bash
git log --format='%h %G? %s%n  %(trailers:key=Assisted-by,valueonly)' \
  "github/${BASE}..HEAD"
git diff --check
```

Per commit, not per branch: an agent-written commit prints `LLM`; a
maintainer-written commit prints nothing and must not be given the tag. Where
present, order is `Fixes:`/`Link:`, `Assisted-by: LLM`, `Signed-off-by:`.
An agent-written commit missing it → stop; fix messages in authoring, then
re-verify. Cannot tell who wrote a commit → ask.

## Evidence

```bash
ls -1 build/work/issue-<id>/ 2>/dev/null
```

Need build/test output, before/after reproduction logs, or an explicit statement
from the user. Nothing available → ask whether to publish with a "not
independently verified" note, or return to authoring. Never invent it.

## Secrets

```bash
git diff "github/${BASE}...HEAD" | grep -nEi \
  'ghp_[A-Za-z0-9]{20,}|github_pat_|BEGIN [A-Z ]*PRIVATE KEY|GH_TOKEN=' || true
git diff --name-only "github/${BASE}...HEAD"
```

No `build/`, logs, `.env`, keys, or `/Users/...` and `/home/...` paths.

## Checklist

- [ ] Token set, never printed
- [ ] Clean tree, intended branch
- [ ] Base agreed, head intended
- [ ] Only intended commits
- [ ] Agent-written commits tagged; human commits untouched
- [ ] Evidence on record
- [ ] Diff free of secrets and artifacts

---

Next: [02 — Body](02-body.md)
