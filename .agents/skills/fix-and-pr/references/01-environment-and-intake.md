# 01 — Environment and issue intake

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

---

## 1. Phase 0: Intake, environment, and target architecture

### 1.1 Credential gate and repository preflight

Before running any repository command, confirm the mandatory credential gate
from [00 — Operating contract](00-operating-contract.md) has passed:

```bash
test -n "${GH_TOKEN:-}"
```

If it fails, stop and ask the user to provide/export `GH_TOKEN`. Do not run the
commands below, spawn subagents, or start the workflow until it succeeds.

After the gate passes, run this repository preflight:

```bash
pwd
git status --short --branch
git remote -v
git branch --show-current
git log --oneline -10
```

Requirements:

- The checkout is on the intended base branch or a dedicated fix branch.
- There are no unintended local changes. Untracked workflow artifacts are fine
  only when they are known and will not be committed.
- `GH_TOKEN` is present in the environment and has not been written to the
  repository or any workflow artifact.
- The remote used for the PR is known:
  - `github` often points at the upstream mirror or fork.
  - `origin` may point at a private/self-hosted remote.
  - Do not assume which remote is the PR target; confirm it in Phase 5.

### 1.2 Build and runtime environment

The kernel primarily targets **x86_64** (default target
`x86_64-acpi_compat`), with cross-support for `riscv64` and `loongarch64`.

Key build parameters:

| Variable | Meaning | Default |
| --- | --- | --- |
| `ARCH` | Target architecture: `x86_64`, `riscv64`, `loongarch64` | `x86_64` |
| `MODE` | Build profile: `release` or `debug` | `debug` |
| `SMP` | Number of virtual CPUs; empty disables the `smp` feature | `4` |
| `FEATURES` | Comma-separated Cargo features, e.g. `log_debug,trace_syscall` | empty |
| `QEMU_ACCEL` | QEMU accelerator; `-accel tcg` on macOS, `-accel kvm` on Linux when available | auto |
| `DEBUG_TRAPS` | Enables QEMU trap logging to `build/qemu.log` | unset |

Configure and build:

```bash
./configure
make build MODE=release
```

If `./configure` cannot find a required tool, fix the environment before
continuing. `./configure` generates the `Makefile`; the generated file is
ignored by Git.

Interactive run:

```bash
make run MODE=release
```

Wait for the shell prompt (`/ # `), run a harmless command such as
`echo READY`, then exit QEMU with `Ctrl-A`, then `x`.

Headless/test run:

```bash
make test-run MODE=release ARCH=x86_64
```

`make test-run` uses `-serial stdio` and is the preferred target for automated
reproduction. Unlike `make run`, it does not multiplex the QEMU monitor into
the serial stream, so exit by terminating the QEMU process rather than
`Ctrl-A x`.

Debug run:

```bash
make srun MODE=release      # starts QEMU paused with GDB stub on :1234
make debug                  # attaches GDB and loads build/kernel.sym
# or, when tmux is available:
make tmux-debug
```

### 1.3 Create a fix branch

Use an isolated worktree when other work may continue in the main checkout:

```bash
git fetch github
git switch -c "fix/${ISSUE}-${SLUG}" "github/master"
# or:
git worktree add -b "fix/${ISSUE}-${SLUG}" "../eonix-${ISSUE}" "github/master"
```

If the issue targets a feature branch, branch from that branch instead of
`master`. Record the chosen base in `build/work/issue-<id>/plan.md`.

### 1.4 Capture the issue

Preferred, if `gh` is installed:

```bash
gh issue view "$ISSUE" \
  --repo greatbridf/osdev \
  --json number,title,body,labels,comments \
  > "$ART/issue.json"
```

`GH_TOKEN` is the standard environment variable read by the `gh` CLI and by the
REST fallback. Export it once for the workflow; do not write it to a file.

Fallback with the GitHub REST API:

```bash
curl -sS \
  -H "Authorization: Bearer ${GH_TOKEN}" \
  -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/greatbridf/osdev/issues/${ISSUE}" \
  > "$ART/issue.json"
```

Do not put the token in shell history or command output. If a command would
print it, redirect the output or redact the value.

Capture in `analysis.md`:

- Expected behavior vs. actual behavior.
- Reproducer commands, stack traces, logs, and screenshots referenced by the
  issue.
- Target architecture, `MODE`, `SMP`, and whether the bug is SMP-only.
- Whether the issue is a hang, panic, wrong result, memory corruption, race, or
  build failure.
- The commit or branch the reporter used, if known.

---

Next: [02 — Automated reproduction](02-reproduction.md)
