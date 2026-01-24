#!/bin/sh
# shellcheck source=./../../lib/lib.sh

copy_to_image user-programs/busybox.static busybox
copy_to_image user-programs/init_script_riscv64-virt.sh initsh
