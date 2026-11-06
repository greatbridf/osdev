#!/bin/sh
#
# Point git at the version-controlled hooks in .githooks/.
#
# `core.hooksPath` lives in the repository config and cannot be committed, so
# every clone (and every linked worktree) has to opt in once. `./configure`
# calls this script; `make hooks` does the same explicitly.
#
# Only pre-commit is tracked today; git ignores missing hook files, so this is
# safe to extend later.

set -eu

if ! command -v git > /dev/null 2>&1; then
    echo "git hooks: git not found, skipping" >&2
    exit 0
fi

if ! git rev-parse --git-dir > /dev/null 2>&1; then
    echo "git hooks: not inside a git repository, skipping" >&2
    exit 0
fi

current="$(git config --get core.hooksPath || true)"
if [ "$current" = ".githooks" ]; then
    exit 0
fi

git config core.hooksPath .githooks
echo "git hooks: core.hooksPath set to .githooks"
