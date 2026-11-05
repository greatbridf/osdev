# AGENTS.md

Guidance for agents working in this repository.

## What this is

Eonix — a monolithic, multi-processor, multi-architecture OS kernel written in
Rust (`no_std`, `no_main`, nightly-only). It exposes a Linux/POSIX-compatible
syscall interface, and boots a `busybox` userland under QEMU.

Supported targets:

| `ARCH`        | Target triple                            | Platform (default) |
| ------------- | ---------------------------------------- | ------------------ |
| `x86_64`      | `x86_64-unknown-none.json` (custom)      | `acpi_compat`      |
| `riscv64`     | `riscv64gc-unknown-none-elf`             | `virt`             |
| `loongarch64` | `loongarch64-unknown-none-softfloat`     | `virt`             |

`configure` also recognises `aarch64`, but there is no implementation for it.

## Toolchain and prerequisites

- `rust-toolchain` pins `nightly-2026-01-09`. Do not bump it casually.
- Required components: `rust-src`, `llvm-tools`. `cargo-binutils` is needed for
  `cargo objcopy` (used to build the boot images).
- QEMU (`qemu-system-<ARCH>`), plus `expect`, `dosfstools`, and `psmisc`
  (`killall`) for `script/test.sh` and image building.
- `.cargo/config.toml` sets `target-dir = build`, `force-unwind-tables`, and
  `build-std = core, compiler_builtins, alloc`. Builds therefore never land in
  `target/`, and `cargo` must be able to build the sysroot from `rust-src`.

## Build, run, test

`Makefile` is generated from `Makefile.src` by `configure` and is gitignored —
run `./configure` first (optionally `ARCH=riscv64 ./configure`), then use make.
Do not hand-edit the generated `Makefile`; edit `Makefile.src`.

```bash
./configure                        # defaults to ARCH=x86_64
make build          ARCH=x86_64 MODE=release
make run            ARCH=x86_64 MODE=release   # QEMU, serial on stdio
make srun           ARCH=x86_64                # -S -s, waits for gdb
make debug          ARCH=x86_64                # rust-gdb against build/kernel.sym

bash script/test.sh                            # headless smoke test (needs ARCH)
ARCH=riscv64 TEST_TIMEOUT=180 bash script/test.sh
```

Useful knobs: `MODE=debug|release`, `SMP=<n>` (defaults 4, adds `--features smp`),
`FEATURES=a,b`, `IMG=<path>` (extra disk), `QEMU=`, `DEBUG_TRAPS=1`
(writes `build/qemu.log`), `QEMU_ACCEL` (defaults to `kvm` on Linux with
`/dev/kvm`, otherwise `tcg`).

Notes that cost time if rediscovered:

- Every `make build` produces a kernel plus a FAT image; it is not a fast inner
  loop. Prefer `cargo build --profile dev` for type errors, and reserve full
  image builds/tests for verification.
- TCG boots are slow; CI allows `TEST_TIMEOUT=180` and a 15-minute step.
- `script/test.sh` drives the serial console with `expect` and fails on panic,
  early QEMU exit, or a missing expected string. An `expect` timeout is never a
  silent pass. It runs `ls -la`, checks for `proc`, echoes a random marker, then
  `poweroff`; `init` exiting is reported as a kernel panic and is expected.
- Never `killall qemu-system-*`; kill by the specific `qemu-system-$ARCH`.
- `make clean` wipes `build/`; `make clean-all` also removes `Makefile`.
- CI (`.github/workflows/test-build.yaml`) builds and smoke-tests all three
  architectures. `loongarch64` needs QEMU >= 9.0, which is why its leg uses
  `ubuntu-26.04`.

## Layout

```
src/                     kernel binary (lib.rs is the crate root)
  kernel/                process/task, vfs, mem, syscall, pcie, timers, user
  driver/                ahci, e1000e, virtio, serial, rtc, sbi console
  fs/  net/  sync/  rcu/ kernel_init.rs  panic.rs  path.rs
crates/                  in-repo library crates, all path dependencies
  eonix_hal/             per-arch HAL: arch/{x86_64,riscv64,loongarch64},
                         bootstrap, trap, link.x.in, eonix_hal_traits,
                         eonix_hal_macros
  eonix_mm/ eonix_sync/ eonix_runtime/ eonix_preempt/ eonix_percpu/
  eonix_log/ buddy_allocator/ slab_allocator/ arcref/ pointers/
  intrusive_list/ atomic_unique_refcell/ posix_types/
macros/                  eonix_macros proc macros
user-programs/           userland + per-platform init scripts
script/                  build-img/ and test.sh
doc/                     design docs (Chinese; code comments are English)
x86_64-unknown-none.json custom x86_64 target spec
```

Architecture-specific code belongs behind `cfg(target_arch = ...)` in
`eonix_hal`; add target-specific dependencies to the matching
`[target.'cfg(target_arch = "...")'.dependencies]` table rather than to the
shared list.

## Code conventions

- Formatting is enforced by `.rustfmt.toml`: `max_width = 80`, 4 spaces,
  `group_imports = "StdExternalCrate"`, `imports_granularity = "Module"`,
  `imports_layout = "Mixed"`, `hex_literal_case = "Preserve"`. Run
  `cargo fmt --all` before committing; `cargo fmt --all -- --check` is the
  non-mutating form. (`./pre-commit` in this tree is a staged-file rustfmt check
  that is *not* installed into git hooks.)
- Nightly features are used deliberately (`allocator_api`, `coerce_unsized`,
  `arbitrary_self_types`, `unsize`, ...). `#![feature(...)]` lives in
  `src/lib.rs` and the crate roots.
- `no_std` + `build-std`: no `std`, nothing that assumes a hosted runtime.
  Async code is executor-driven (`eonix_runtime`); avoid spinning or sleeping in
  kernel paths.
- Locking and lifetime rules matter more than style here: this is a preemptive,
  SMP kernel. Reads of shared state need the right `eonix_sync` primitive,
  refcounts (`arcref`, `pointers`) must be balanced on every error path, and
  lock order must stay consistent.
- Keep changes minimal and localized. Do not mix refactors, formatting sweeps,
  or dependency bumps into a functional change.
- Docs under `doc/` are Chinese; source comments and identifiers are English.
  Match the surrounding file's language and comment style.
- A `cargo build` for one arch does not prove the others compile; `eonix_hal`
  and driver changes usually need a build per affected target.

## Commits

Subject: `<subsystem>: <imperative>`, <= 72 characters, matching history
(`fix/futex: ...`, `mm: ...`, `vfs, inode: ...`). Body explains what failed and
when, the root cause, and how the fix restores the invariant, wrapped at 72
columns. Trailers in order: `Fixes:`/`Link:`, then
`Assisted-by: LLM` (only for agent-written commits), then `Signed-off-by:` last.

Stage explicit paths (`git add path/to/file.rs`), never `git add -A` or `git add .`.
Commit non-interactively with `git commit -F -` so trailers cannot be dropped.
No pushes, PRs, or merges unless explicitly asked.

Project skills in `.agents/skills/` describe the full issue → fix → PR flow:
`fix-and-pr` (reproduce, fix, verify, commit) and `create-pr` (publish).
