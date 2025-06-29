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
