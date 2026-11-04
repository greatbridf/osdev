# 03 — PR title and body

Part of the `create-pr` skill. Read [01 — Preflight](01-preflight.md) for the
evidence you must cite.

---

## 3. Write the title and body

### 3.1 Delegation

Drafting is a `worker` task and checking it is a fresh `reviewer` task; the
supervisor approves the result, it does not write or vouch for it. Write the
brief with the structure in
[prompt hygiene](../../fix-and-pr/references/10-prompt-hygiene.md) and have the
worker produce both files plus `reports/03-body-worker.md`:

```text
Objective
Write the PR title and body for issue #<id> from the verified evidence.
Do not modify code, commits, or the branch.

Inputs
- cwd: <worktree>, head: <branch> @ <sha>, base: <base>
- local diff: `git diff github/<base>...HEAD`
- verification evidence: <report and log paths from the fix-and-pr run>

Authority
- Write only build/work/issue-<id>/pr-title.txt, pr-body.md, and your report.
- Every bullet under `## Verification` must come from a log or report you were
  given. Never invent a command, exit code, or pass rate.

Output
- pr-title.txt: one line, `<subsystem>: <imperative description>`.
- pr-body.md: Summary, Root cause, Fix, Verification, Risks, then
  `Fixes #<id>` and the `Assisted-by: LLM` footer.
- reports/03-body-worker.md: claims you could not substantiate, and why.
```

Then have a `reviewer` that has read the diff check the claims against it, and
record the verdict in `reports/03-body-reviewer.md`:

```text
Check pr-body.md against `git diff github/<base>...HEAD`. Do not edit files.
Confirm every Verification bullet and every root-cause claim is supported by the
diff or the named log, that no behavior is described that the code does not
change, and that no local path or credential leaked in. Return blocking issues
first, or state explicitly that there are none.
```

Body drafting is reported through those two files, so the phase-2 gate stays
checkable after the session ends.

### 3.2 Title

Use the repository's subsystem style, matching the commit subject:

```text
<subsystem>: <short imperative description>
```

Examples:

```text
fix/futex: await do_futex_exit in futex_exit
mm: introduce MapFolio for folios mapped to page tables
```

Keep it under ~72 characters, imperative mood, no trailing period, no
`[WIP]`, no issue number in the title.

### 3.3 Body

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
<Remaining uncertainty, behavior change, or follow-up work.>

Fixes #<issue-id>

Assisted-by: LLM
```

Rules for content:

- Every bullet in `## Verification` must be something that actually ran. Delete
  rows you cannot substantiate; do not keep placeholders.
- If verification was partial, say what is missing instead of omitting the
  section.
- Summarize logs, do not paste them. No machine-local absolute paths, no
  `build/` contents, no CI URLs that require credentials.
- Use `Fixes #<id>` only when the PR should close the issue. Use `Refs #<id>`
  otherwise.
- `Fixes:`/`Link:` trailers belong in commits; the body uses the `Fixes #<id>`
  form.
- The `Assisted-by: LLM` footer is the last line whenever the body was drafted
  with agent help, which is the normal case here. It records who wrote the
  description, not who wrote the commits: a human-authored commit on the same
  branch still gets no trailer, and the footer stays because the prose is
  agent-written. Keep it even if the commits are later squashed or re-applied.

The supervisor sets `ART="build/work/issue-${ISSUE}"` before running the checks
below; they validate the subagent output rather than replacing it.

### 3.4 Title hygiene

```bash
wc -c < "$ART/pr-title.txt"     # stay near ~72 characters
```

The title is a single line with no trailing newline requirement, but strip
`[WIP]`, issue numbers, and trailing periods before publishing.

### 3.5 Body self-check

```bash
grep -c '^## ' "$ART/pr-body.md"                 # expect 4-5 sections
grep -qF 'Assisted-by: LLM' "$ART/pr-body.md"    # must succeed
grep -qE '^(Fixes|Refs) #[0-9]+' "$ART/pr-body.md" || echo "no issue link"
grep -nE '/Users/|/home/[a-z]' "$ART/pr-body.md" || echo "no local paths"
```

Then do the same for the commit trailers, remembering that only agent-written
commits carry the tag:

```bash
git log --format='%h %(trailers:key=Assisted-by,valueonly)' "github/${BASE}..HEAD"
```

---

Next: [04 — Publish and validate](04-publish-and-validate.md)
