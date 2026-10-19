# 11 — Prompt hygiene

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

---

## Appendix C: Prompt hygiene

Every subagent prompt should contain:

1. **Objective** — one concrete deliverable.
2. **Repository, cwd, and ref** — exact paths and commit/branch.
3. **Authority boundary** — what the subagent may read, edit, run, or publish.
4. **Relevant evidence** — issue, logs, root cause, plan, or diff paths.
5. **Acceptance criteria** — commands and expected exit codes.
6. **Output contract** — changed files, commands run, exit codes, log paths,
   verdict, and open questions.
7. **Stop conditions** — ambiguity, missing credentials, infrastructure
   failure, or a second independent bug.

The supervisor checks `GH_TOKEN`; never ask a subagent to discover, retrieve,
or validate the token, and never include the token in a prompt. Prompts must not
contain credentials or unredacted credential-bearing URLs.