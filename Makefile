.PHONY: run
run:
	-bochs -f bochs.conf

build/boot.vdi: build/boot.img
	VBoxManage convertfromraw $< $@ --format VDI
