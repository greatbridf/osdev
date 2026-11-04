# 01 — Preflight and evidence

Part of the `create-pr` skill. Read [README.md](README.md) and the skill's
`SKILL.md` for the hard rules before using this document.

---

## 1. Preflight

Publication is irreversible enough that the checks come first. Run all of them
before any push or API call.

### 1.1 Credential gate

```bash
test -n "${GH_TOKEN:-}"   # must succeed; never print the value
```

If this fails, stop and ask the user to export the token.

### 1.2 Repository state

```bash
git rev-parse --show-toplevel
git status --short --branch
git log --oneline -n 20
git remote -v
```

Confirm:

- You are in the intended checkout, not a stray worktree.
- The worktree is clean. Only `build/` or other ignored paths may differ.
- The branch is the intended head branch, not a detached HEAD by accident.

If untracked files exist that should be part of the PR, they belong to the
authoring step, not here. Do not `git add` in this skill.

### 1.3 Base and head

```bash
BASE=<base-branch>
git fetch --all --prune
git log --oneline "github/${BASE}..HEAD"
git diff --stat "github/${BASE}...HEAD"
git merge-base --is-ancestor "github/${BASE}" HEAD && echo "base is ancestor"
```

Confirm:

- The base branch exists and is the branch the change is meant to land on. Do
  not assume `master`; feature work may target a subsystem branch.
- The branch is not behind in a way that makes the PR diff misleading. If the
  base has moved, rebase/merge is an authoring decision — ask, do not do it here.
- The commit list contains only intended commits (no `wip`, no `fixup!`, no
  unrelated carrying commits).
- `git diff` shows only files that belong to the change.

Ask the user when the base is ambiguous or when the diff contains unrelated
files.

### 1.4 Commit hygiene and disclosure trailers

```bash
git log --format='%h %G? %s%n  %(trailers:key=Assisted-by,valueonly)' \
  "github/${BASE}..HEAD"
git log -1 --format=%B
git diff --check
```

The `Assisted-by:` trailer is a disclosure of authorship, so the test is per
commit, not per branch:

- A commit written with agent assistance must print `LLM`.
- A commit the maintainer wrote alone prints nothing, and must not be given the
  tag. Do not "normalise" mixed history by adding it everywhere.

When the tag is present, the trailer block order is `Fixes:`/`Link:` first, then
`Assisted-by: LLM`, then `Signed-off-by:` last. The tag is exactly:

```text
Assisted-by: LLM
```

If a commit that the agent wrote is missing it, stop. Commit messages are fixed
by the author (amend or interactive rebase) before publication, and that path
also requires re-running verification. Do not rewrite messages while running this
skill, and do not amend a human-authored commit to add the tag.

If you cannot tell whether a commit was agent-written (for example a maintainer
follow-up squashed onto the branch), ask instead of guessing.

### 1.5 Verification evidence

Collect proof from the authoring session, usually under
`build/work/issue-<id>/`:

```bash
ls -1 build/work/issue-<id>/ 2>/dev/null
```

Look for `before.log`, `after.log`, `analysis.md`, `plan.md`, build/test output,
or an explicit user statement. At least one of the following must be true:

- The change was built and tested, and the commands and results are recorded.
- A reproduction or regression test passed before and after.
- The user states the verification that was performed and accepts responsibility.

If nothing is available, do not invent it. Say so, and either ask the user
whether to publish with an explicit "not independently verified" note, or return
to the authoring workflow. A PR that claims verification it does not have is
worse than no PR.

### 1.6 Secret and artifact scan

```bash
git diff --name-only "github/${BASE}...HEAD"
git diff "github/${BASE}...HEAD" | grep -nEi \
  'ghp_[A-Za-z0-9]{20,}|github_pat_|BEGIN [A-Z ]*PRIVATE KEY|GH_TOKEN=' || true
```

Confirm no build artifacts, logs, `.env` files, keys, or machine-local absolute
paths (`/Users/...`, `/home/...`) entered the diff.

### 1.7 Preflight checklist

- [ ] `GH_TOKEN` is set and was never printed.
- [ ] Worktree is clean and in the intended checkout.
- [ ] Base branch confirmed with the user, not assumed.
- [ ] Head branch is the intended one.
- [ ] Commit range contains only intended commits.
- [ ] Every agent-written commit carries `Assisted-by: LLM` in the correct
      trailer position.
- [ ] Human-authored commits were left untouched (no tag added).
- [ ] Verification evidence exists and is recorded.
- [ ] Diff contains no secrets, artifacts, or unrelated files.

Only after the whole checklist passes, continue to
[02 — Credential handling](02-credentials.md).

---

Next: [02 — Credential handling](02-credentials.md)
