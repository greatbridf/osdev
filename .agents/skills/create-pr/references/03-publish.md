# 03 — Publish and validate

Phases 2 and 3, run by a publisher `worker`. Nothing here may change code.

## Publisher brief

- Objective: push the branch and open the PR. No code, commit, or message
  changes.
- Parameters: cwd, repo, base, head, remote, `$ART/pr-title.txt`,
  `$ART/pr-body.md`.
- Authority: `git push` and the GitHub CLI/API only. No amend, rebase,
  force-push, or merge. Never print or persist `GH_TOKEN`.
- Steps: verify branch, SHA, clean tree, and diff; confirm `GH_TOKEN` in the
  environment; push; create the PR; read it back; compare the PR diff with the
  local diff.
- Report URL, SHA, checks, and any failure to
  `$ART/reports/02-publish-worker.md`.
- Stop on: rejected push, base/head mismatch, bad token, or a PR diff with files
  outside the change.

## Credentials

Preferred: SSH remote for the push, token only for API calls. Otherwise `gh`
handles an HTTPS remote. Last resort, REST with the token in an
`Authorization: Bearer` header.

- No `echo "$GH_TOKEN"`, no `printenv`, no `set -x` near credentials.
- No token in a URL: it persists in `.git/config` and shell history.
- No token in a brief, body, commit, comment, or log.
- Redact token-bearing URLs from anything returned to the supervisor.

Only when HTTPS push is unavoidable, a temporary askpass helper that reads the
environment and is deleted after use:

```bash
ASKPASS=$(mktemp)
printf '%s\n' '#!/bin/sh' \
  'case "$1" in *Username*) printf "%s\n" "${GITHUB_USER:-x-access-token}" ;;' \
  '*) printf "%s\n" "$GH_TOKEN" ;; esac' > "$ASKPASS"
chmod 0700 "$ASKPASS"
trap 'rm -f "$ASKPASS"' EXIT
GIT_ASKPASS="$ASKPASS" GIT_TERMINAL_PROMPT=0 git push "$REMOTE" "$HEAD"
```

Token needs `repo`, or fine-grained `contents: write` + `pull_requests: write`,
on the target repository, from an account allowed to push the head branch. Fork
PRs push the head to the fork and target upstream. On auth failure, stop; do not
switch account, remote, or to a tokenized URL.

## Push

```bash
git push --set-upstream "$REMOTE" "$HEAD"
```

No `--force`, no `--force-with-lease`. A rejected push is an authoring decision
that requires re-verification.

## Create

```bash
gh pr create --repo "$OWNER/$REPO" --base "$BASE" --head "$HEAD" \
  --title "$(cat "$ART/pr-title.txt")" --body-file "$ART/pr-body.md"
```

- Fork head: `--head <owner>:<branch>`.
- `--draft` when the user asked or verification is incomplete.
- No `gh`: REST API, token in the header only.

## Validate

```bash
gh pr view "$PR" --json url,number,baseRefName,headRefName,headRefOid,mergeable,isDraft,title
gh pr diff "$PR" --patch | diff - <(git diff "github/${BASE}...HEAD")
gh pr view "$PR" --json body --jq .body | grep -F 'Assisted-by: LLM'
gh pr view "$PR" --json body --jq .body | grep -E '^(Fixes|Refs) #[0-9]+'
gh pr checks "$PR" || true
```

Require: title matches, base and head intended, `headRefOid` equals local `HEAD`,
PR diff equals local diff, footer and issue link present, no artifacts or secrets,
CI started.

Fixes: wrong base or head → `gh pr edit --base`, or close and recreate after
asking. Body → `gh pr edit --body-file`. Unintended files, commits, or failing CI
→ report and return to authoring; never add commits to patch the PR, never weaken
tests to make CI green.

Record the result:

```bash
printf '# %s\n\nPR: %s\nBase: %s\nHead: %s\n' \
  "$(cat "$ART/pr-title.txt")" "$PR_URL" "$BASE" "$HEAD" > "$ART/pr.md"
```

Do not merge unless asked.
