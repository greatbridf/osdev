QEMU_ARGS=-drive file=build/boot.img,format=raw -no-reboot -no-shutdown -enable-kvm
.PHONY: run
run: build
	qemu-system-i386 $(QEMU_ARGS) -display curses -S -s
.PHONY: srun
srun: build
	qemu-system-i386 $(QEMU_ARGS) -display none -S -s -serial mon:stdio
.PHONY: nativerun
nativerun: build
	qemu-system-i386 $(QEMU_ARGS) -display none -serial mon:stdio

.PHONY: build
build:
	cmake --build build --target boot.img

.PHONY: debug
debug:
	gdb --symbols=build/kernel.out --init-eval-command 'target remote:1234' --eval-command 'hbr kernel_main' --eval-command 'c'

build/boot.vdi: build/boot.img
	-rm build/boot.vdi
	VBoxManage convertfromraw $< $@ --format VDI
