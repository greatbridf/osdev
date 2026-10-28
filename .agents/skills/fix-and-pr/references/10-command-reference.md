# 10 — Quick command reference

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

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
  "github/${BASE}..HEAD"   # every commit must print LLM
git log -1 --format=%B      # inspect the trailer block before pushing

# Verification
ARCH=x86_64 sh script/test.sh
make build MODE=release ARCH=riscv64
make build MODE=release ARCH=loongarch64

# PR
gh pr create \
  --repo greatbridf/osdev --base "$BASE" --head "$HEAD" \
  --title "$TITLE" --body-file "$ART/pr.md"
gh pr view "$PR" --json body --jq .body | grep -F 'Assisted-by: LLM'
```

---

Next: [11 — Prompt hygiene](11-prompt-hygiene.md)
