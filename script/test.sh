#!/bin/sh

SUCCESS_MSG="###$RANDOM :SuCCeSS: $RANDOM###"

if [ "$ARCH" = "" ]; then
    echo "Error: ARCH environment variable is not set." >&2
    exit 1
fi

printf "ls\necho \"$SUCCESS_MSG\"\npoweroff\n" \
    | make test-run ARCH=$ARCH MODE=release QEMU=qemu-system-$ARCH \
    | tee build/test-$$.log \
    | grep "$SUCCESS_MSG" > /dev/null && echo TEST\ $$\ WITH\ ARCH=$ARCH\ PASSED
