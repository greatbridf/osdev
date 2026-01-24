#!/bin/bash

LOGFILE="build/test-$$.log"
SUCCESS_MSG="###$RANDOM :SuCCeSS: $RANDOM###"

die() {
    echo "error: $1" >&2
    exit 1
}

runcmd() {
    _cmd=$(printf "%q" "$*")

    cat <<EOF
    expect {
        "*/ # " { send "$_cmd\n" }
        "*panicked" { exit 1 }
    }
EOF
}

wait_str() {
    cat <<EOF
    expect {
        "*$*" {}
        "*panicked" { exit 1 }
    }
EOF
}

wait_init_exit() {
    echo 'expect "*init exited:" { exit 0 }'
}

cleanup() {
    killall -KILL "qemu-system-$ARCH"
}

[ -z "$ARCH" ] && die "ARCH environment variable is not set"

trap cleanup EXIT

expect <<EOF | tee "$LOGFILE"

set timeout 10
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
    echo "=== Test $$ with ARCH=$ARCH failed"
else
    echo "=== Test $$ with ARCH=$ARCH passed"
fi
