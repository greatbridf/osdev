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

### glibc git test
mkdir -p glibc-test/usr/bin
ln -s /mnt1/glibc/usr/bin/git ./glibc-test/usr/bin
ln -s /mnt1/glibc/busybox ./glibc-test
ln -s /mnt1/glibc/git_testcode.sh ./glibc-test
cd glibc-test
sh ./git_testcode.sh

### musl git test
cd ..
mkdir -p musl-test/usr/bin
ln -s /mnt1/musl/usr/bin/git ./musl-test/usr/bin
ln -s /mnt1/musl/busybox ./musl-test
ln -s /mnt1/musl/git_testcode.sh ./musl-test
cd musl-test
sh ./git_testcode.sh
