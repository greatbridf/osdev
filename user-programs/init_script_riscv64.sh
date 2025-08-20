#!/mnt/busybox sh

set -x

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

do_or_freeze $BUSYBOX mkdir -p /tmp

do_or_freeze $BUSYBOX mkdir -p /dev

do_or_freeze $BUSYBOX mknod -m 666 /dev/console c 5 1
do_or_freeze $BUSYBOX mknod -m 666 /dev/null c 1 3
do_or_freeze $BUSYBOX mknod -m 666 /dev/zero c 1 5
do_or_freeze $BUSYBOX mknod -m 666 /dev/random c 1 8
do_or_freeze $BUSYBOX mknod -m 666 /dev/urandom c 1 9
do_or_freeze $BUSYBOX mknod -m 666 /dev/vda b 8 0
do_or_freeze $BUSYBOX mknod -m 666 /dev/vdb b 8 16
do_or_freeze $BUSYBOX mknod -m 666 /dev/vdb1 b 8 17
do_or_freeze $BUSYBOX mknod -m 666 /dev/ttyS0 c 4 64
do_or_freeze $BUSYBOX mknod -m 666 /dev/ttyS1 c 4 65

echo -n -e "deploying busybox... " >&2

do_or_freeze $BUSYBOX mkdir -p /bin
do_or_freeze $BUSYBOX --install -s /bin

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
export PATH="/bin:/usr/bin:\$PATH"

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

ln -s /mnt1/lib /lib
ln -s /mnt1/usr /usr

exec $BUSYBOX sh -l < /dev/ttyS0 > /dev/ttyS0 2> /dev/ttyS0
