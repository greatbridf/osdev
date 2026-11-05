# create-pr references

Read [01 — Preflight](01-preflight.md) first.

| Step | Document | Read it when |
| --- | --- | --- |
| 0 Preflight | [01](01-preflight.md) | Checking tree, range, trailers, evidence, secrets |
| 1 Body | [02](02-body.md) | Drafting and reviewing title and body |
| 2–3 Publish | [03](03-publish.md) | Credentials, push, `gh pr create`, read-back |

Each delegated step's report under `$ART/reports/` is its gate evidence; a step
run inline was not done. Preflight is the exception: the supervisor runs it
directly and keeps no report.

The supervisor keeps the base/head decision, body approval, and final gate
checks; the publisher `worker` owns the push and the API call.
