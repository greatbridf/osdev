#!/mnt/busybox sh

BUSYBOX=/mnt/busybox
TERMINAL=/dev/ttyS0
VERBOSE=

error() {
    printf "\033[91merror: \033[0m%s\n" "$1" >&2
}

warn() {
    printf "\033[93mwarn : \033[0m%s\n" "$1" >&2
}

info() {
    printf "\033[92minfo : \033[0m%s\n" "$1" >&2
}

die() {
    error "$1" && freeze
}

freeze() {
    info "freezing..." >&2
    while true; do
        :
    done

    exit 1
}

unrecoverable() {
    die "unrecoverable error occurred. check the message above."
}

busybox() {
    $BUSYBOX "$@"
}

trap unrecoverable EXIT

set -euo pipefail

if [ -n "$VERBOSE" ]; then
    set -x
fi

busybox mkdir -p /dev

busybox mknod -m 666 /dev/console c 5 1
busybox mknod -m 666 /dev/null c 1 3
busybox mknod -m 666 /dev/zero c 1 5
busybox mknod -m 666 /dev/vda b 8 0
busybox mknod -m 666 /dev/vda1 b 8 1
busybox mknod -m 666 /dev/vdb b 8 16
busybox mknod -m 666 /dev/ttyS0 c 4 64
busybox mknod -m 666 /dev/ttyS1 c 4 65

exec < "$TERMINAL"
exec > "$TERMINAL" 2>&1

info "deploying busybox..."

busybox mkdir -p /bin /lib
busybox --install -s /bin

info "done"

export PATH="/bin"

mkdir -p /etc /root /proc
mount -t procfs proc proc

cat > /etc/passwd <<EOF
root:x:0:0:root:/root:/bin/sh
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

exec sh -l

# We don't have a working init yet, so we use busybox sh directly for now.
# exec /mnt/init /bin/sh -c 'exec sh -l < /dev/ttyS0 > /dev/ttyS0 2> /dev/ttyS0'
