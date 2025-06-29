#!/mnt/busybox sh

BUSYBOX=/mnt/busybox

freeze() {
    echo "an error occurred while executing '''$@''', freezing..." >&2

    while true; do
        true
    done
}

do_or_freeze() {
    if $@; then
        return
    fi

    freeze $@
}

do_or_freeze $BUSYBOX mkdir -p /dev

do_or_freeze $BUSYBOX mknod -m 666 /dev/console c 5 1
do_or_freeze $BUSYBOX mknod -m 666 /dev/null c 1 3
do_or_freeze $BUSYBOX mknod -m 666 /dev/zero c 1 5
do_or_freeze $BUSYBOX mknod -m 666 /dev/vda b 8 0
do_or_freeze $BUSYBOX mknod -m 666 /dev/vdb b 8 16
do_or_freeze $BUSYBOX mknod -m 666 /dev/vdb1 b 8 17
do_or_freeze $BUSYBOX mknod -m 666 /dev/ttyS0 c 4 64
do_or_freeze $BUSYBOX mknod -m 666 /dev/ttyS1 c 4 65

echo -n -e "deploying busybox... " >&2

do_or_freeze $BUSYBOX mkdir -p /bin
do_or_freeze $BUSYBOX --install -s /bin
do_or_freeze $BUSYBOX mkdir -p /lib

export PATH="/bin"

echo ok >&2

do_or_freeze mkdir -p /etc /root /proc
do_or_freeze mount -t procfs proc proc

# Check if the device /dev/vda is available and can be read
if dd if=/dev/vda of=/dev/null bs=512 count=1; then
    echo -n -e "Mounting the ext4 image... " >&2
    do_or_freeze mkdir -p /mnt1
    do_or_freeze mount -t ext4 /dev/vda /mnt1
    echo ok >&2
fi

cat > /etc/passwd <<EOF
root:x:0:0:root:/root:/mnt/busybox sh
EOF

cat > /etc/group <<EOF
root:x:0:root
EOF

cat > /etc/profile <<EOF
export PATH=/bin
EOF

cat > /root/.profile <<EOF
export HOME=/root

alias ll="ls -l "
alias la="ls -la "
EOF

cat > /root/test.c <<EOF
#include <stdio.h>

int main() {
    int var = 0;
    printf("Hello, world!\n");
    printf("Please input a number: \n");
    scanf("%d", &var);
    if (var > 0) {
        printf("You typed a positive number.\n");
    } else if (var == 0 ) {
        printf("You input a zero.\n");
    } else {
        printf("You typed a negative number.\n");
    }
    return 0;
}
EOF

cp -r /mnt1/glibc/lib .
ln -s /mnt1/musl/lib/libc.so /lib/ld-musl-riscv64-sf.so.1

ln -s $BUSYBOX /busybox
ln -s $BUSYBOX /bin/busybox

print_wtf() {
    echo "#### OS COMP TEST GROUP START $1 ####"
    echo "#### OS COMP TEST GROUP END $1 ####"
}

### MUSL ###

mkdir /musl-tests
cd /musl-tests

ln -s $BUSYBOX ./busybox

cp -r /mnt1/musl/basic .

ln -s /mnt1/musl/busybox_cmd.txt .

ln -s /mnt1/musl/iozone .

ln -s /mnt1/musl/lua .
ln -s /mnt1/musl/test.sh .
ln -s /mnt/libctest-static.sh .
ln -s /mnt/libctest-dynamic.sh .

for item in `ls /mnt1/musl/*.lua`; do
    ln -s $item .
done

for item in `ls /mnt1/musl/*.exe`; do
    ln -s $item .
done

ln -s /mnt1/musl/iozone_testcode.sh .
ln -s /mnt1/musl/lua_testcode.sh .
ln -s /mnt1/musl/busybox_testcode.sh .
ln -s /mnt1/musl/basic_testcode.sh .

sh libctest-static.sh
sh libctest-dynamic.sh
sh iozone_testcode.sh
sh busybox_testcode.sh
sh basic_testcode.sh
sh lua_testcode.sh

print_wtf "cyclictest-musl"
print_wtf "iperf-musl"
print_wtf "libcbench-musl"
#print_wtf "libctest-musl"
print_wtf "lmbench-musl"
print_wtf "ltp-musl"
print_wtf "netperf-musl"
print_wtf "scene-musl"
print_wtf "unixbench-musl"

### END MUSL ###

cd /
mkdir glibc-tests
cd glibc-tests

ln -s $BUSYBOX ./busybox

cp -r /mnt1/glibc/basic .

ln -s /mnt1/glibc/busybox_cmd.txt .

ln -s /mnt1/glibc/iozone .

ln -s /mnt1/glibc/lua .
ln -s /mnt1/glibc/test.sh .

for item in `ls /mnt1/glibc/*.lua`; do
    ln -s $item .
done

ln -s /mnt1/glibc/iozone_testcode.sh .
ln -s /mnt1/glibc/lua_testcode.sh .
ln -s /mnt1/glibc/busybox_testcode.sh .
ln -s /mnt1/glibc/basic_testcode.sh .

sh iozone_testcode.sh
sh busybox_testcode.sh
sh basic_testcode.sh
sh lua_testcode.sh

print_wtf "cyclictest-glibc"
print_wtf "iperf-glibc"
print_wtf "libcbench-glibc"
print_wtf "libctest-glibc"
print_wtf "lmbench-glibc"
print_wtf "ltp-glibc"
print_wtf "netperf-glibc"
print_wtf "scene-glibc"
print_wtf "unixbench-glibc"
