# 01 — Intake

Phase 0. The preflight checks below are the supervisor's; build, branch, and
issue capture are `scout`'s.

## Preflight

Run by the supervisor, with no subagent and no report file: a decision it must
take itself before spending subagents, and one that produces no work product;
delegating it would only re-check what the supervisor has to decide anyway.

```bash
pwd; git status --short --branch; git remote -v
git branch --show-current; git log --oneline -10
```

Require: intended branch, no unexpected local changes, `GH_TOKEN` present and
never written to the repo, `github` remote identified. The base branch is
confirmed with the user; `create-pr` confirms the push remote later.

## Build and run

| Variable | Meaning | Default |
| --- | --- | --- |
| `ARCH` | `x86_64`, `riscv64`, `loongarch64` | `x86_64` |
| `MODE` | `release` or `debug` | `debug` |
| `SMP` | virtual CPUs; empty disables the `smp` feature | `4` |
| `FEATURES` | Cargo features, e.g. `log_debug,trace_syscall` | empty |
| `QEMU_ACCEL` | `tcg` on macOS, `kvm` on Linux when available | auto |
| `DEBUG_TRAPS` | trap logging to `build/qemu.log` | unset |

```bash
make build MODE=release
make test-run MODE=release ARCH=x86_64   # headless, serial stdio; preferred
make run MODE=release                    # interactive; exit with Ctrl-A then x
make srun MODE=release                   # paused QEMU, GDB stub on :1234
make debug                               # attach GDB, load build/kernel.sym
make tmux-debug                          # both, in tmux
```

`make test-run` exits by terminating QEMU, not with `Ctrl-A x`. The kernel
targets `x86_64-acpi_compat` by default.

## Branch

```bash
git fetch github
git switch -c "fix/${ISSUE}-${SLUG}" "github/master"
# or, isolated:
git worktree add -b "fix/${ISSUE}-${SLUG}" "../eonix-${ISSUE}" "github/master"
```

Feature-branch issues branch from that branch. Record the base in `plan.md`.

## Capture the issue

```bash
gh issue view "$ISSUE" --repo greatbridf/osdev \
  --json number,title,body,labels,comments > "$ART/issue.json"
```

REST fallback, token in the header only:

```bash
curl -sS -H "Authorization: Bearer ${GH_TOKEN}" \
  -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/greatbridf/osdev/issues/${ISSUE}" > "$ART/issue.json"
```

Seed `analysis.md` with expected vs actual behavior, reproducer commands and
logs, `ARCH`/`MODE`/`SMP`, failure class (hang, panic, wrong result,
corruption, race, build), and the reporter's commit if known.

---

Next: [02 — Reproduction](02-reproduction.md)
