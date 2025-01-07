#!/bin/sh

OS=`uname -s`

dd if=/dev/zero of=build/fs.img bs=`expr 1024 \* 1024` count=512
mkfs.fat -n SYSTEM build/fs.img

if [ "$OS" = "Darwin" ]; then
    hdiutil detach build/mnt > /dev/null 2>&1 || true
    hdiutil attach build/fs.img -mountpoint build/mnt
else
    mkdir -p build/mnt
    sudo mount disk.img build/mnt
fi

cp build/user-space-program/hello-world.out build/mnt/hello
cp build/user-space-program/interrupt-test.out build/mnt/int
cp build/user-space-program/stack-test.out build/mnt/stack
cp build/user-space-program/init.out build/mnt/init
cp build/user-space-program/priv-test.out build/mnt/priv
cp ./busybox build/mnt/busybox
cp ./busybox-minimal build/mnt/busybox_
cp ./init_script.sh build/mnt/initsh

# Add your custom files here

cp -r $HOME/.local/i486-linux-musl-cross build/mnt/

# End of custom files

if [ "$OS" = "Darwin" ]; then
    hdiutil detach build/mnt
else
    sudo umount build/mnt
fi
