.PHONY: run
run: build
	qemu-system-x86_64 -drive file=build/boot.img,format=raw -display curses -no-reboot -no-shutdown -S -s -enable-kvm

.PHONY: build
build:
	cmake --build build --target boot.img

build/boot.vdi: build/boot.img
	-rm build/boot.vdi
	VBoxManage convertfromraw $< $@ --format VDI
