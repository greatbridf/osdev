# 07 — PR submission

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

---

## 6. Phase 5: PR submission

Use a `worker` as publisher. It must not change code during this phase.

### 6.1 Pre-flight

```bash
git status --short --branch
git log --oneline "github/${BASE}..HEAD"
git diff --stat "github/${BASE}...HEAD"
git remote -v
```

Confirm:

- The working tree is clean.
- The branch contains only intended commits.
- The base branch is correct. If the issue targets a feature branch, use that
  branch as the PR base; do not assume `master`.
- The head branch has been pushed to the repository expected by the PR host.
- No credential material, `GH_TOKEN` value, or workflow logs are in the diff.

If the base branch is ambiguous, ask the maintainer or supervisor. A PR against
the wrong base can create a misleading diff and break CI.

### 6.2 Credential handling

The mandatory gate in [00 — Operating contract](00-operating-contract.md) has
already confirmed that `GH_TOKEN` is present. Re-check it immediately before
publishing:

```bash
test -n "${GH_TOKEN:-}" || {
  echo "GH_TOKEN is not set; ask the user before continuing." >&2
  return 1
}
```

Preferred: use the existing SSH remote for the push and the token only for
GitHub API calls. `GH_TOKEN` is the standard environment variable read by `gh`.
Export it before publishing:

```bash
gh pr create ...
```

Rules:

- Never `echo "$GH_TOKEN"`, enable `set -x` around credential handling, or
  write the value to a file.
- Never embed the token in a remote URL such as
  `https://<token>@github.com/...`.
- Never put the token in a subagent prompt, PR body, commit message, or log.
- If an HTTPS push is unavoidable, use a temporary `GIT_ASKPASS` helper that
  reads `GH_TOKEN` from the environment, set it to mode `0700`, and remove it
  afterward. Prefer `gh`/SSH when available.
- Redact token-bearing URLs from any command output returned to the supervisor.

### 6.3 Publisher subagent task

```text
Objective
Publish the verified fix for issue #<id> as a pull request. Do not modify code.

Parameters
- Local cwd: <worktree>
- Target repository: greatbridf/osdev
- Base branch: <base>
- Head branch: <head>
- Remote to push to: <github|origin|fork>
- PR title: <subsystem>: <short description>
- PR body file: build/work/issue-<id>/pr.md

Authority
- You may run git push and the GitHub API/CLI.
- Do not edit source files, amend commits, rebase, force-push, or merge.
- Do not print or persist `GH_TOKEN`.
- If authentication fails or the base/head is ambiguous, stop and report.

Steps
1. Verify the local branch, HEAD sha, clean worktree, and intended diff.
2. Confirm `GH_TOKEN` is present in the environment. Do not read it from a
   file. Ensure it is exported; `gh` reads it directly.
3. Push the head branch to its remote. Use SSH if configured; otherwise use a
   temporary askpass mechanism that reads `GH_TOKEN`, never a tokenized URL.
4. Create a PR against the target repository with the given base and head.
   Include the verification evidence and `Fixes #<id>` in the body.
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

Recommended `gh` command:

```bash
gh pr create \
  --repo greatbridf/osdev \
  --base "$BASE" \
  --head "$HEAD" \
  --title "$TITLE" \
  --body-file "$ART/pr.md"
```

If the head branch lives on a fork, use the fully qualified head form required
by the host, for example `--head <owner>:<branch>`.

If `gh` is unavailable, use the GitHub REST API with `GH_TOKEN`. Never place the
token in the JSON body or URL.

### 6.4 PR content

Use a concise, evidence-based body:

```markdown
## Summary
<One paragraph describing the user-visible problem and the fix.>

## Root cause
<Specific mechanism and why it caused the observed failure.>

## Fix
<What changed and why it restores the invariant.>

## Verification
- Baseline reproduction: `script/repro/issue-<id>-<slug>.exp --expect bug` — exit 0, `before.log`
- Fixed reproduction: `... --expect fixed` — exit 0, `after.log`
- Build: `make build MODE=release ARCH=x86_64` — pass
- Build: `make build MODE=release ARCH=riscv64` — pass (if applicable)
- Existing smoke test: `ARCH=x86_64 sh script/test.sh` — pass (if applicable)
- SMP stress: 10/10 runs passed at `SMP=4` (if applicable)

## Risks / notes
<Any remaining uncertainty, follow-up work, or behavior change.>

Fixes #<issue-id>
```

Do not paste full logs into the PR. Link or summarize them, and attach only what
is necessary. Do not include machine-local absolute paths.

### 6.5 Post-create validation

- Open the PR URL and confirm the title, base, head, and body.
- Confirm `Fixes #<id>` is present so the issue closes when merged.
- Confirm CI starts and record the check names.
- Confirm the PR diff matches `git diff "github/${BASE}"...HEAD`.
- Confirm no credential value, `build/`, or workflow logs appear in the diff.
- If CI fails, return to Phase 4 with the failing log; do not "fix" CI by
  weakening the reproduction.

Report to the user:

```text
PR: <url>
Head: <branch> @ <sha>
Base: <base>
Reproduction: before=pass (bug), after=pass (fixed)
Builds: <architectures/modes>
Review: <no blocking findings / summary>
CI: <pending/passing/failing>
```

Do not merge the PR unless the user explicitly asks.

---

Next: [08 — Failure handling and escalation](08-failure-handling.md)
