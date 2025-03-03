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

$SUDO cp build/user-space-program/hello-world.out build/mnt/hello
$SUDO cp build/user-space-program/interrupt-test.out build/mnt/int
$SUDO cp build/user-space-program/stack-test.out build/mnt/stack
$SUDO cp build/user-space-program/init.out build/mnt/init
$SUDO cp build/user-space-program/priv-test.out build/mnt/priv
$SUDO cp ./busybox build/mnt/busybox
$SUDO cp ./busybox-minimal build/mnt/busybox_
$SUDO cp ./init_script.sh build/mnt/initsh

# Add your custom files here

$SUDO cp -r $HOME/.local/i486-linux-musl-cross build/mnt/

# End of custom files

if [ "$OS" = "Darwin" ]; then
    hdiutil detach build/mnt
else
    $SUDO losetup -d /dev/loop2
    $SUDO umount build/mnt
fi
