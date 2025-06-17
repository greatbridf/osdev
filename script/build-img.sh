#!/bin/sh

OS=`uname -s`
SUDO=sudo

dd if=/dev/zero of=build/fs.img bs=`expr 1024 \* 1024` count=512
mkfs.fat -n SYSTEM build/fs.img

if [ "$OS" = "Darwin" ]; then
    SUDO=''
    hdiutil detach build/mnt > /dev/null 2>&1 || true
    hdiutil attach build/fs.img -mountpoint build/mnt
else
    mkdir -p build/mnt
    $SUDO losetup -P /dev/loop2 build/fs.img
    $SUDO mount /dev/loop2 build/mnt
fi

$SUDO cp ./user-programs/init.out build/mnt/init
$SUDO cp ./user-programs/int.out build/mnt/int
$SUDO cp ./user-programs/dynamic_test build/mnt/dynamic_test
$SUDO cp ./user-programs/busybox build/mnt/busybox
$SUDO cp ./user-programs/busybox-minimal build/mnt/busybox_
$SUDO cp ./user-programs/ld-musl-i386.so.1 build/mnt/ld-musl-i386.so.1
$SUDO cp ./user-programs/pthread_test build/mnt/pthread_test
$SUDO cp ./init_script.sh build/mnt/initsh

# Add your custom files here


# End of custom files

if [ "$OS" = "Darwin" ]; then
    hdiutil detach build/mnt
else
    $SUDO losetup -d /dev/loop2
    $SUDO umount build/mnt
fi
