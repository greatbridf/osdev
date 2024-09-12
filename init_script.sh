#!/mnt/busybox sh

BUSYBOX=/mnt/busybox

$BUSYBOX mkdir -p /dev

$BUSYBOX mknod -m 666 /dev/console c 5 1
$BUSYBOX mknod -m 666 /dev/null c 1 3
$BUSYBOX mknod -m 666 /dev/zero c 1 5
$BUSYBOX mknod -m 666 /dev/sda b 8 0
$BUSYBOX mknod -m 666 /dev/sda1 b 8 1

echo -n -e "deploying busybox... " > /dev/console

$BUSYBOX mkdir -p /bin
$BUSYBOX --install -s /bin

export PATH="/bin"

echo ok > /dev/console

mkdir -p /etc /root /proc
mount -t procfs proc proc

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

exec /mnt/init /bin/sh -l \
    < /dev/console > /dev/console 2> /dev/console
