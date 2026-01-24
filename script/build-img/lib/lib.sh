#!/bin/sh

OS=$(uname -s)
SUDO=$(which sudo || :)

info() {
    echo "info : $1"
}

warn() {
    echo "warn : $1" >&2
}

error() {
    echo "fatal: $1" >&2
}

die() {
    error "$1" && exit 1
}

sudo() {
    "$SUDO" "$@"
}

copy_to_image() {
    _prefix=sudo
    [ "$OS" = Darwin ] && _prefix=

    $_prefix cp "$1" "$MOUNTPOINT/$2"
}

iter_files() {
    find -L "$1" -maxdepth 1 -type f
}

iter_files_sorted() {
    iter_files "$@" | sort
}
