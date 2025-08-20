#!/bin/sh

addresses=`sed -n '/<<<<<<<<<< 8< CUT HERE 8< <<<<<<<<<</,$p' $1 | tail -n +2 | awk '{print $3}'`

for addr in $addresses; do
    riscv64-unknown-elf-addr2line \
        -e build/riscv64gc-unknown-none-elf/debug/eonix_kernel \
        -i $addr 2>/dev/null
done
