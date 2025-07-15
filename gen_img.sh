rm -rf ex4.img
dd if=/dev/zero of=ex4.img bs=1M count=8192
mkfs.ext4 -b 4096 ./ex4.img
sudo chown $USER:$USER ex4.img
