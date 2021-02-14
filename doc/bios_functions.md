# BIOS Functions
## int 0x10

### print string
bp: string address
ah: 0x13 // write string
al: 0x01 // update cursor after printing
bh: 0x00 // page number?
bl: 0x0f // color
cx: $char_nums_to_print

### print char
ah: 0x0e // print char
al: $char_to_print
bh: 0x00 // page number?

## int 0x13

### read LBA
si: $read_data_pack_address
ah: 0x42 // read LBA
dl: $drive_number // $0x80 + \[drive_number\]

check error flag after performing interrupt
