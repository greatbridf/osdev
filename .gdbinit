source pretty-print.py
set pagination off
set print pretty on
set output-radix 16

symbol-file build/kernel.out
target remote:1234

layout src
b do_socket
c
