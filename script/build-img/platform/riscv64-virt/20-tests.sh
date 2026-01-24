#!/bin/sh

name="$(mktemp)"

cat > "$name" <<EOF
#!/bin/sh

forks=100

getpid() {
    echo "\$(exec sh -c 'echo \$PPID')"
}

for i in \$(seq \$forks); do
    ( getpid )
done

echo good
EOF

copy_to_image "$name" test-fork
