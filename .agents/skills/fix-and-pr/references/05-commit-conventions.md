# 05 — Commit conventions

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

---

## 4.4 Commit conventions

Follow the repository's existing style. Commit subjects use a subsystem prefix:

```text
<subsystem>: <short imperative description>
```

Examples from this repository:

```text
fix/futex: forget to .await in `futex_exit`
mm: introduce MapFolio for folios mapped to page tables
vfs, inode: remove {read,write}_direct
style, vfs: reformat fat32.rs, file.rs and inode.rs
```

Commit body requirements:

- What failed and under which conditions.
- Why it failed (root cause), not just what changed.
- How the fix restores the invariant.
- Reference the fixed commit or issue:
  - `Fixes: <commit-hash> ("<subject>")`
  - `Fixes #<issue-id>`
- Use `Signed-off-by` if the repository expects it (`git commit -s`).
- Add the `Assisted-by: LLM` trailer to every AI-generated commit. It is
  mandatory in this repository: all commits produced with agent assistance
  must disclose it, and it goes in the trailer block immediately before
  `Signed-off-by`. The trailer is added by the author, not by a script the CI
  enforces, so verify it in `git log -1 --format=%B` before pushing.
- Keep lines readable (typically 72 columns).

Example:

```text
fix/futex: wake waiters in futex_exit

Testcase `pthread_test` freezes while waiting for clear-tid futex wakeups.
`futex_exit()` forgets to await `do_futex_exit()`, so the exit path returns
before the wakeup is performed.

Await the exit operation so waiters are woken before the task is reaped.

Fixes: <hash> ("<subject>")
Fixes #<issue-id>
Assisted-by: LLM
Signed-off-by: <name> <email>
```

The trailer block order is fixed: `Fixes:`/`Link:` first, then `Assisted-by:`,
with `Signed-off-by:` last. Do not append `Assisted-by: LLM` after the
sign-off, and do not use ad-hoc spellings (`AI-assisted`, `Generated-by`,
`Co-authored-by: AI`); the tag is exactly `Assisted-by: LLM`.

Before committing:

```bash
git status --short
git diff --cached --name-only
```

Commit explicitly, never with `-A`:

```bash
git add path/to/file.rs path/to/other.rs
git commit -s           # then add `Assisted-by: LLM` to the message
```

Preferred non-interactive form that cannot silently drop the trailer:

```bash
git commit -F - <<'EOF'
<subsystem>: <short imperative description>

<body>

Fixes: <hash> ("<subject>")
Assisted-by: LLM
Signed-off-by: <name> <email>
EOF
```

Use `git commit -s -F <file>` only when the message file omits the
`Signed-off-by:` line, so the sign-off is not added twice.

After committing, verify the trailer is present and the commit is signed:

```bash
git log --format='%h %G? %s%n  %(trailers:key=Assisted-by,valueonly)' -1
```

If it is missing, fix it with `git commit --amend -F <message-file>` (or
`git rebase`) before pushing; a pushed commit without the tag must be corrected
with the same care as any other message fix, including a force-push and re-run
of the verification.

If the commit must be amended after review, re-run the build and reproduction
before force-pushing.

---

Next: [06 — Verification and independent review](06-verification.md)
