# disable kvm to debug triple faults
GDB_BIN=gdb
QEMU_BIN=qemu-system-i386
QEMU_ACCELERATION_FLAG=-enable-kvm
QEMU_DEBUG_FLAG=-d cpu_reset,int
QEMU_ARGS=-drive file=build/boot.img,format=raw -no-reboot -no-shutdown $(QEMU_ACCELERATION_FLAG) #$(QEMU_DEBUG_FLAG)
CROSS_COMPILE=#--toolchain cross-compile.cmake
.PHONY: run
run: build
	$(QEMU_BIN) $(QEMU_ARGS) -display curses -S -s
.PHONY: srun
srun: build
	$(QEMU_BIN) $(QEMU_ARGS) -display none -S -s -serial mon:stdio
.PHONY: nativerun
nativerun: build
	$(QEMU_BIN) $(QEMU_ARGS) -display none -serial mon:stdio

.PHONY: configure
configure:
	cmake -Bbuild -DCMAKE_BUILD_TYPE=Debug $(CROSS_COMPILE)
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
	$(GDB_BIN) --symbols=build/kernel.out --init-eval-command 'set pagination off' --init-eval-command 'target remote:1234' --eval-command 'hbr kernel_main' --eval-command 'c'

build/boot.vdi: build/boot.img
	-rm build/boot.vdi
	VBoxManage convertfromraw $< $@ --format VDI
