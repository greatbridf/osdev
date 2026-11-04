# 03 — Analysis

Phase 2, run by `scout`.

## Freeze the evidence

```bash
git rev-parse HEAD > "$ART/baseline-commit.txt"
git status --short > "$ART/baseline-status.txt"
cp "$ART/before.log" "$ART/before.keep.log"
DEBUG_TRAPS=1 make test-run MODE=release ARCH=x86_64   # build/qemu.log
```

Keep the baseline commit, the exact reproduction command, the panic or backtrace
text, and any symbols or disassembly used.

## Where to look

```bash
rg -n "<symbol or error string>" src crates macros
git log --oneline --all -- <path>
git blame -L <start>,<end> -- <path>
```

Check callers and callees, lock order, ownership and refcounts, allocation and
user-pointer error paths, HAL variants under `crates/eonix_hal`, and related
MM, VFS, scheduler, and syscall code. For this kernel, always consider:

- SMP: shared state, single-CPU assumptions, locks held across an await.
- Interrupts and preemption: check-then-act windows.
- Refcounts: who owns each reference and which path drops it.
- Page tables and folios: mapping, CoW, and flag updates.
- User memory: validation, faulting mid-copy.
- Architecture: does `riscv64` or `loongarch64` need the same change?
- ABI: syscall return values, errno, struct layout.

## Tools

Smallest tool that answers the next question: `build/qemu.log`, GDB (`make srun`
then `make debug`), `bt full` / `thread apply all bt` / `info registers`,
`trace_syscall` / `trace_scheduler` / `log_debug` features, and `SMP=1` vs
`SMP=4` comparison. Temporary prints are removed or feature-gated before commit.
Leave no QEMU or GDB running between phases.

## Hypotheses

One block per candidate in `analysis.md`: hypothesis, expected, actual, evidence
for, evidence against, falsification test. Prefer the explanation of the first
wrong state over the last visible symptom. Keep disproved hypotheses; they stop
the next agent repeating the work. Two live hypotheses → one `scout` each, no
duplicate prompts.

## Root cause

Complete with evidence before planning:

```text
Root cause: <mechanism> causes <failure> because <invariant violated>, shown by <log, backtrace, code path, or experiment>.
```

Then record the smallest code boundary, what must not change, the invariants to
preserve, and the configs that could regress. Unproven root cause → no phase 3.

## Reviewer challenge

Required, by a `reviewer` that did not do the analysis. Report to
`$ART/reports/02-analysis-reviewer.md`; verdict `supported`, `uncertain`, or
`contradicted`, plus contradicting evidence, any simpler explanation, a
falsification test, and unproven lifecycle or locking assumptions.

## `analysis.md`

```markdown
# Issue <id>: <title>

## Reported behavior
Expected: / Actual:

## Environment
commit, ARCH, MODE, SMP, QEMU/accelerator, extra image or user program

## Reproduction
script, command, baseline result, log path

## Observations
1. ...

## Hypotheses
| Hypothesis | For | Against | Test | Result |

## Root cause
## Fix boundary
must change / must not change / invariants / regression risks

## Open questions
```

---

Next: [04 — Fix](04-fix-and-commits.md)
