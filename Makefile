.PHONY: all
all:
	FDISK='/opt/homebrew/Cellar/util-linux/2.39.3/sbin/fdisk' OUT="Makefile.real" ./configure

	make -f Makefile.real \
		build/riscv64gc-unknown-none-elf/release/eonix_kernel \
		ARCH=riscv64 MODE=release

	make -f Makefile.real \
		build/loongarch64-unknown-none-softfloat/release/eonix_kernel \
		ARCH=loongarch64 MODE=release

	make -f Makefile.real build/boot-comp.img
	
	cp build/riscv64gc-unknown-none-elf/release/eonix_kernel \
		kernel-rv

	cp build/loongarch64-unknown-none-softfloat/release/eonix_kernel \
		kernel-la
	
	cp build/boot-comp.img disk.img
