#!/bin/sh
# shellcheck source=./../../lib/lib.sh

copy_to_image user-programs/init.out init
copy_to_image user-programs/busybox busybox
copy_to_image user-programs/busybox-minimal busybox_
copy_to_image user-programs/init_script_x86_64-acpi_compat.sh initsh
