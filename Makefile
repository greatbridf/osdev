.PHONY: all
all:
	OUT="Makefile.real" ./configure

	make -f Makefile.real \
		build/riscv64gc-unknown-none-elf/release/eonix_kernel \
		ARCH=riscv64 MODE=release

	make -f Makefile.real \
		build/loongarch64-unknown-none-softfloat/release/eonix_kernel \
		ARCH=loongarch64 MODE=release
	
	cp build/riscv64gc-unknown-none-elf/release/eonix_kernel \
		kernel-rv

	cp build/loongarch64-unknown-none-softfloat/release/eonix_kernel \
		kernel-la
	
	xz -k -d disk.img.xz >/dev/null 2>&1 || true
	cp disk.img disk-la.img
