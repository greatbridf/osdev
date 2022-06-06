# disable kvm to debug triple faults
QEMU_ARGS=-drive file=build/boot.img,format=raw -no-reboot -no-shutdown -enable-kvm #-d cpu_reset,int
.PHONY: run
run: build
	qemu-system-i386 $(QEMU_ARGS) -display curses -S -s
.PHONY: srun
srun: build
	qemu-system-i386 $(QEMU_ARGS) -display none -S -s -serial mon:stdio
.PHONY: nativerun
nativerun: build
	qemu-system-i386 $(QEMU_ARGS) -display none -serial mon:stdio

.PHONY: configure
configure:
	cmake -Bbuild -DCMAKE_BUILD_TYPE=Debug
	cp build/compile_commands.json .

.PHONY: build
build:
	cmake --build build --target boot.img

.PHONY: clean
clean:
	-rm -rf build
	-rm compile_commands.json

.PHONY: debug
debug:
	gdb --symbols=build/kernel.out --init-eval-command 'set pagination off' --init-eval-command 'target remote:1234' --eval-command 'hbr kernel_main' --eval-command 'c'

build/boot.vdi: build/boot.img
	-rm build/boot.vdi
	VBoxManage convertfromraw $< $@ --format VDI
