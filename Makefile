.PHONY: all
all:
	OUT="Makefile.real" ./configure

	make -f Makefile.real \
		build/riscv64gc-unknown-none-elf/release/eonix_kernel \
		ARCH=riscv64 MODE=release

	make -f Makefile.real \
		build/loongarch64-unknown-none-softfloat/release/eonix_kernel \
		ARCH=loongarch64 MODE=release
	
	make -f Makefile.real build/boot-riscv64.img
	make -f Makefile.real build/boot-loongarch64.img
	
	cp build/riscv64gc-unknown-none-elf/release/eonix_kernel \
		kernel-rv

	cp build/loongarch64-unknown-none-softfloat/release/eonix_kernel \
		kernel-la
	
	mv build/boot-riscv64.img disk.img
	mv build/boot-loongarch64.img disk-la.img

.PHONY: run-rv
run-rv:
	qemu-system-riscv64 -machine virt -kernel kernel-rv -m 1G -nographic -bios default \
		-drive file=sdcard-rv.img,if=none,format=raw,id=x0 \
		-device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 -no-reboot \
		-device virtio-net-device,netdev=net -netdev user,id=net -rtc base=utc \
		-drive file=disk.img,if=none,format=raw,id=x1 \
		-device virtio-blk-device,drive=x1,bus=virtio-mmio-bus.1

.PHONY: run-la
run-la:
	qemu-system-loongarch64 -kernel kernel-la -m 1G -nographic \
		-drive file=sdcard-la.img,if=none,format=raw,id=x0 \
		-device virtio-blk-pci,drive=x0 -no-reboot \
		-device virtio-net-pci,netdev=net0 -netdev user,id=net0 -rtc base=utc \
		-drive file=disk-la.img,if=none,format=raw,id=x1 \
		-device virtio-blk-pci,drive=x1
