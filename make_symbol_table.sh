#!/bin/sh

objdump -t ./build/kernel.out | dd of=./build/dump.txt

awk '($1 ~ /[0-9]/) && ($4 != "*ABS*") && ($4 != ".text.bootsect") && ($3 != ".text.bootsect") && ($4 != ".magicnumber") {print $1 " " $NF}' ./build/dump.txt | dd of=./build/kernel.sym

rm ./build/dump.txt
