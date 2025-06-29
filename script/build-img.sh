#!/bin/sh

OS=`uname -s`

if sudo --version > /dev/null 2>&1; then
    SUDO=sudo
fi

if [ "$OUTPUT" = "" ]; then
    OUTPUT="build/fs-$ARCH.img"
fi

if [ "$ARCH" = "" ]; then
    echo "ARCH is not set, exiting..." >&2
    exit 1
fi

dd if=/dev/zero of="$OUTPUT" bs=`expr 1024 \* 1024` count=1020
mkfs.fat -n SYSTEM "$OUTPUT"

if [ "$OS" = "Darwin" ]; then
    SUDO=''
    hdiutil detach build/mnt > /dev/null 2>&1 || true
    hdiutil attach "$OUTPUT" -mountpoint build/mnt
else
    mkdir -p build/mnt
    $SUDO losetup -P /dev/loop2 "$OUTPUT"
    $SUDO mount /dev/loop2 build/mnt
fi

if [ "$ARCH" = "x86_64" ]; then
    $SUDO cp ./user-programs/init.out build/mnt/init
    $SUDO cp ./user-programs/int.out build/mnt/int
    $SUDO cp ./user-programs/dynamic_test build/mnt/dynamic_test
    $SUDO cp ./user-programs/busybox build/mnt/busybox
    $SUDO cp ./user-programs/busybox-minimal build/mnt/busybox_
    $SUDO cp ./user-programs/ld-musl-i386.so.1 build/mnt/ld-musl-i386.so.1
    $SUDO cp ./user-programs/pthread_test build/mnt/pthread_test
    $SUDO cp ./user-programs/init_script_x86_64.sh build/mnt/initsh
elif [ "$ARCH" = "riscv64" ]; then
    $SUDO cp ./user-programs/busybox.static build/mnt/busybox
    $SUDO cp ./user-programs/init_script_riscv64.sh build/mnt/initsh
    $SUDO cp ./user-programs/libctest-static.sh build/mnt/libctest-static.sh
    $SUDO cp ./user-programs/libctest-dynamic.sh build/mnt/libctest-dynamic.sh
elif [ "$ARCH" = "loongarch64" ]; then
    $SUDO cp ./user-programs/busybox.la64 build/mnt/busybox
    $SUDO cp ./user-programs/init_script_loongarch64.sh build/mnt/initsh
    $SUDO cp ./user-programs/libctest-static.sh build/mnt/libctest-static.sh
    $SUDO cp ./user-programs/libctest-dynamic.sh build/mnt/libctest-dynamic.sh
fi

# Add your custom files here


# End of custom files

if [ "$OS" = "Darwin" ]; then
    hdiutil detach build/mnt
else
    $SUDO losetup -d /dev/loop2
    $SUDO umount build/mnt
fi
