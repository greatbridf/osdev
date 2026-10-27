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
Signed-off-by: <name> <email>
```

Before committing:

```bash
git status --short
git diff --cached --name-only
```

Commit explicitly, never with `-A`:

```bash
git add path/to/file.rs path/to/other.rs
git commit -s
```

If the commit must be amended after review, re-run the build and reproduction
before force-pushing.

---

Next: [06 — Verification and independent review](06-verification.md)
