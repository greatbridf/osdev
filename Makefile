.PHONY: run
run: build
	qemu-system-i386 -drive file=build/boot.img,format=raw -display curses -no-reboot -no-shutdown -S -s -enable-kvm

.PHONY: build
build:
	cmake --build build --target boot.img

.PHONY: debug
debug:
	gdb --symbols=build/kernel.out --init-eval-command 'target remote:1234' --eval-command 'hbr kernel_main' --eval-command 'c'

build/boot.vdi: build/boot.img
	-rm build/boot.vdi
	VBoxManage convertfromraw $< $@ --format VDI
