# 06 — Verification and independent review

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

---

## 5. Phase 4: Verification and independent review

Verification is a separate phase. The author's successful run is not enough.

### 5.1 Verification matrix

Run all applicable rows from `plan.md`. At minimum:

```bash
# Rebuild from the fixed tree.
make build MODE=release ARCH=x86_64
make build MODE=debug   ARCH=x86_64

# The exact same reproducer, now expecting the fix.
script/repro/issue-${ISSUE}-${SLUG}.exp \
  --expect fixed --arch x86_64 --mode release --smp 4 \
  --log "$ART/after.log"

# Existing repository smoke test, if it is applicable and available.
ARCH=x86_64 sh script/test.sh

# Diff hygiene.
git diff --check
git diff "github/${BASE}"...HEAD
```

For shared code, also build and, where feasible, run a second architecture:

```bash
make build MODE=release ARCH=riscv64
make build MODE=release ARCH=loongarch64
```

For SMP/race issues:

- Run the reproduction at least 5–10 times in the fixed configuration.
- Run with `SMP=1` and `SMP=4`.
- Record failures and flakiness, not just the final green run.

Acceptance for this phase:

- Baseline `--expect bug` still exits 0 on the base commit.
- Fixed `--expect fixed` exits 0 on the fixed commit.
- Builds pass for the required architectures and modes.
- Existing tests pass, or any failure is proven unrelated and documented.
- No orphan QEMU processes remain.
- Logs are saved and contain no secrets.

### 5.2 Independent fix review

Spawn a fresh `reviewer` that did not write the fix:

```text
Review the diff for issue #<id> against base <sha>. Do not edit files.

Inputs
- diff: `git diff github/<base>...HEAD`
- root cause: build/work/issue-<id>/analysis.md
- plan: build/work/issue-<id>/plan.md
- before/after logs: build/work/issue-<id>/before.log, after.log

Check, in order:
1. Correctness: does the change actually fix the stated root cause?
2. Lifecycle and ownership: refcounts, locks, lock ordering, awaits,
   interrupts, and preemption.
3. Error paths: allocation failure, invalid user pointers, partial setup,
   teardown, and double-free/use-after-free.
4. Concurrency: SMP races, lost wakeups, check-then-act windows.
5. Architecture: shared-code assumptions and missing HAL/arch variants.
6. Scope: unrelated changes, debug prints, formatting noise, new dependencies.
7. Verification: does the evidence actually prove the fix, or only a proxy?

Report blocking findings first, with file:line and a concrete failure scenario.
Then report non-blocking suggestions. If there are no blocking findings, say so
explicitly. Do not approve merely because the reproducer passes.
```

If the reviewer finds a blocking issue, return to Phase 3 to correct it, then
re-run the full Phase 4 verification matrix. Do not open a PR with an unresolved
review finding.

### 5.3 Definition of done

- [ ] Issue details and expected behavior are recorded.
- [ ] Reproduction script exists and is independently verified.
- [ ] Baseline `--expect bug` exits 0.
- [ ] Fixed `--expect fixed` exits 0.
- [ ] Root cause is stated with evidence.
- [ ] Fix is minimal and scoped to the root cause.
- [ ] Required builds and tests pass.
- [ ] SMP/race behavior is tested when relevant.
- [ ] Independent review has no blocking findings.
- [ ] Commits are clean, build independently, and follow conventions.
- [ ] No secrets, logs, or build artifacts are staged.
- [ ] PR base and head branches are correct.

---

Next: [07 — PR submission](07-pr-submission.md)
