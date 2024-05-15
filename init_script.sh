#!/mnt/busybox sh

BUSYBOX=/mnt/busybox
alias mkdir="$BUSYBOX mkdir "
alias mknod="$BUSYBOX mknod "
alias cat="$BUSYBOX cat "

mkdir -p /etc
mkdir -p /root
mkdir -p /dev

mknod -m 666 /dev/console c 2 0
mknod -m 666 /dev/null c 1 0
mknod -m 666 /dev/sda b 8 0
mknod -m 666 /dev/sda1 b 8 1

cat > /etc/passwd <<EOF
root:x:0:0:root:/root:/mnt/busybox sh
EOF

cat > /etc/group <<EOF
root:x:0:root
EOF

cat > /root/.profile <<EOF
export PATH=/mnt
export HOME=/root

export BUSYBOX=/mnt/busybox

alias ls="$BUSYBOX ls "
alias ll="$BUSYBOX ls -l "
alias la="$BUSYBOX ls -la "

alias cat="$BUSYBOX cat "
alias clear="$BUSYBOX clear "
EOF

exec /mnt/init /mnt/busybox sh -l \
    < /dev/console > /dev/console 2> /dev/console
