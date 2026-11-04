# 05 — Commits

## Message

Subject: `<subsystem>: <imperative>`, ≤72 characters, matching repo history
(`fix/futex: ...`, `mm: ...`, `vfs, inode: ...`).

Body: what failed and when, the root cause, how the fix restores the invariant,
wrapped at 72 columns. Reference context with `Fixes: <hash> ("<subject>")` or
`Fixes #<id>` when applicable.

Trailers, in this order: `Fixes:`/`Link:`, then `Assisted-by: LLM` **only if an
agent wrote the commit**, then `Signed-off-by:` last. A commit the maintainer
wrote alone gets no `Assisted-by:` line; never add one retroactively. The tag
exactly as spelled — no `AI-assisted`, `Generated-by`, or `Co-authored-by: AI`.

```text
fix/futex: wake waiters in futex_exit

<pthread_test> freezes waiting for clear-tid wakeups because `futex_exit()`
never awaits `do_futex_exit()`, so the exit path returns before the wakeup.

Await the exit operation so waiters are woken before the task is reaped.

Fixes: <hash> ("<subject>")
Assisted-by: LLM
Signed-off-by: <name> <email>
```

## Staging

```bash
git status --short; git diff --cached --name-only
git add path/to/file.rs path/to/other.rs      # never -A or .
```

Commit non-interactively so a trailer cannot be dropped:

```bash
git commit -F - <<'EOF'
<message>
EOF
```

Use `git commit -s` only when the message omits `Signed-off-by:`, to avoid a
duplicate.

## Verify

```bash
git log --format='%h %G? %s%n  %(trailers:key=Assisted-by,valueonly)' -1
git interpret-trailers --parse <(git log -1 --format=%B)   # confirms the block
```

An agent-written commit missing `Assisted-by: LLM` is amended before pushing;
that is a history rewrite, so re-run the verification afterwards.

---

Next: [06 — Verification](06-verification.md)
