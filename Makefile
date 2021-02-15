.PHONY: run
run:
	-rm build/boot.img.lock
	-bochs -f bochs.conf

build/boot.vdi: build/boot.img
	VBoxManage convertfromraw $< $@ --format VDI
