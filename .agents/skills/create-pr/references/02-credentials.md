# 02 — Credential handling

Part of the `create-pr` skill. Read [01 — Preflight](01-preflight.md) first.

---

## 2. Credentials and remotes

### 2.1 Re-check immediately before publishing

```bash
test -n "${GH_TOKEN:-}" || {
  echo "GH_TOKEN is not set; ask the user before continuing." >&2
  return 1
}
```

`GH_TOKEN` is the environment variable `gh` reads directly, so exporting it in
the shell is enough; never pass it as an argument.

### 2.2 Choose the push mechanism

Preferred order:

1. **SSH remote** for `git push`, token only for GitHub API/CLI calls.
2. **`gh`** for both push and PR creation when the remote is HTTPS and `gh` is
   authenticated.
3. **REST API** with `GH_TOKEN` in the `Authorization` header when `gh` is
   unavailable.

Inspect remotes without exposing userinfo:

```bash
git remote -v | sed -E 's#(https?://)[^@/]*@#\1***@#'
```

### 2.3 Rules

- Never `echo "$GH_TOKEN"`, `printenv GH_TOKEN`, or enable `set -x` around
  credential handling.
- Never embed the token in a remote URL such as
  `https://<token>@github.com/...`. It persists in `.git/config` and shell
  history.
- Never put the token in a subagent prompt, PR body, commit message, issue
  comment, or log file.
- Never write the token to disk, including temporary files, except inside an
  askpass helper that reads it from the environment and is deleted afterward.
- Redact token-bearing URLs from any command output returned to the supervisor.

### 2.4 If HTTPS push is unavoidable

Use a temporary askpass helper that reads the token from the environment:

```bash
ASKPASS=$(mktemp)
cat >"$ASKPASS" <<'EOF'
#!/bin/sh
case "$1" in
  *Username*) printf '%s\n' "${GITHUB_USER:-x-access-token}" ;;
  *)          printf '%s\n' "$GH_TOKEN" ;;
esac
EOF
chmod 0700 "$ASKPASS"
trap 'shred -u "$ASKPASS" 2>/dev/null || rm -f "$ASKPASS"' EXIT
GIT_ASKPASS="$ASKPASS" GIT_TERMINAL_PROMPT=0 git push <remote> "$HEAD"
```

The helper prints the token only to `git`, never to the terminal or a log.

### 2.5 Scope and identity

- The token needs `repo` (or fine-grained `contents: write` +
  `pull_requests: write`) on the target repository.
- The token must belong to an account allowed to push the head branch. For
  fork-based PRs, the head branch lives on the fork; the base is on upstream.
- If authentication fails, stop and report the failure. Do not try a different
  account, a different remote, or a tokenized URL.

---

Next: [03 — PR title and body](03-pr-body.md)
