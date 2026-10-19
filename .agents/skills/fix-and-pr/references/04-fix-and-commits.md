# 04 — Fix planning and implementation

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

---

## 4. Phase 3: Implement the fix

### 4.1 Plan the fix

Write `build/work/issue-<id>/plan.md`:

```markdown
# Fix plan: issue <id>

## Root cause
...

## Change boundary
Files/functions to change:
- ...

Explicitly out of scope:
- ...

## Invariants
- ...

## Failure paths to preserve
- ...

## Commit split
1. <prep/refactor, if needed>
2. <the actual fix>
3. <regression test or reproducer, if appropriate>

## Verification matrix
| Case | Arch | MODE | SMP | Command | Expected |
| --- | --- | --- | --- | --- | --- |
| Baseline repro | x86_64 | release | 4 | `--expect bug` | 0 |
| Fixed repro | x86_64 | release | 4 | `--expect fixed` | 0 |
| Build | x86_64 | release | 4 | `make build` | 0 |
| Existing smoke | x86_64 | release | 4 | `script/test.sh` | 0 |

## Risks / open questions
- ...
```

For a complex or cross-subsystem fix, have a fresh `reviewer` critique the plan
before any edit:

```text
Review this fix plan against the established root cause and repository
conventions. Do not edit files. Identify missing lock/lifetime/refcount/error
paths, missing architecture variants, and any step that is broader than the
root cause requires. Return blocking issues first, then non-blocking suggestions.
```

Approve the plan only when the change boundary and verification matrix are
explicit.

### 4.2 Write the fix with one worker

Use one writer per worktree. The task must state the exact files or subsystem
the worker may touch.

```text
Objective
Implement the approved fix for issue #<id> according to plan.md.

Repository and ref
- cwd: <worktree path>
- branch: fix/<id>-<slug>
- base commit: <sha>

Authority
- You may edit only: <files or subsystem>.
- Do not edit unrelated modules, do not reformat files, do not update
  dependencies, and do not add debug prints that survive the commit.
- Do not commit unless the supervisor asks you to.
- Stop and report if the plan is wrong or if a second, independent bug is found.

Required work
1. Read the relevant code and the root-cause analysis before editing.
2. Apply the minimal change that restores the violated invariant.
3. Handle error paths and partial-failure cases.
4. Preserve SMP, interrupt, and preemption safety where applicable.
5. Run `cargo fmt --all -- --check` if the repository expects rustfmt.
6. Build: `make build MODE=release` and `make build MODE=debug` for the target
   architecture; add a second architecture when the change is shared.
7. Run the reproduction:
   - `--expect bug` must still exit 0 on the baseline commit.
   - `--expect fixed` must exit 0 on the fixed tree.
8. Report changed files, commands, exit codes, and log paths.

Acceptance
- The reproduction passes in fixed mode.
- The baseline still reproduces.
- No unrelated diff.
- No secrets, logs, or build artifacts staged.
```

Implementation guardrails:

- Make the smallest change that fixes the root cause. Do not combine a bug fix
  with a broad cleanup.
- If a refactor is required to make the fix safe, keep it in a separate commit.
- Remove temporary instrumentation or put it behind an existing feature flag.
- If the build or reproduction fails, collect evidence and return to the
  supervisor. Do not keep editing blindly.
- If a second independent bug appears, record it and ask whether to expand the
  scope; do not silently fix it.

### 4.3 Refine and split the fix

After the first working version, review the diff as if it were hostile:

```bash
git diff --stat
git diff
git diff --check
```

Check:

- Is every changed line necessary for the root cause?
- Are lock order, reference ownership, and error paths still correct?
- Is the fix race-free under the reported `SMP` configuration?
- Are architecture-specific assumptions handled?
- Is user input validated on all paths?
- Are there any behavior changes that the issue does not require?
- Are debug prints, temporary hacks, and unrelated formatting removed?

If the fix is clearer as several commits, split it while preserving
bisectability. A typical sequence is:

1. `refactor:` or `prep:` — a behavior-preserving preparation, if required.
2. `fix:` — the actual bug fix.
3. `test:` — an optional regression test or reproducer.

Every commit must build on its own. Use an interactive rebase only on a private
branch:

```bash
git rebase -i "github/${BASE}"
git diff "github/${BASE}"...HEAD
```

Do not rewrite a shared branch that others may have based work on.

---

Next: [05 — Commit conventions](05-commit-conventions.md)
