# create-pr reference library

Detailed reference for PR publication of GBOS/Eonix branches. The workflow is
split so an agent can load only the step it is on.

Read [01 — Preflight](01-preflight.md) first, then the document for the step you
are entering.

## Step documents

| Step | Document | Read it when |
| --- | --- | --- |
| 0. Preflight | [01 — Preflight and evidence](01-preflight.md) | Checking the worktree, commits, base/head, trailers, and verification evidence. |
| 1. Credentials | [02 — Credential handling](02-credentials.md) | Choosing the remote and push mechanism without leaking the token. |
| 2. Body | [03 — PR title and body](03-pr-body.md) | Writing `pr.md`: title, summary, root cause, fix, verification, footer. |
| 3–4. Publish and validate | [04 — Publish and validate](04-publish-and-validate.md) | Pushing, creating the PR, and validating it after creation. |

## Supporting documents

| Document | Purpose |
| --- | --- |
| [05 — Failure handling](05-failure-handling.md) | Stop conditions and recovery choices when a push or API call fails. |

## Reading rules

1. Confirm `GH_TOKEN` is set before doing anything else. If it is missing, stop
   and ask the user; do not read further.
2. Load only the current step's document plus any supporting document it points
   to.
3. Before publishing, re-check every item in the preflight checklist. Do not
   rely on a previous session's state.
4. This skill publishes. If preflight shows the change is unverified or
   incomplete, stop and return to authoring instead of publishing anyway.
5. Draft the body, review it, and publish in subagents; check the report each one
   writes rather than doing the step inline.
