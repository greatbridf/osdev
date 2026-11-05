# 06 — Verification

Phase 4. A verifier subagent runs the matrix; a fresh `reviewer` reviews the
diff. The author's own run is not the gate.

## Matrix

Run every applicable row of `plan.md`, report to
`$ART/reports/04-verify-verifier.md`:

```bash
make build MODE=release ARCH=x86_64
make build MODE=debug   ARCH=x86_64
script/repro/issue-${ISSUE}-${SLUG}.exp --expect fixed \
  --arch x86_64 --mode release --smp 4 --log "$ART/after.log"
ARCH=x86_64 sh script/test.sh
git diff --check
```

Shared code: also build `ARCH=riscv64` and `ARCH=loongarch64`. SMP or race
issues: 5–10 runs at `SMP=4`, plus `SMP=1`; record flakiness, not just the green
run.

Pass requires: baseline `--expect bug` exits 0 on the base commit, `--expect
fixed` exits 0 on the fix, builds pass, existing tests pass or their failure is
proven unrelated, no orphan QEMU processes, logs kept and secret-free.

## Diff review

Required, by a `reviewer` that did not write the fix; verdict to
`$ART/reports/04-diff-reviewer.md`. A passing reproduction does not substitute
for it.

Inputs: `git diff github/<base>...HEAD`, `analysis.md`, `plan.md`, `before.log`,
`after.log`. Check in order: does it fix the stated cause; refcounts, locks,
lock order, awaits, preemption; error and teardown paths; SMP races and lost
wakeups; architecture variants; scope creep and debug leftovers; whether the
evidence proves the fix or only a proxy. Blocking findings first with file:line
and a failure scenario, then suggestions, or an explicit "no blocking findings".

Blocker → back to phase 3, then re-run the whole matrix.

## Done

- [ ] Issue and expected behavior recorded
- [ ] Reproducer independently verified
- [ ] Baseline bug run and fixed run both pass
- [ ] Root cause stated with evidence
- [ ] Minimal, scoped diff
- [ ] Required builds and tests pass
- [ ] SMP behavior tested where relevant
- [ ] Review has no blocking findings
- [ ] Commits clean, build alone, correct trailers
- [ ] No secrets, logs, or artifacts staged
- [ ] Base branch agreed for handoff

---

Next: [create-pr](../../create-pr/SKILL.md) publishes the branch.
