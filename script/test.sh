#!/bin/bash
#
# Boot the kernel under QEMU and drive a trivial smoke test through the serial
# console.
#
# A step fails when the kernel panics, when QEMU exits early, or when the
# expected output never arrives within TEST_TIMEOUT seconds. An expect timeout
# must not be a silent success: a kernel that prints nothing at all has to fail
# this test, not pass it.
#
# Environment:
#   ARCH          target architecture (required)
#   TEST_TIMEOUT  seconds to wait per step (default 120, TCG boot is slow)
#   LOGFILE       serial log path (default build/test-<pid>.log)

# `printf %q` and BASH_VERSION require bash; re-exec when invoked via `sh`.
if [ -z "${BASH_VERSION:-}" ]; then
    exec bash "$0" "$@"
fi

LOGFILE="${LOGFILE:-build/test-$$.log}"
TIMEOUT="${TEST_TIMEOUT:-120}"
SUCCESS_MSG="###$RANDOM :SuCCeSS: $RANDOM###"

die() {
    echo "error: $1" >&2
    exit 1
}

# Shared failure branches for every expect block below.
abort_branches() {
    cat <<EOF
        "*panicked" { exit 1 }
        eof         { exit 1 }
        timeout     { exit 1 }
EOF
}

runcmd() {
    _cmd=$(printf "%q" "$*")

    cat <<EOF
    expect {
        "*/ # " { send "$_cmd\n" }
$(abort_branches)
    }
EOF
}

wait_str() {
    cat <<EOF
    expect {
        "*$*" {}
$(abort_branches)
    }
EOF
}

wait_init_exit() {
    # The kernel reports the exit of init through a panic, so the panic text is
    # expected here; only the missing exit report is a failure.
    cat <<EOF
    expect {
        "*init exited:" { exit 0 }
        eof             { exit 1 }
        timeout         { exit 1 }
    }
EOF
}

cleanup() {
    killall -KILL "qemu-system-$ARCH" 2>/dev/null
}

[ -z "$ARCH" ] && die "ARCH environment variable is not set"
command -v expect > /dev/null || die "expect is not installed"

trap cleanup EXIT

# `tee` alone returns 0, so without pipefail a failed run would look successful.
set -o pipefail

expect <<EOF | tee "$LOGFILE"

set timeout $TIMEOUT
spawn make test-run "ARCH=$ARCH" MODE=release "QEMU=qemu-system-$ARCH"

$(runcmd ls -la)

$(wait_str proc)

$(runcmd echo \""$SUCCESS_MSG"\")

$(wait_str "\n$SUCCESS_MSG")

$(runcmd poweroff)

$(wait_init_exit)

EOF

status=$?

echo

# shellcheck disable=SC2181
if [ $status -ne 0 ]; then
    echo "=== Test $$ with ARCH=$ARCH failed (expect exit $status)"
    exit 1
fi

echo "=== Test $$ with ARCH=$ARCH passed"
