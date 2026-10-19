# 02 — Automated reproduction

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

---

## 2. Phase 1: Automated reproduction

Manual interactive QEMU sessions easily hang terminals or leave orphaned
processes. Reproduce with a script, and verify the script itself before trusting
its result.

### 2.1 Spawn a subagent to generate the reproduction script

Spawn a `scout` or `worker` with a bounded, script-only mandate. The subagent
may inspect the repository, but must not edit kernel source.

#### Generator task template

```text
Objective
Create an automated, headless reproduction for GBOS/Eonix issue #<id>:
<issue title or one-line summary>.

Repository and target
- cwd: <repository path>
- ref/branch: <base branch or commit>
- target: ARCH=<x86_64|riscv64|loongarch64>, MODE=release, SMP=<n>
- issue evidence: <issue URL or $ART/issue.json>

Authority and boundaries
- You may create or edit only script/repro/issue-<id>-<slug>.exp.
- Do not edit kernel code, user programs, the Makefile, or repository docs.
- Do not commit.
- Do not print, copy, or commit any credential.

Required script interface
The script must accept:
  --expect bug|fixed    required; a default of "bug" is acceptable if documented
  --arch <arch>         default x86_64
  --mode <mode>         default release
  --smp <n>             default 4
  --timeout <seconds>   overall budget, default 180
  --log <path>          default build/work/issue-<id>/<timestamp>.log
  --help                print usage and exit without launching QEMU

Required behavior
1. Validate arguments before launching anything.
2. Launch the kernel with `make test-run` (not `make run`), using
   `ARCH`, `MODE`, and `SMP` from the arguments.
3. Give QEMU a unique name, e.g. `-name gbos-repro-<id>`, via QEMU_ARGS, so
   cleanup can target this instance only.
4. Wait for the busybox shell prompt `/ # ` and fail as infrastructure error
   if it never appears.
5. Send the exact command(s) that trigger the issue.
6. Detect success, wrong output, panic, hang, or QEMU exit with explicit
   timeouts. Treat a kernel panic or a timeout after the trigger as "bug
   observed", not as infrastructure failure.
7. On completion, terminate only the QEMU instance started by this script —
   never `killall qemu-system-*`. Kill the process group or use the unique
   QEMU name. Ensure no orphan remains.
8. Write a bounded log; do not let it grow without limit.
9. Exit with the contract below.

Exit-code contract
0  observed behavior matches --expect
1  observed behavior contradicts --expect
2  infrastructure failure: build failure, no prompt, missing tool, cleanup
   failure, or timeout before the trigger could be sent
3  usage/argument error
130 interrupted by the user

Required log markers
REPRO issue=<id> expect=<bug|fixed>
REPRO arch=<arch> mode=<mode> smp=<n>
REPRO booted
REPRO trigger-sent
REPRO bug-observed | REPRO fixed-observed | REPRO timeout
REPRO result=pass|fail exit=<code> log=<path>

Acceptance
- `--help` exits 0 without starting QEMU.
- On the unmodified tree, `--expect bug` exits 0 and the log shows the issue.
- On the unmodified tree, `--expect fixed` exits non-zero with a mismatch, not
  a false success.
- No QEMU process started by the script remains after any exit path.
- The script contains no credentials and no absolute paths tied to your home
  directory.

Deliverable
- The script path.
- The exact command used for a baseline run.
- The baseline log path and exit code.
- Any assumptions or flakiness observed.
```

Use `expect` or Python `pexpect`. Existing `script/test.sh` is a useful
reference for prompt matching, timeouts, and `poweroff`, but its global
`killall` cleanup is not acceptable for a reusable reproducer.

### 2.2 Spawn a subagent to verify the reproduction script

Do **not** immediately trust the generator. A broken script can "pass" for the
wrong reason or fail on infrastructure. The verifier must use a fresh context
and must not modify kernel code.

#### Verifier task template

```text
Objective
Independently verify that script/repro/issue-<id>-<slug>.exp is a trustworthy
reproduction for issue #<id>. Do not fix the kernel.

Repository and ref
- cwd: <repository path>
- ref: <unmodified base commit/branch>
- artifacts: build/work/issue-<id>/

Authority and boundaries
- Read-only for kernel code.
- You may edit only the reproduction script if it is defective, and you must
  report every change. Do not edit tests, Makefile, or kernel sources.
- Do not commit, push, or create a PR.
- Never print credentials.

Verification steps
1. Read the issue and the script. Confirm the script tests the reported behavior
   rather than an unrelated proxy.
2. Run `script/repro/issue-<id>-<slug>.exp --help`; confirm it exits 0 and starts
   no QEMU.
3. On the unmodified tree, run `--expect bug` in the documented configuration.
   Confirm exit 0 and that the log contains the trigger and a bug/timeout/panic
   marker.
4. Run the negative control on the same unmodified tree:
   `--expect fixed`. It must exit 1 (mismatch) and must not report a false
   success. If it exits 0, the script cannot distinguish bug from fixed.
5. Check the timeout behavior: the script must fail as infrastructure (2) if the
   prompt never appears, and report "bug observed" (0 for --expect bug) if the
   trigger hangs.
6. Check cleanup after every run:
   `pgrep -fl 'qemu-system-.*gbos-repro-<id>'` must return nothing.
7. Check that logs do not contain credentials, home-directory paths, or unbounded
   output.
8. If the bug is timing-dependent, repeat the baseline at least 3 times and
   report the pass rate. Do not hide flakiness.

Report
Verdict: PASS | FAIL
- Script path and exact commands run.
- Baseline --expect bug: exit code, log path.
- Negative control --expect fixed: exit code, log path.
- Cleanup check: command and result.
- Flakiness: number of runs and pass rate.
- Defects found; if you edited the script, show the diff and explain why.
- Remaining uncertainty.
```

If the verifier cannot make the script distinguish bug from fixed behavior,
return to the generator. Do not proceed to analysis with an ambiguous test.

### 2.3 Running and reusing the script

Use the same script and configuration throughout the workflow.

```bash
# On the unmodified tree: confirm the bug is present.
script/repro/issue-${ISSUE}-${SLUG}.exp \
  --expect bug \
  --arch x86_64 --mode release --smp 4 \
  --log "$ART/before.log"

# After the fix and rebuild: confirm the bug is gone.
script/repro/issue-${ISSUE}-${SLUG}.exp \
  --expect fixed \
  --arch x86_64 --mode release --smp 4 \
  --log "$ART/after.log"
```

Interpretation:

| Exit | Meaning |
| --- | --- |
| `0` | Observed behavior matched `--expect`; this is a successful run. |
| `1` | Behavior contradicted `--expect`; the fix is incomplete or the baseline did not reproduce. |
| `2` | Infrastructure failure; fix the environment before drawing conclusions. |
| `3` | Invalid invocation; fix the command. |

The exact same script and arguments must be used before and after the fix. If
the script must change, re-run both baseline and fixed cases with the new
version and record why it changed.

For SMP or race-related issues, run a matrix and keep all logs:

```bash
for smp in 1 4; do
  for i in 1 2 3; do
    script/repro/issue-${ISSUE}-${SLUG}.exp \
      --expect bug --smp "$smp" \
      --log "$ART/before-smp${smp}-run${i}.log"
  done
done
```

### 2.4 If the issue cannot be reproduced

Stop and report:

- The exact commands and environment tried.
- The arch, `MODE`, `SMP`, and QEMU accelerator used.
- Whether the issue depends on a feature branch, a user program, an additional
  disk image (`IMG=`), or a particular init script.
- The closest observed behavior and its log.

Ask the issue reporter or supervisor for missing details. Do not invent a
reproduction by weakening the assertion or by testing a different behavior.

---

Next: [03 — Root-cause analysis and debugging](03-analysis.md)
