#!/bin/sh

# $1: text to find
# $2: file extension
do_find()
{
    for ext in $2; do
        find src include -name "*.$ext" -exec grep -n -H -T -i "$1" {} \;
    done
}

do_find "$1" "c h cpp hpp s"
