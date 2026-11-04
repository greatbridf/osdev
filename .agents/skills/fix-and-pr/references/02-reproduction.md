# 02 — Reproduction

Phase 1. A `worker` writes the script; a fresh `reviewer` verifies it. The
supervisor does neither and does not accept the author's own result.

## Script requirements

`script/repro/issue-<id>-<slug>.exp`, `expect` or Python `pexpect`.

- `--expect bug|fixed` (required), `--arch`, `--mode`, `--smp`, `--timeout`,
  `--log`, `--help`.
- Validate arguments before launching QEMU.
- Run `make test-run` with the given `ARCH`, `MODE`, `SMP`, under a unique QEMU
  name so cleanup can target it alone.
- Wait for the `/ # ` prompt; fail hard if it never appears.
- Send the trigger commands, then classify: bug observed, fixed observed,
  panic, hang, exit.
- On every exit path kill only this QEMU instance — never `killall
  qemu-system-*` — and leave no orphan.
- Bounded log, no credentials, no home-directory paths.

| Exit | Meaning |
| --- | --- |
| 0 | Observation matched `--expect` |
| 1 | Observation contradicted `--expect` |
| 2 | Infrastructure failure: build, no prompt, missing tool, timeout before trigger |
| 3 | Usage error |
| 130 | Interrupted |

Log markers: `REPRO issue= expect=`, `REPRO booted`, `REPRO trigger-sent`,
`REPRO bug-observed|fixed-observed|timeout`, `REPRO result=pass|fail exit=`.

`script/test.sh` is a useful reference for prompt matching, but its global
`killall` cleanup is not acceptable.

## Worker brief

Objective, cwd/ref, and the script path; authority limited to that file; no
kernel, user-program, Makefile, or doc edits; no commits. Deliver the script,
the baseline command, the log path, exit code, and any flakiness. Report to
`$ART/reports/01-repro-worker.md`.

## Reviewer brief

Read-only except for the script itself, reporting every change. Steps:

1. Confirm the script tests the reported behavior, not a nearby proxy.
2. `--help` exits 0 and starts no QEMU.
3. Unmodified tree: `--expect bug` exits 0 with the trigger and a bug marker.
4. Unmodified tree: `--expect fixed` exits 1. Exit 0 means the script cannot
   tell bug from fixed.
5. Timeout behavior: no prompt → 2; trigger hang → bug observed.
6. `pgrep -fl 'qemu-system-.*<unique-name>'` is empty after each run.
7. Logs are bounded and free of credentials and local paths.
8. Timing-dependent bugs: repeat 3+ times and report the pass rate.

Report to `$ART/reports/01-repro-reviewer.md`: verdict, commands, exit codes,
log paths, cleanup result, flakiness, defects. If the script cannot separate bug
from fixed, return to the generator; do not proceed to analysis.

## Run it

Same script and flags throughout the workflow:

```bash
script/repro/issue-${ISSUE}-${SLUG}.exp --expect bug \
  --arch x86_64 --mode release --smp 4 --log "$ART/before.log"
script/repro/issue-${ISSUE}-${SLUG}.exp --expect fixed \
  --arch x86_64 --mode release --smp 4 --log "$ART/after.log"
```

If the script changes, re-run both cases and record why. Races need a matrix:
`SMP=1` and `SMP=4`, three runs each, all logs kept.

## If it will not reproduce

Report the commands, `ARCH`/`MODE`/`SMP`/accelerator, any feature branch, user
program, `IMG=`, or init script the issue needs, and the closest observed
behavior. Ask for the missing detail. Never weaken the assertion or test a
different behavior to manufacture a reproduction.

---

Next: [03 — Analysis](03-analysis.md)
