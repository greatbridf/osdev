# 02 — Title and body

A `worker` drafts; a fresh `reviewer` checks. The supervisor approves, it does
not write or vouch.

## Worker brief

- Objective: title and body for issue #<id> from verified evidence. No code,
  commit, or branch changes.
- Inputs: cwd, head and base refs, `git diff github/<base>...HEAD`, the
  verification logs and reports.
- Authority: write only `$ART/pr-title.txt`, `$ART/pr-body.md`, and
  `$ART/reports/01-body-worker.md`.
- Every `## Verification` bullet must come from a log or report it was given.
  Never invent a command, exit code, or pass rate.
- Report anything it could not substantiate.

## Content

Title: `<subsystem>: <imperative>`, ≤72 characters, no `[WIP]`, no issue number,
no trailing period.

```markdown
## Summary
<User-visible problem and the fix.>

## Root cause
<Mechanism and why it produced the failure.>

## Fix
<What changed and which invariant it restores.>

## Verification
- Baseline: `<script> --expect bug` — exit 0, before.log
- Fixed: `<script> --expect fixed` — exit 0, after.log
- Build: `make build MODE=release ARCH=x86_64` — pass
- Smoke: `ARCH=x86_64 sh script/test.sh` — pass

## Risks / notes
<Remaining uncertainty or follow-up.>

Fixes #<id>

Assisted-by: LLM
```

- Delete any bullet that did not run; state what is missing rather than dropping
  the section.
- Summarize logs, never paste them. No local paths or `build/` contents.
- `Fixes #<id>` closes the issue; `Refs #<id>` otherwise.
- The footer records who wrote the description, so keep it even though commits
  may be human-written.

## Reviewer brief

Read-only. Compare `pr-body.md` against the diff:

- Every verification bullet and root-cause claim is supported by the diff or the
  named log.
- No behavior described that the code does not change.
- No local path, credential, or pasted log.
- Blocking issues first, or an explicit "no blocking findings".

Verdict to `$ART/reports/01-body-reviewer.md`.

## Checks

```bash
wc -c < "$ART/pr-title.txt"                      # near ≤72
grep -c '^## ' "$ART/pr-body.md"                 # 4-5 sections
grep -qF 'Assisted-by: LLM' "$ART/pr-body.md"
grep -qE '^(Fixes|Refs) #[0-9]+' "$ART/pr-body.md"
grep -nE '/Users/|/home/[a-z]' "$ART/pr-body.md" || echo "no local paths"
```

---

Next: [03 — Publish](03-publish.md)
