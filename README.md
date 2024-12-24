# Eonix

A simple OS kernel written in C++ and Rust that aims to be Linux compatible out of the box.

- [x] Multitasking
- [x] Memory management
- [x] Filesystem implementation
- [x] ELF program loader
- [x] Some rather naive AHCI driver and FAT32 impl.
- [x] TTY and job control interface (partially working)
- [ ] Move to Rust (WIP)
- [ ] SMP support (WIP)
- [ ] POSIX thread support (WIP)
- [ ] Network stack (WIP)
- [ ] Dynamic loader support
- [ ] Users, permission...

# Build & Run

## Prerequisites

#### Compile

- [GCC (Tested)](https://gcc.gnu.org/) or
- [Clang (It should work, but haven't been tested for a while.)](https://clang.llvm.org/)

- [Rust](https://www.rust-lang.org/)
- [CMake](https://cmake.org/)

#### Generate disk image

- [(Optional) busybox (We have a prebuilt version of busybox in the project directory)](https://www.busybox.net/)
- [fdisk](https://www.gnu.org/software/fdisk/)
- [mtools](http://www.gnu.org/software/mtools/)

#### Debug and Run

- [GDB](https://www.gnu.org/software/gdb/)
- [QEMU](https://www.qemu.org/)

## Build and Run

```bash
./configure && make prepare && make build

make nativerun

# or if you need debugging

# 1:
make srun

# 2:
make debug
```

You may want to specify the correct version of build tools to use when running `./configure` in ENV.

- `QEMU`: QEMU used to debug run. `qemu-system-x86_64` by default.
- `GDB`: GDB used by `make debug`. We will search for `gdb` and `x86_64-elf-gdb` and check the supported archs by default.
- `FDISK_BIN`: fdisk executable used to create disk image part tables. `fdisk` by default.

If you are doing a cross compile, run `./configure` with `CROSS_COMPILE` set to the corresponding target triple of your cross compiler.

## Run your own program

The `user` directory under the project dir exists mainly for some *historical* reason and has almost no use. So don't ever try to look inside that.

To copy your program into the built disk image, you can edit the `CMakeLists.txt` and add a line to the `boot.img` section. You can also try editing the `init_script.sh` to customize the booting procedure.
