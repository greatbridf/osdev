#!/bin/sh
# shellcheck source=./../../lib/lib.sh

copy_to_image user-programs/busybox.la64 busybox
copy_to_image user-programs/init_script_loongarch64-virt.sh initsh
