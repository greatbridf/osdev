# 09 — Issue analysis template

Part of the GBOS/Eonix `fix-and-pr` skill. Read [00-operating-contract.md](00-operating-contract.md) for the phase gates and hard rules before using this document.

---

## Appendix A: Issue analysis record

Keep this in `build/work/issue-<id>/analysis.md`:

```markdown
# Issue <id>: <title>

## Reported behavior
Expected:
Actual:

## Environment
- Credential gate: `GH_TOKEN` present (never record the value)
- Commit/branch:
- ARCH:
- MODE:
- SMP:
- QEMU / accelerator:
- Extra image or user program:

## Reproduction
- Script:
- Command:
- Baseline result:
- Log:

## Timeline / observations
1. ...
2. ...

## Candidate hypotheses
| Hypothesis | Evidence for | Evidence against | Test | Result |
| --- | --- | --- | --- | --- |

## Root cause
<Specific mechanism and evidence.>

## Fix boundary
Must change:
Must not change:
Invariants:

## Open questions
- ...
```

---

Next: [10 — Quick command reference](10-command-reference.md)
