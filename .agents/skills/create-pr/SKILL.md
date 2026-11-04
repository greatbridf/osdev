---
name: create-pr
description: Publish a verified branch as a pull request against GBOS/Eonix, or any branch the user names, by delegating body drafting, body review, and publication to subagents. Use for PR preflight (clean tree, intended diff, AI-disclosure trailers), PR title/body, credential-safe push and `gh pr create`, and post-create validation. Handles PR publication only; it never changes code.
---

# Create a Pull Request

This skill takes a branch whose commits are already written and verified and
turns it into a validated pull request: preflight, credential-safe push, title
and body, `gh pr create`, then read-back validation. It does not debug,
reproduce, or fix anything. When the code is not ready, the work belongs to the
authoring workflow (see [fix-and-pr](../fix-and-pr/SKILL.md)).

Each phase runs in a subagent. The supervisor writes the brief, checks the
returned report against the gate, and decides base and head; it does not draft
the body, push, or call the API itself.

Read `references/README.md` first, then
`references/01-preflight.md` before touching a remote.

## Mandatory credential gate

Run this before **any** other action: preflight, remote inspection, push, or
subagent launch.

1. Check `GH_TOKEN` without printing it:

   ```bash
   test -n "${GH_TOKEN:-}"
   ```

2. If the check succeeds, continue.
3. If `GH_TOKEN` is unset or empty, stop immediately and ask the user to export
   it:

   ```text
   GH_TOKEN is not set. Please provide a GitHub token by exporting it in your
   shell, for example:
     export GH_TOKEN='...'
   Tell me when it is set; I will not continue until then.
   ```

4. Do not run tools, inspect the repository, push, or spawn subagents until the
   user confirms it is set and the check succeeds.
5. Never echo, log, commit, or put `GH_TOKEN` in a subagent prompt. Do not
   write it to a file, and never embed it in a remote URL.

## When this skill applies

- A verified branch must be pushed and turned into a PR.
- An existing PR needs its title, body, base, or disclosure footer corrected.
- The user asks to "open a PR", "publish this branch", or "submit the fix".
- Verification finished elsewhere, for example through the `fix-and-pr` skill.

## Entry point

```bash
ISSUE=<issue-id>            # or a slug when there is no issue
SLUG=<short-slug>
BASE=<base-branch>          # never default blindly to master
HEAD=$(git rev-parse --abbrev-ref HEAD)
ART="build/work/issue-${ISSUE}"
mkdir -p "$ART"
```

Then follow the phase gates in order.

## Phase gates

| Phase | Executed by | Required evidence | Reference document |
| --- | --- | --- | --- |
| 0. Preflight | supervisor | `GH_TOKEN` present; clean worktree; branch contains only intended commits; base/head confirmed by the user; every agent-written commit carries `Assisted-by: LLM`, human commits are left alone. | [01 — Preflight and evidence](references/01-preflight.md) |
| 1. Credentials | supervisor | Token confirmed usable; remote and push mechanism chosen (SSH preferred); no token in any URL, file, prompt, or log. | [02 — Credential handling](references/02-credentials.md) |
| 2. Body | `worker` drafts, fresh `reviewer` checks | `$ART/pr-title.txt` and `$ART/pr-body.md` exist with summary, root cause, fix, verification evidence, `Fixes #<id>`, and — since you drafted the body — the `Assisted-by: LLM` footer; the reviewer confirms every claim matches the diff. | [03 — PR title and body](references/03-pr-body.md) |
| 3. Publish | publisher `worker` | Branch pushed; PR created against the correct base/head; no code changes during this phase. | [04 — Publish and validate](references/04-publish-and-validate.md) |
| 4. Validation | publisher `worker`, checked by supervisor | PR URL, number, base, head, head SHA, mergeable state, body, and diff all read back and verified against the local diff. | [04 — Publish and validate](references/04-publish-and-validate.md) |

Do not skip Phase 0 because "the branch is obviously fine". A PR against the
wrong base, or an agent-written commit without the disclosure trailer, is a
published mistake.

## Critical rules

1. **Publication only.** Never edit source, amend, rebase, or force-push while
   running this skill. If the diff is wrong, stop and return to authoring.
2. **Delegate the work.** A `worker` drafts the title and body, a fresh
   `reviewer` checks them against the diff, and a publisher `worker` pushes and
   creates the PR. The supervisor briefs, gates, and decides base/head; it does
   not run `git push` or `gh pr create` itself. If a subagent cannot be launched,
   stop and report that publication cannot proceed.
3. **No verification, no PR.** Require evidence that the change was verified:
   build/test output, reproduction logs, or an explicit statement from the user.
   A PR without evidence gets a `## Verification` section that says so, and you
   ask the user before publishing anyway.
4. **Never leak `GH_TOKEN`.** No `echo`, no `set -x` around credential handling,
   no tokenized URLs, no token in prompts, bodies, commits, or logs.
5. **Disclose AI assistance, and only that.** A commit you wrote carries
   `Assisted-by: LLM` immediately before `Signed-off-by`; a commit the
   maintainer wrote alone is left exactly as it is, with no tag added. Never
   publish an agent-written commit that lacks the tag. The PR body footer
   records that you drafted it.
6. **Ask about the base.** If the target base is ambiguous (feature branch vs
   `master`), stop and ask.
7. **No secrets or artifacts in the diff.** No `build/`, logs, `.env`, keys,
   or machine-local absolute paths.
8. **Never merge.** Do not merge, enable auto-merge, or close the PR unless the
   user explicitly asks.
9. **Stop on ambiguity.** Unclear repository, fork vs upstream, or head branch
   owner means ask, not guess.

## Subagent use

| Role | Phases | Use |
| --- | --- | --- |
| `worker` | 2 | Drafts `pr-title.txt` and `pr-body.md` from the preflight evidence. |
| `reviewer` | 2 | Fresh-context check that every body claim matches the actual diff. |
| Publisher `worker` | 3, 4 | Pushes the branch, creates the PR, reads it back. Read-only on code; may run `git push` and GitHub API/CLI. |

Every brief ends with the report path its subagent must write under
`build/work/issue-<id>/reports/`, and that report is the gate evidence. The
supervisor keeps base/head decisions, body approval, and final gate checks.

## Artifacts

```text
build/work/issue-<id>/pr-title.txt    # PR title
build/work/issue-<id>/pr-body.md      # PR body markdown, with the LLM footer
build/work/issue-<id>/pr.md           # record: title, URL, base, head
build/work/issue-<id>/reports/        # one report per subagent run
```

`build/` is ignored by Git but removed by `make clean`; copy these elsewhere if
they must survive a clean rebuild.

## Output expectations

When reporting completion, include:

- PR URL and number.
- Repository, base branch, head branch, and head SHA.
- Title and a one-line summary of the body.
- Verification evidence referenced in the body (or that none was available).
- CI status observed at create time.
- Confirmation that the PR diff matches the local diff.
- Remaining risks or blockers.

READ ALL REFERENCE DOCUMENTATIONS UNDER THIS SKILL'S references/ DIRECTORY BEFORE CONTINUING!
