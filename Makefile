BUILD_DIR=build
MBR_NAME=$(BUILD_DIR)/mbr.bin
BOOT_IMAGE_NAME=$(BUILD_DIR)/boot.img

BOOT_SOURCE=src/boot.s

AS=as
CC=gcc
DD=dd
LD=ld

$(BOOT_IMAGE_NAME): $(MBR_NAME)
	$(DD) if=$(MBR_NAME) of=$(BOOT_IMAGE_NAME)
	$(DD) if=/dev/zero of=$(BOOT_IMAGE_NAME) seek=1 bs=512 count=2879

$(MBR_NAME): $(BUILD_DIR)/boot.o
	$(LD) -t ldscript.ld $(BUILD_DIR)/boot.o -o $(MBR_NAME) --oformat=binary

$(BUILD_DIR)/boot.o: $(BOOT_SOURCE)
	$(AS) $< -o $@

%.o: %.s
	$(AS) $< -o $(BUILD_DIR)/$@

%.o: %.c
	$(CC) -c $< -o $(BUILD_DIR)/$@

.PHONY: run
run: $(BOOT_IMAGE_NAME)
	-bochs -f bochs.conf

.PHONY: clean
clean:
	-rm $(BUILD_DIR)/*.o
	-rm $(MBR_NAME)
	-rm $(BOOT_IMAGE_NAME)
