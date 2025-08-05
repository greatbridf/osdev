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
do_or_freeze $BUSYBOX mknod -m 666 /dev/vda1 b 8 1
do_or_freeze $BUSYBOX mknod -m 666 /dev/vdb b 8 16
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

# Check if the device /dev/vdb is available and can be read
if dd if=/dev/vdb of=/dev/null bs=512 count=1; then
    echo -n -e "Mounting the ext4 image... " >&2
    do_or_freeze mkdir -p /mnt1
    do_or_freeze mount -t ext4 /dev/vdb /mnt1
    echo ok >&2
fi

cp /mnt/ld-musl-i386.so.1 /lib/ld-musl-i386.so.1
ln -s /lib/ld-musl-i386.so.1 /lib/libc.so

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

exec $BUSYBOX sh -l < /dev/ttyS0 > /dev/ttyS0 2> /dev/ttyS0

# We don't have a working init yet, so we use busybox sh directly for now.
# exec /mnt/init /bin/sh -c 'exec sh -l < /dev/ttyS0 > /dev/ttyS0 2> /dev/ttyS0'
