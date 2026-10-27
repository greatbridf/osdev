# 03 — Root-cause analysis and debugging

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

---

## 3. Phase 2: Root-cause analysis and debugging

The goal is a **causal explanation**, not a patch that happens to make the test
green.

### 3.1 Freeze the evidence

Before editing anything:

```bash
git rev-parse HEAD > "$ART/baseline-commit.txt"
git status --short > "$ART/baseline-status.txt"
cp "$ART/before.log" "$ART/before.keep.log"
```

Preserve:

- The baseline commit and branch.
- The exact reproduction command and configuration.
- The complete panic/backtrace text.
- The QEMU log, if trap logging was enabled.
- Any kernel symbols or disassembly used.

For trap logging:

```bash
DEBUG_TRAPS=1 make test-run MODE=release ARCH=x86_64
# QEMU writes detailed logs to build/qemu.log
```

### 3.2 Static reconnaissance

Map the code path before changing it:

```bash
rg -n "<function-or-symbol>" src crates macros
rg -n "<error string or assertion>" src crates
git log --oneline --all -- <path>
git blame -L <start>,<end> -- <path>
```

Inspect:

- Callers and callees of the failing function.
- Lock ordering, reference counting, and lifetime assumptions.
- Error paths: allocation failure, invalid user pointers, partial initialization.
- Architecture-specific implementations under `src/` and `crates/eonix_hal`.
- Related filesystem, scheduler, VFS, MM, or syscall code.
- Existing tests and documentation under `doc/` and `script/`.

For GBOS/Eonix, explicitly consider:

- **SMP:** Is the code correct with concurrent access? Does it assume a single
  CPU? Is the lock held across an await point or a blocking operation?
- **Interrupts/preemption:** Can the state change between check and use?
- **Refcounts:** Who owns each reference, and which path drops it?
- **Page tables/folios:** Are mappings, CoW, and page flags updated atomically?
- **User memory:** Is the access validated, and can it fault mid-copy?
- **Architecture:** Does the fix need a RISC-V or LoongArch counterpart?
- **ABI:** Does the change alter syscall return values, errno, or struct layout?

### 3.3 Dynamic analysis

Use the smallest tool that answers the next question.

- **QEMU trap log:** `DEBUG_TRAPS=1` and inspect `build/qemu.log`.
- **GDB:** `make srun` then `make debug`, or `make tmux-debug`.
- **Backtrace:** `bt full`, `thread apply all bt`, `info registers`.
- **Memory/state:** `x/`, `p`, `p/x`, `info threads`, `monitor info registers`.
- **Tracing:** enable `trace_syscall`, `trace_scheduler`, `trace_pci`, or
  `log_debug` only as needed. Remove or feature-gate temporary prints before
  committing.
- **SMP comparison:** run `SMP=1` and `SMP=4`; a difference is strong evidence
  of a race or missing synchronization.
- **Architecture comparison:** if x86_64 fails, check whether the shared code
  or the HAL is responsible; test a second architecture when feasible.

Do not leave GDB or QEMU running between phases. Record the exact commands and
observations in `analysis.md`.

### 3.4 Form and disprove hypotheses

Write down candidates before testing:

```text
Hypothesis:
Expected behavior:
Actual behavior:
Evidence for:
Evidence against:
Falsification test:
```

Rules:

- Prefer the hypothesis that explains the **first** wrong state, not the last
  visible symptom.
- Distinguish the bug from the mechanism that exposes it.
- If a hypothesis predicts a log line, register value, or code path, verify it
  directly.
- If two independent hypotheses remain, run one read-only `scout` per
  hypothesis; do not run duplicate scouts with the same prompt.
- Keep failed hypotheses in `analysis.md`; they prevent the next agent from
  repeating the work.

### 3.5 Write the root-cause statement

Before planning the fix, complete this sentence with evidence:

```text
Root cause: <specific mechanism> causes <observed failure> because <invariant
is violated / state is inconsistent / operation is missing>, as shown by
<log, backtrace, code path, or experiment>.
```

Also record:

- The smallest code boundary that must change.
- What must **not** change.
- Which invariants the fix must preserve.
- Which tests or configurations could regress.

If the root cause is not established, do not proceed to Phase 3.

### 3.6 Optional independent analysis

For a high-risk or surprising diagnosis, spawn a fresh `reviewer` with the
evidence and ask it to challenge the conclusion:

```text
Challenge this root-cause claim using the supplied issue, logs, and code paths.
Do not edit files. Identify:
1. evidence that contradicts the claim;
2. a simpler competing explanation;
3. a falsification test the supervisor can run;
4. any lifecycle/locking/refcount assumption that is not proven.
Return a concise verdict: supported, uncertain, or contradicted.
```

---

Next: [04 — Fix planning and implementation](04-fix-and-commits.md)
