#!/bin/sh
# shellcheck source=./../../lib/lib.sh

copy_to_image user-programs/int.out int
copy_to_image user-programs/dynamic_test dynamic_test
copy_to_image user-programs/ld-musl-i386.so.1 ld-musl-i386.so.1
copy_to_image user-programs/pthread_test pthread_test
