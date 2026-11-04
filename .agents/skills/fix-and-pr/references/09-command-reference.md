# 09 — Quick command reference

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

Publishing commands (`gh pr create`, PR validation) live in the
[create-pr](../../create-pr/references/04-publish-and-validate.md) skill.

---

## Appendix B: Quick command reference

```bash
# Mandatory credential gate
test -n "${GH_TOKEN:-}" || echo "Set GH_TOKEN before doing any work"

# Environment
./configure
make build MODE=release
make run MODE=release          # interactive; exit with Ctrl-A then x
make test-run MODE=release     # headless, serial stdio
make srun MODE=release         # paused QEMU + GDB stub
make debug                     # attach GDB
make tmux-debug                # both, in tmux
DEBUG_TRAPS=1 make test-run    # writes build/qemu.log

# Reproducer
script/repro/issue-<id>-<slug>.exp --expect bug   --log "$ART/before.log"
script/repro/issue-<id>-<slug>.exp --expect fixed --log "$ART/after.log"

# Diff and commit hygiene
git diff --check
git status --short
git diff --cached --name-only
git log --oneline "github/${BASE}..HEAD"
git log --format='%h %G? %s%n  %(trailers:key=Assisted-by,valueonly)' \
  "github/${BASE}..HEAD"   # agent-written commits print LLM; human ones print nothing
git log -1 --format=%B      # inspect the trailer block before pushing

# Verification
ARCH=x86_64 sh script/test.sh
make build MODE=release ARCH=riscv64
make build MODE=release ARCH=loongarch64

# Handoff (publishing is the create-pr skill)
test -n "${GH_TOKEN:-}" || echo "create-pr will need GH_TOKEN"
git log --oneline "github/${BASE}..HEAD"
```

---

Next: [10 — Prompt hygiene](10-prompt-hygiene.md)
