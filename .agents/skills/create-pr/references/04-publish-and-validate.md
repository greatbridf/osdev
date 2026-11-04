# 04 — Publish and validate

Part of the `create-pr` skill. Read [02 — Credential handling](02-credentials.md)
and [03 — PR title and body](03-pr-body.md) first.

---

## 4. Phase 3: Publish

The publisher is a `worker` subagent; the supervisor never runs `git push` or
`gh pr create` itself. Nothing in this phase may change code.

### 4.1 Publisher subagent task

```text
Objective
Publish the verified branch for issue #<id> as a pull request. Do not modify code.

Parameters
- Local cwd: <worktree>
- Target repository: <owner>/<repo>
- Base branch: <base>
- Head branch: <head>
- Remote to push to: <github|origin|fork>
- PR title file: build/work/issue-<id>/pr-title.txt
- PR body file: build/work/issue-<id>/pr-body.md

Authority
- You may run git push and the GitHub API/CLI.
- Do not edit source files, amend commits, rebase, force-push, or merge.
- Do not print or persist `GH_TOKEN`.
- If authentication fails or the base/head is ambiguous, stop and report.

Steps
1. Re-verify the local branch, HEAD sha, clean worktree, and intended diff.
2. Confirm `GH_TOKEN` is present in the environment. Do not read it from a file.
   Ensure it is exported; `gh` reads it directly.
3. Push the head branch to its remote. Use SSH if configured; otherwise use a
   temporary askpass helper that reads `GH_TOKEN`, never a tokenized URL.
4. Create a PR against the target repository with the given base and head.
   Include the verification evidence and `Fixes #<id>` from the body file.
5. Read back the PR: URL, number, base, head, head SHA, mergeable state, body.
6. Confirm the PR diff matches the local diff and contains no secrets or
   unintended files.
7. Report the PR URL, head SHA, checks observed, and any failure.

Stop conditions
- Push rejected and would require force.
- PR base/head mismatch.
- Token invalid or insufficient scope.
- PR diff contains files outside the intended change.
```

### 4.2 Push

```bash
git push --set-upstream "$REMOTE" "$HEAD"
```

- Never `--force` or `--force-with-lease` in this skill. If the push is
  rejected, stop; force-pushing is an authoring decision that requires re-running
  verification.
- Never push to a branch you did not expect. Confirm with `git rev-parse HEAD`
  before and after.

### 4.3 Create

```bash
gh pr create \
  --repo "$OWNER/$REPO" \
  --base "$BASE" \
  --head "$HEAD" \
  --title "$(cat "$ART/pr-title.txt")" \
  --body-file "$ART/pr-body.md"
```

Notes:

- If the head branch lives on a fork, use the fully qualified form required by
  the host, for example `--head <owner>:<branch>`.
- Omit `--draft` only when the change is ready for review. Use
  `--draft` when the user asked for a draft or verification is incomplete.
- If `gh` is unavailable, use the GitHub REST API with `GH_TOKEN` in the
  `Authorization: Bearer` header. Never place the token in the JSON body or the
  URL.

Capture the URL:

```bash
gh pr view --json url,number,baseRefName,headRefName,headRefOid,mergeable,body,title \
  > "$ART/pr-state.json"
```

## 5. Phase 4: Validate

### 5.1 Read back and compare

```bash
gh pr view "$PR" --json url,number,baseRefName,headRefName,headRefOid,mergeable,isDraft,title
gh pr diff "$PR" --patch | diff - <(git diff "github/${BASE}...HEAD") && echo "diff matches"
```

Confirm:

- Title matches `pr-title.txt`.
- Base and head are the intended branches.
- `headRefOid` equals the local `HEAD`.
- The PR diff equals the local diff.
- The body still carries the `Assisted-by: LLM` footer and `Fixes #<id>`:

  ```bash
  gh pr view "$PR" --json body --jq .body | grep -F 'Assisted-by: LLM'
  gh pr view "$PR" --json body --jq .body | grep -E '^(Fixes|Refs) #[0-9]+'
  ```

- No `build/`, logs, or credential material appear in the diff.
- CI checks started; record their names:

  ```bash
  gh pr checks "$PR" || true
  ```

### 5.2 If validation fails

- Wrong base or head: correct with `gh pr edit "$PR" --base <base>` if the
  branch contents are right; otherwise close and recreate. Ask first.
- Body wrong: `gh pr edit "$PR" --body-file "$ART/pr-body.md"`.
- Diff contains unintended files or commits: stop and return to authoring. Do
  not patch the PR by adding commits.
- CI fails: report the failing check and log. Do not weaken tests or the
  reproduction to make CI green.

### 5.3 Report

```text
PR: <url>
Repo: <owner>/<repo>
Head: <branch> @ <sha>
Base: <base>
Diff: matches local (verified)
Disclosure: commits OK, body footer OK
Verification cited: <bullets from the body>
CI: <pending/passing/failing — check names>
```

Record the final state in `build/work/issue-<id>/pr.md`:

```bash
printf '# %s\n\nPR: %s\nBase: %s\nHead: %s\n' \
  "$(cat "$ART/pr-title.txt")" "$PR_URL" "$BASE" "$HEAD" > "$ART/pr.md"
```

Do not merge the PR unless the user explicitly asks.

---

Next: [05 — Failure handling](05-failure-handling.md)
